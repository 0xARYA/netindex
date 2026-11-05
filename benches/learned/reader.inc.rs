fn learned_model<K: crate::study_learned::Key>(
    count: usize,
    epsilon: usize,
    remaining: &mut usize,
    mut key: impl FnMut(usize) -> Result<K, Error>,
) -> Result<Option<crate::study_learned::Model<K>>, Error> {
    let bytes = count
        .checked_mul(std::mem::size_of::<K>())
        .ok_or(Error::Limit("learned key bytes"))?;
    if count == 0 || bytes > *remaining {
        return Ok(None);
    }
    let mut keys = Vec::new();
    keys.try_reserve_exact(count)?;
    for i in 0..count {
        keys.push(key(i)?);
    }
    let model = crate::study_learned::Model::new(keys, epsilon, *remaining)?;
    *remaining -= model.allocation_bytes().iter().sum::<usize>();
    Ok(Some(model))
}

impl<B: AsRef<[u8]>> Reader<B> {
    pub fn study_prepare(&self, epsilon: usize) -> Result<([usize; 9], [[usize; 3]; 3]), Error> {
        use crate::study_learned::{BUDGET, Cache};
        if ![4, 32, 128].contains(&epsilon) {
            return Err(Error::Invalid("learned epsilon"));
        }
        if let Some(c) = self.study_cache.get() {
            if c.epsilon != epsilon {
                return Err(Error::Invalid("learned epsilon changed"));
            }
            return Ok((c.allocation_bytes(), c.dimensions()));
        }
        let fences = |s: Section| {
            if s.encoding == Encoding::Packed && self.broad_routes.is_none() {
                s.count.div_ceil(packed::BLOCK)
            } else {
                0
            }
        };
        let bytes = self.bytes.as_ref();
        let mut remaining = BUDGET;
        let v4 = learned_model(fences(self.layout.v4), epsilon, &mut remaining, |i| {
            u32::try_from(crate::layout::u128_at(
                bytes,
                self.layout.v4.offset + i * packed::FENCE,
            )?)
            .map_err(|_| Error::Invalid("learned IPv4 width"))
        })?;
        let n6 = fences(self.layout.v6);
        let v6 = if n6 > 0 {
            let base = crate::layout::u128_at(bytes, self.layout.v6.offset)?;
            let end =
                crate::layout::u128_at(bytes, self.layout.v6.offset + (n6 - 1) * packed::FENCE)?;
            if end - base <= u128::from(u64::MAX) {
                learned_model(n6, epsilon, &mut remaining, |i| {
                    u64::try_from(
                        crate::layout::u128_at(bytes, self.layout.v6.offset + i * packed::FENCE)?
                            - base,
                    )
                    .map_err(|_| Error::Invalid("learned IPv6 span"))
                })?
                .map(|m| (base, m))
            } else {
                None
            }
        } else {
            None
        };
        let asns = learned_model(self.layout.asns.count, epsilon, &mut remaining, |i| {
            u32_at(bytes, self.layout.asns.offset + i * 8)
        })?;
        let cache = Cache {
            epsilon,
            asns,
            v4,
            v6,
        };
        let allocated = cache.allocation_bytes();
        let dimensions = cache.dimensions();
        if allocated.iter().sum::<usize>() > BUDGET {
            return Err(Error::Limit("learned aggregate capacity"));
        }
        self.study_cache
            .set(cache)
            .map_err(|_| Error::Invalid("learned cache initialized"))?;
        Ok((allocated, dimensions))
    }

    pub fn study_learned_asns<'a, E: From<Error>>(
        &'a self,
        queries: &[u32],
        method: usize,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if method > 3 {
            return Err(Error::Invalid("learned method").into());
        }
        let cache = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("learned missing cache"))?;
        let Some(model) = &cache.asns else {
            for (q, &n) in queries.iter().enumerate() {
                self.visit_asn(n, |row| emit(q, row))?;
            }
            return Ok(());
        };
        let bytes = self.bytes.as_ref();
        let rows = slice(bytes, self.layout.asns.offset, self.layout.asns.length)?
            .as_chunks::<8>()
            .0;
        for (q, &n) in queries.iter().enumerate() {
            let low = if n == 0 {
                0
            } else {
                model.search(u64::from(n - 1), method).index
            };
            for row in &rows[low..] {
                if u32_at(row, 0)? != n {
                    break;
                }
                let id = u32_at(row, 4)?;
                emit(
                    q,
                    Match {
                        id,
                        target: Target::Asn(n),
                        payload: self.layout.payload(bytes, id)?,
                    },
                )?;
            }
        }
        Ok(())
    }

    pub fn study_learned_ips<'a, E: From<Error>>(
        &'a self,
        queries: &[IpAddr],
        method: usize,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if method > 3 {
            return Err(Error::Invalid("learned method").into());
        }
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("learned missing cache"))?;
        for (q, &ip) in queries.iter().enumerate() {
            let family = usize::from(ip.is_ipv6());
            let section = if family == 0 {
                self.layout.v4
            } else {
                self.layout.v6
            };
            if section.encoding != Encoding::Packed
                || self.broad_routes.is_some()
                || (family == 0 && c.v4.is_none())
                || (family == 1 && c.v6.is_none())
            {
                self.visit_ip(ip, |row| emit(q, row))?;
                continue;
            }
            let value = number(ip);
            if value < self.bounds[family].0 || value > self.bounds[family].1 {
                continue;
            }
            let upper = if family == 0 {
                c.v4.as_ref()
                    .ok_or(Error::Invalid("learned v4 cache"))?
                    .search(value as u64, method)
                    .index
            } else {
                let (base, m) = c.v6.as_ref().ok_or(Error::Invalid("learned v6 cache"))?;
                match value.checked_sub(*base) {
                    None => 0,
                    Some(delta) => {
                        m.search(u64::try_from(delta).unwrap_or(u64::MAX), method)
                            .index
                    }
                }
            };
            if let Some(block) = upper.checked_sub(1)
                && let Some(entry) = packed::study_at_block(
                    slice(self.bytes.as_ref(), section.offset, section.length)?,
                    section.count,
                    block,
                    value,
                    family == 1,
                    self.search,
                )?
            {
                emit(
                    q,
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

    pub fn study_probe_asns(&self, queries: &[u32], method: usize) -> Result<[usize; 4], Error> {
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("learned missing cache"))?;
        let mut stats = [0; 4];
        if let Some(m) = &c.asns {
            for &n in queries {
                record_probe(&mut stats, m.search(u64::from(n.saturating_sub(1)), method));
            }
        }
        Ok(stats)
    }
    pub fn study_probe_ips(&self, queries: &[IpAddr], method: usize) -> Result<[usize; 4], Error> {
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("learned missing cache"))?;
        let mut stats = [0; 4];
        for &ip in queries {
            let family = usize::from(ip.is_ipv6());
            let value = number(ip);
            if value < self.bounds[family].0 || value > self.bounds[family].1 {
                continue;
            }
            if family == 0 {
                if let Some(m) = &c.v4 {
                    record_probe(&mut stats, m.search(value as u64, method));
                }
            } else if let Some((base, m)) = &c.v6
                && let Some(delta) = value.checked_sub(*base)
            {
                record_probe(
                    &mut stats,
                    m.search(u64::try_from(delta).unwrap_or(u64::MAX), method),
                );
            }
        }
        Ok(stats)
    }
}
fn record_probe(stats: &mut [usize; 4], bound: crate::study_learned::Bound) {
    stats[0] += 1;
    stats[1] += usize::from(bound.fallback);
    stats[2] += bound.window;
    stats[3] = stats[3].max(bound.window);
}

#[cfg(test)]
mod learned_reader_tests {
    use super::*;
    use crate::Builder;
    #[test]
    fn cache_identity_and_duplicate_emission_preserve_query_major_borrowed_results() {
        let mut b = Builder::new(Limits::default());
        for n in [1, 3, 3, 7] {
            b.push(Target::Asn(n), &[n as u8]).unwrap();
        }
        let bytes = b.into_bytes(&[]).unwrap();
        let r = Reader::open(bytes, Limits::default()).unwrap();
        assert!(matches!(r.study_prepare(0), Err(Error::Invalid(_))));
        let built = r.study_prepare(4).unwrap();
        assert_eq!(built, r.study_prepare(4).unwrap());
        assert!(matches!(r.study_prepare(32), Err(Error::Invalid(_))));
        let q = [0, 1, 3, 7, 8, u32::MAX];
        let mut want = Vec::new();
        for (i, &n) in q.iter().enumerate() {
            r.visit_asn(n, |m| {
                want.push((i, m.id, m.payload.as_ptr()));
                Ok::<_, Error>(())
            })
            .unwrap();
        }
        for method in 0..=3 {
            let mut got = Vec::new();
            r.study_learned_asns(&q, method, |i, m| {
                got.push((i, m.id, m.payload.as_ptr()));
                Ok::<_, Error>(())
            })
            .unwrap();
            assert_eq!(got, want);
        }
    }
}
