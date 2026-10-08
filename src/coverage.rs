use std::net::IpAddr;

use crate::{Error, target::number};

const BUCKETS: usize = 4096;
const WORDS: usize = BUCKETS / 64;

/// Conservative IP coverage of one immutable reader, using 12 address bits.
///
/// False positives are possible; a false result guarantees an IP miss. ASN
/// assertions do not contribute. Derived coverage is not an on-disk format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    v4: [u64; WORDS],
    v6: [u64; WORDS],
}

impl Coverage {
    pub(crate) fn empty() -> Self {
        Self {
            v4: [0; WORDS],
            v6: [0; WORDS],
        }
    }

    /// Whether this reader could contain an assertion covering the address.
    pub fn may_contain(&self, address: IpAddr) -> bool {
        let (ipv6, bucket) = bucket(address);
        let words = if ipv6 { &self.v6 } else { &self.v4 };

        words
            .get(bucket / 64)
            .is_none_or(|word| word & (1 << (bucket % 64)) != 0)
    }

    pub(crate) fn insert(&mut self, start: u128, end: u128, ipv6: bool) {
        let shift = if ipv6 { 116 } else { 20 };
        let first = (start >> shift) as usize;
        let last = (end >> shift) as usize;
        let words = if ipv6 { &mut self.v6 } else { &mut self.v4 };

        for (index, word) in words
            .iter_mut()
            .enumerate()
            .take(last / 64 + 1)
            .skip(first / 64)
        {
            let low = if index == first / 64 { first % 64 } else { 0 };
            let high = if index == last / 64 { last % 64 } else { 63 };
            *word |= (u64::MAX << low) & (u64::MAX >> (63 - high));
        }
    }
}

/// Candidate reader positions for a fixed ordered set of at most 64 readers.
///
/// The table occupies 64 KiB, independent of assertion count. Keep it paired
/// with the readers used to construct it; replacing or reordering those readers
/// requires rebuilding the router. Candidates still need exact lookup.
#[derive(Debug)]
pub struct CoverageRouter {
    masks: Vec<u64>,
}

impl CoverageRouter {
    /// Build routing tables from coverage in reader order.
    ///
    /// # Errors
    /// Fails beyond 64 readers or if a memory reservation fails.
    pub fn new(coverage: &[Coverage]) -> Result<Self, Error> {
        if coverage.len() > 64 {
            return Err(Error::Limit("routed readers"));
        }

        let mut masks = Vec::new();
        masks.try_reserve_exact(BUCKETS * 2)?;
        masks.resize(BUCKETS * 2, 0);

        for (source, coverage) in coverage.iter().enumerate() {
            for (family, words) in [&coverage.v4, &coverage.v6].into_iter().enumerate() {
                for (word_index, &word) in words.iter().enumerate() {
                    let mut bits = word;
                    while bits != 0 {
                        let bit = bits.trailing_zeros() as usize;
                        let mask = masks
                            .get_mut(family * BUCKETS + word_index * 64 + bit)
                            .ok_or(Error::Invalid("coverage bucket"))?;
                        *mask |= 1 << source;
                        bits &= bits - 1;
                    }
                }
            }
        }

        Ok(Self { masks })
    }

    /// Yield every possible reader position in original order, without allocating.
    pub fn candidates(&self, address: IpAddr) -> impl Iterator<Item = usize> + use<> {
        candidates(self.mask(address))
    }

    pub(crate) fn mask(&self, address: IpAddr) -> u64 {
        let (ipv6, bucket) = bucket(address);
        self.masks
            .get(usize::from(ipv6) * BUCKETS + bucket)
            .copied()
            .unwrap_or(u64::MAX)
    }
}

pub(crate) fn candidates(mut mask: u64) -> impl Iterator<Item = usize> {
    std::iter::from_fn(move || {
        if mask == 0 {
            return None;
        }

        let position = mask.trailing_zeros() as usize;
        mask &= mask - 1;

        Some(position)
    })
}

fn bucket(address: IpAddr) -> (bool, usize) {
    let ipv6 = address.is_ipv6();
    let shift = if ipv6 { 116 } else { 20 };

    (ipv6, (number(address) >> shift) as usize)
}
