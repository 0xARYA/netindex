use std::{fmt, net::IpAddr, sync::Arc};

use crate::{Coverage, CoverageRouter, Error, Match, Reader};

/// An immutable ordered set of at most 64 readers with IP coverage routing.
///
/// Source positions follow input order; record IDs remain local to each reader.
/// Visits preserve every assertion and borrow payloads without allocating output.
/// Schema decoding, provider precedence and release activation belong to the caller.
pub struct ReaderSet<B: AsRef<[u8]>> {
    readers: Vec<Arc<Reader<B>>>,
    coverage: Vec<Coverage>,
    router: Routing,
}

impl<B: AsRef<[u8]>> ReaderSet<B> {
    /// Compute coverage once and bind it to the supplied readers.
    ///
    /// # Errors
    /// Returns coverage-validation, allocation or reader-count limit errors.
    pub fn new(readers: Vec<Arc<Reader<B>>>) -> Result<Self, Error> {
        if readers.len() > 64 {
            return Err(Error::Limit("routed readers"));
        }

        let mut coverage = Vec::new();
        coverage.try_reserve_exact(readers.len())?;
        for reader in &readers {
            coverage.push(reader.coverage()?);
        }

        let router = Routing::Coarse(CoverageRouter::new(&coverage)?);

        Ok(Self {
            readers,
            coverage,
            router,
        })
    }

    /// Build a set with finer, 16-bit IP coverage routing.
    ///
    /// Uses a 1 MiB table instead of the default 64 KiB. This can exclude more
    /// candidate files but may cost more cache space; measure on your workload.
    ///
    /// # Errors
    /// Returns coverage-validation, allocation or reader-count limit errors.
    pub fn with_fine_routing(readers: Vec<Arc<Reader<B>>>) -> Result<Self, Error> {
        if readers.len() > 64 {
            return Err(Error::Limit("routed readers"));
        }

        let mut router = FineRouter::empty()?;
        for (position, reader) in readers.iter().enumerate() {
            router.insert(position, reader)?;
        }

        Ok(Self {
            readers,
            coverage: Vec::new(),
            router: Routing::Fine(router),
        })
    }

    /// Number of source readers.
    pub fn len(&self) -> usize {
        self.readers.len()
    }

    /// Whether the set contains no readers.
    pub fn is_empty(&self) -> bool {
        self.readers.is_empty()
    }

    /// Borrow a source reader by its position, or return `None` when absent.
    pub fn reader(&self, position: usize) -> Option<&Arc<Reader<B>>> {
        self.readers.get(position)
    }

    /// Visit every matching IP assertion, in source order then reader order.
    ///
    /// Coverage only excludes definite misses. ASN keys are not inferred from IPs.
    ///
    /// # Errors
    /// Stops on a reader or visitor error, preserving the visitor's error type.
    pub fn visit_ip<'a, E: From<Error>>(
        &'a self,
        address: IpAddr,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        for position in crate::coverage::candidates(self.router.mask(address)) {
            let reader = self
                .readers
                .get(position)
                .ok_or(Error::Invalid("routed reader position"))?;

            reader.visit_ip(address, |matched| emit(position, matched))?;
        }

        Ok(())
    }

    /// Visit every exact-ASN assertion across all readers in source order.
    ///
    /// IP coverage does not filter ASN lookups.
    ///
    /// # Errors
    /// Stops on a reader or visitor error, preserving the visitor's error type.
    pub fn visit_asn<'a, E: From<Error>>(
        &'a self,
        number: u32,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        for (position, reader) in self.readers.iter().enumerate() {
            reader.visit_asn(number, |matched| emit(position, matched))?;
        }

        Ok(())
    }

    /// Create a new set with one reader replaced, leaving this set usable.
    ///
    /// Shares unchanged readers and retains their routing coverage; scans only the replacement.
    /// The caller decides when to publish the returned set.
    ///
    /// # Errors
    /// Returns an invalid-position, coverage-validation or allocation error.
    pub fn replaced(&self, position: usize, reader: Arc<Reader<B>>) -> Result<Self, Error> {
        if position >= self.len() {
            return Err(Error::Invalid("reader position"));
        }

        let mut readers = Vec::new();
        readers.try_reserve_exact(self.len())?;
        readers.extend(self.readers.iter().enumerate().map(|(index, existing)| {
            if index == position {
                Arc::clone(&reader)
            } else {
                Arc::clone(existing)
            }
        }));

        let (coverage, router) = match &self.router {
            Routing::Coarse(_) => {
                let replacement = reader.coverage()?;

                let mut coverage = Vec::new();
                coverage.try_reserve_exact(self.len())?;
                coverage.extend(self.coverage.iter().enumerate().map(|(index, existing)| {
                    if index == position {
                        replacement.clone()
                    } else {
                        existing.clone()
                    }
                }));

                let router = Routing::Coarse(CoverageRouter::new(&coverage)?);

                (coverage, router)
            }
            Routing::Fine(existing) => {
                let mut router = FineRouter::empty()?;
                let keep = !(1u64 << position);
                for (output, &mask) in router.masks.iter_mut().zip(&existing.masks) {
                    *output = mask & keep;
                }

                router.insert(position, &reader)?;

                (Vec::new(), Routing::Fine(router))
            }
        };

        Ok(Self {
            readers,
            coverage,
            router,
        })
    }
}

impl<B: AsRef<[u8]>> fmt::Debug for ReaderSet<B> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReaderSet")
            .field("readers", &self.readers)
            .field("fine_routing", &matches!(self.router, Routing::Fine(_)))
            .finish()
    }
}

enum Routing {
    Coarse(CoverageRouter),
    Fine(FineRouter),
}

impl Routing {
    fn mask(&self, address: IpAddr) -> u64 {
        match self {
            Self::Coarse(router) => router.mask(address),
            Self::Fine(router) => {
                let bucket = fine_bucket(crate::target::number(address), address.is_ipv6());
                router.masks.get(bucket).copied().unwrap_or(u64::MAX)
            }
        }
    }
}

struct FineRouter {
    masks: Vec<u64>,
}

impl FineRouter {
    fn empty() -> Result<Self, Error> {
        let mut masks = Vec::new();
        masks.try_reserve_exact(2 * 65536)?;
        masks.resize(2 * 65536, 0);

        Ok(Self { masks })
    }

    fn insert<B: AsRef<[u8]>>(&mut self, position: usize, reader: &Reader<B>) -> Result<(), Error> {
        reader.visit_all(|matched| {
            if matches!(matched.target, crate::Target::Asn(_)) {
                return Ok(());
            }

            let (entry, ipv6) = matched.target.entry(matched.id)?;
            let first = fine_bucket(entry.start, ipv6);
            let last = fine_bucket(entry.end, ipv6);

            let masks = self
                .masks
                .get_mut(first..=last)
                .ok_or(Error::Invalid("coverage bucket"))?;
            for mask in masks {
                *mask |= 1u64 << position;
            }

            Ok::<_, Error>(())
        })
    }
}

fn fine_bucket(address: u128, ipv6: bool) -> usize {
    usize::from(ipv6) * 65536 + (address >> if ipv6 { 112 } else { 16 }) as usize
}
