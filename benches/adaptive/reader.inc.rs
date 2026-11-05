impl<B: AsRef<[u8]>> Reader<B> {
    pub fn study_prepare(&self, budget: usize) -> Result<[usize; 5], Error> {
        use crate::study_adaptive::{Cache, MAX_BUDGET, admit};
        use crate::study_flat_btree::Tree;
        if budget > MAX_BUDGET {
            return Err(Error::Limit("research cache budget"));
        }
        if let Some(cache) = self.study_cache.get() {
            if cache.budget != budget {
                return Err(Error::Invalid("research cache budget changed"));
            }
            return Ok(cache.allocation_bytes());
        }
        let fences = |s: Section| {
            if s.encoding == Encoding::Packed && self.broad_routes.is_none() {
                s.count.div_ceil(packed::BLOCK)
            } else {
                0
            }
        };
        let counts = [
            self.layout.asns.count,
            fences(self.layout.v4),
            fences(self.layout.v6),
        ];
        let mut remaining = budget;
        let cost4 = counts[1]
            .checked_mul(4)
            .ok_or(Error::Limit("research IPv4 bytes"))?;
        let cost6 = counts[2]
            .checked_mul(16)
            .ok_or(Error::Limit("research IPv6 bytes"))?;
        let use4 = admit(&mut remaining, cost4);
        let use6 = admit(&mut remaining, cost6);
        let use_asn = admit(&mut remaining, Tree::<u32>::requested_bytes(counts[0])?);
        let v4 = if use4 {
            let mut keys = Vec::new();
            keys.try_reserve_exact(counts[1])?;
            for i in 0..counts[1] {
                keys.push(
                    u32::try_from(crate::layout::u128_at(
                        self.bytes.as_ref(),
                        self.layout.v4.offset + i * packed::FENCE,
                    )?)
                    .map_err(|_| Error::Invalid("research IPv4 fence width"))?,
                );
            }
            Some(keys)
        } else {
            None
        };
        let v6 = if use6 {
            let mut keys = Vec::new();
            keys.try_reserve_exact(counts[2])?;
            for i in 0..counts[2] {
                keys.push(crate::layout::u128_at(
                    self.bytes.as_ref(),
                    self.layout.v6.offset + i * packed::FENCE,
                )?);
            }
            Some(keys)
        } else {
            None
        };
        let asns = if use_asn {
            let mut keys = Vec::new();
            keys.try_reserve_exact(counts[0])?;
            for i in 0..counts[0] {
                keys.push(u32_at(
                    self.bytes.as_ref(),
                    self.layout.asns.offset + i * 8,
                )?);
            }
            Some(Tree::new(keys)?)
        } else {
            None
        };
        let cache = Cache {
            budget,
            asns,
            v4,
            v6,
        };
        let bytes = cache.allocation_bytes();
        if bytes
            .iter()
            .try_fold(0usize, |a, &b| a.checked_add(b))
            .is_none_or(|n| n > budget)
        {
            return Err(Error::Limit("research cache capacity"));
        }
        self.study_cache
            .set(cache)
            .map_err(|_| Error::Invalid("research cache already initialized"))?;
        Ok(bytes)
    }

    pub fn study_plan_asns(&self, queries: &[u32]) -> Result<usize, Error> {
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
        if queries.len() < 32 {
            return Ok(0);
        }
        let sample = &queries[..32];
        let span = sample
            .iter()
            .max()
            .zip(sample.iter().min())
            .map(|(&hi, &lo)| u128::from(hi - lo));
        Ok(crate::study_adaptive::choose(
            queries.len(),
            span,
            c.asns.is_some(),
            false,
        ))
    }

    pub fn study_plan_ips(&self, queries: &[IpAddr]) -> Result<usize, Error> {
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
        if queries.len() < 32 {
            return Ok(0);
        }
        let Some(first) = queries.first() else {
            return Ok(0);
        };
        let family = first.is_ipv6();
        let mut lo = number(*first);
        let mut hi = lo;
        for &q in queries.iter().take(32) {
            if q.is_ipv6() != family {
                return Ok(0);
            }
            lo = lo.min(number(q));
            hi = hi.max(number(q));
        }
        Ok(crate::study_adaptive::choose(
            queries.len(),
            Some(hi - lo),
            if family {
                c.v6.is_some()
            } else {
                c.v4.is_some()
            },
            !family,
        ))
    }

    pub fn study_cached_asns<'a, E: From<Error>>(
        &'a self,
        queries: &[u32],
        method: usize,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if method > 4 || method == 3 {
            return Err(Error::Invalid("research cache method").into());
        }
        let plan = if method == 2 {
            self.study_plan_asns(queries)?
        } else {
            600 + method
        };
        if plan == 108 {
            return self.study_vector_asns::<8, E>(queries, true, emit);
        }
        if plan == 0 {
            for (q, &n) in queries.iter().enumerate() {
                self.visit_asn(n, |row| emit(q, row))?;
            }
            return Ok(());
        }
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
        let bytes = self.bytes.as_ref();
        let rows = slice(bytes, self.layout.asns.offset, self.layout.asns.length)?
            .as_chunks::<8>()
            .0;
        for (q, &n) in queries.iter().enumerate() {
            let Some(tree) = &c.asns else {
                self.visit_asn(n, |row| emit(q, row))?;
                continue;
            };
            let low = tree.bound_specialized(n, false, plan != 601, method != 4);
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

    pub fn study_cached_ips<'a, E: From<Error>>(
        &'a self,
        queries: &[IpAddr],
        method: usize,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if method > 4 || method == 3 {
            return Err(Error::Invalid("research cache method").into());
        }
        let plan = if method == 2 {
            self.study_plan_ips(queries)?
        } else {
            600 + method
        };
        if plan == 108 {
            return self.study_vector_ips::<8, E>(queries, true, emit);
        }
        if plan == 0 {
            for (q, &ip) in queries.iter().enumerate() {
                self.visit_ip(ip, |row| emit(q, row))?;
            }
            return Ok(());
        }
        let c = self
            .study_cache
            .get()
            .ok_or(Error::Invalid("research cache missing"))?;
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
                let keys = c.v4.as_ref().ok_or(Error::Invalid("research IPv4 cache"))?;
                let (low, high) = if plan == 601 {
                    self.ipv4_directory
                        .as_ref()
                        .map(|d| d.study_range(value))
                        .transpose()?
                        .unwrap_or((0, keys.len()))
                } else {
                    (0, keys.len())
                };
                let rows = keys
                    .get(low..high)
                    .ok_or(Error::Invalid("research directory range"))?;
                low + rows.partition_point(|&k| u128::from(k) <= value)
            } else {
                c.v6.as_ref()
                    .ok_or(Error::Invalid("research IPv6 cache"))?
                    .partition_point(|&k| k <= value)
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
}

#[cfg(test)]
mod adaptive_reader_tests {
    use super::*;
    use crate::Builder;

    #[test]
    fn cache_budget_rejection_identity_and_exact_admission_preserve_visits() {
        let mut builder = Builder::new(Limits::default());
        for n in [1, 3, 3, 7] {
            builder.push(Target::Asn(n), &[n as u8]).unwrap();
        }
        let bytes = builder.into_bytes(&[]).unwrap();
        for budget in [0, 15, 16, 17, 128] {
            let reader = Reader::open(bytes.clone(), Limits::default()).unwrap();
            let allocated = reader.study_prepare(budget).unwrap();
            assert!(allocated.iter().sum::<usize>() <= budget);
            assert_eq!(reader.study_prepare(budget).unwrap(), allocated);
            assert!(matches!(
                reader.study_prepare(budget + 1),
                Err(Error::Invalid(_))
            ));
            let mut expected = Vec::new();
            for (q, n) in [0, 1, 3, 7, 8].into_iter().enumerate() {
                reader
                    .visit_asn(n, |row| {
                        expected.push((q, row.id));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
            }
            for method in [0, 1, 2, 4] {
                let mut actual = Vec::new();
                reader
                    .study_cached_asns(&[0, 1, 3, 7, 8], method, |q, row| {
                        actual.push((q, row.id));
                        Ok::<_, Error>(())
                    })
                    .unwrap();
                assert_eq!(actual, expected);
            }
            assert_eq!(allocated[0] > 0, budget >= 16);
        }
        let reader = Reader::open(bytes, Limits::default()).unwrap();
        assert!(matches!(
            reader.study_prepare(usize::MAX),
            Err(Error::Limit(_))
        ));
        assert!(reader.study_prepare(0).is_ok());
    }
}
