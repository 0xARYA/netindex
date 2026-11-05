impl<B: AsRef<[u8]>> Reader<B> {
    pub fn study_prepare(&self) -> Result<[usize; 9], Error> {
        use crate::study_eytzinger::{Cache, Tree};
        if self.study_cache.get().is_none() {
            let fences = |section: Section| {
                if section.encoding == Encoding::Packed {
                    section.count.div_ceil(packed::BLOCK)
                } else {
                    0
                }
            };
            let counts = [
                self.layout.asns.count,
                fences(self.layout.v4),
                fences(self.layout.v6),
            ];
            let bytes = counts[0]
                .checked_mul(12)
                .and_then(|a| {
                    counts[1]
                        .checked_add(counts[2])
                        .and_then(|n| n.checked_mul(36))
                        .and_then(|b| a.checked_add(b))
                })
                .and_then(|n| n.checked_add(48))
                .ok_or(Error::Limit("research cache bytes"))?;
            if bytes > 512 * 1024 * 1024 {
                return Err(Error::Limit("research cache bytes"));
            }
            let mut asns = Vec::new();
            asns.try_reserve_exact(counts[0])?;
            for index in 0..counts[0] {
                asns.push(u32_at(
                    self.bytes.as_ref(),
                    self.layout.asns.offset + index * 8,
                )?);
            }
            let ip = |section: Section, count: usize| -> Result<Tree<u128>, Error> {
                let mut keys = Vec::new();
                keys.try_reserve_exact(count)?;
                for index in 0..count {
                    keys.push(crate::layout::u128_at(
                        self.bytes.as_ref(),
                        section.offset + index * packed::FENCE,
                    )?);
                }
                Tree::new(keys)
            };
            let cache = Cache {
                asns: Tree::new(asns)?,
                v4: ip(self.layout.v4, counts[1])?,
                v6: ip(self.layout.v6, counts[2])?,
            };
            self.study_cache
                .set(cache)
                .map_err(|_| Error::Invalid("research cache already initialized"))?;
        }
        let cache = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
        let a = cache.asns.allocation_bytes();
        let b = cache.v4.allocation_bytes();
        let c = cache.v6.allocation_bytes();
        Ok([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]])
    }

    pub fn study_layout_asns<'a, E: From<Error>>(
        &'a self,
        queries: &[u32],
        method: usize,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if method > 2 {
            return Err(Error::Invalid("research layout method").into());
        }
        let cache = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
        let bytes = self.bytes.as_ref();
        let section = self.layout.asns;
        let rows = slice(bytes, section.offset, section.length)?
            .as_chunks::<8>()
            .0;
        for (query, &number) in queries.iter().enumerate() {
            let low = cache.asns.bound(number, false, method);
            for row in &rows[low..] {
                if u32_at(row, 0)? != number {
                    break;
                }
                let id = u32_at(row, 4)?;
                emit(
                    query,
                    Match {
                        id,
                        target: Target::Asn(number),
                        payload: self.layout.payload(bytes, id)?,
                    },
                )?;
            }
        }
        Ok(())
    }

    pub fn study_layout_ips<'a, E: From<Error>>(
        &'a self,
        queries: &[IpAddr],
        method: usize,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if method > 2 {
            return Err(Error::Invalid("research layout method").into());
        }
        let cache = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
        for (query, &address) in queries.iter().enumerate() {
            let family = usize::from(address.is_ipv6());
            let section = if family == 0 {
                self.layout.v4
            } else {
                self.layout.v6
            };
            if section.encoding != Encoding::Packed || self.broad_routes.is_some() {
                self.visit_ip(address, |row| emit(query, row))?;
                continue;
            }
            let value = number(address);
            let (lo, hi) = self.bounds[family];
            if value < lo || value > hi {
                continue;
            }
            let tree = if family == 0 { &cache.v4 } else { &cache.v6 };
            let upper = tree.bound(value, true, method);
            if let Some(index) = upper.checked_sub(1)
                && let Some(entry) = packed::study_at_block(
                    slice(self.bytes.as_ref(), section.offset, section.length)?,
                    section.count,
                    index,
                    value,
                    family == 1,
                    self.search,
                )?
            {
                emit(
                    query,
                    Match {
                        id: entry.id,
                        target: entry.target(family == 1)?,
                        payload: self.layout.payload(self.bytes.as_ref(), entry.id)?,
                    },
                )?;
            }
        }
        Ok(())
    }
}
