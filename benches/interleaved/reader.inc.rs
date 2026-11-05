// Experimental batch entry points exist only in the generated benchmark copy.
impl<B: AsRef<[u8]>> Reader<B> {
    pub fn study_asns<'a, const N: usize, E: From<Error>>(
        &'a self,
        queries: &[u32],
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if N == 0 || N > 16 {
            return Err(Error::Invalid("interleave lanes").into());
        }
        let bytes = self.bytes.as_ref();
        let section = self.layout.asns;
        let (rows, remainder) = slice(bytes, section.offset, section.length)?.as_chunks::<8>();
        if !remainder.is_empty() || rows.len() != section.count {
            return Err(Error::Invalid("ASN section length").into());
        }
        for (batch, queries) in queries.chunks(N).enumerate() {
            let mut low = [0usize; N];
            let mut high = [rows.len(); N];
            loop {
                let mut pending = false;
                for lane in 0..queries.len() {
                    if low[lane] < high[lane] {
                        pending = true;
                        let mid = low[lane] + (high[lane] - low[lane]) / 2;
                        let row = rows.get(mid).ok_or(Error::Invalid("ASN index position"))?;
                        if u32_at(row, 0)? < queries[lane] {
                            low[lane] = mid + 1;
                        } else {
                            high[lane] = mid;
                        }
                    }
                }
                if !pending {
                    break;
                }
            }
            for (lane, &number) in queries.iter().enumerate() {
                for row in rows
                    .get(low[lane]..)
                    .ok_or(Error::Invalid("ASN index position"))?
                {
                    if u32_at(row, 0)? != number {
                        break;
                    }
                    let id = u32_at(row, 4)?;
                    emit(
                        batch * N + lane,
                        Match {
                            id,
                            target: Target::Asn(number),
                            payload: self.layout.payload(bytes, id)?,
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn study_ips<'a, const N: usize, E: From<Error>>(
        &'a self,
        queries: &[IpAddr],
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        if N == 0 || N > 16 {
            return Err(Error::Invalid("interleave lanes").into());
        }
        for (batch, queries) in queries.chunks(N).enumerate() {
            let mut v4 = [None; N];
            let mut v6 = [None; N];
            for (lane, &address) in queries.iter().enumerate() {
                let family = usize::from(address.is_ipv6());
                let query = number(address);
                let (start, end) = self.bounds[family];
                if start <= query && query <= end {
                    if family == 0 {
                        v4[lane] = Some(query);
                    } else {
                        v6[lane] = Some(query);
                    }
                }
            }
            let mut found = [[None; N]; 2];
            for (family, section) in [self.layout.v4, self.layout.v6].into_iter().enumerate() {
                if section.encoding == Encoding::Packed && self.broad_routes.is_none() {
                    found[family] = packed::study_interleaved::<N>(
                        slice(self.bytes.as_ref(), section.offset, section.length)?,
                        section.count,
                        if family == 0 {
                            &v4[..queries.len()]
                        } else {
                            &v6[..queries.len()]
                        },
                        family == 1,
                        self.search,
                        if family == 0 {
                            self.ipv4_directory.as_ref()
                        } else {
                            None
                        },
                    )?;
                }
            }
            for (lane, &address) in queries.iter().enumerate() {
                let ipv6 = address.is_ipv6();
                let section = if ipv6 { self.layout.v6 } else { self.layout.v4 };
                if section.encoding == Encoding::Packed && self.broad_routes.is_none() {
                    if let Some(entry) = found[usize::from(ipv6)][lane] {
                        emit(
                            batch * N + lane,
                            Match {
                                id: entry.id,
                                target: entry.target(ipv6)?,
                                payload: self.layout.payload(self.bytes.as_ref(), entry.id)?,
                            },
                        )?;
                    }
                } else {
                    self.visit_ip(address, |row| emit(batch * N + lane, row))?;
                }
            }
        }
        Ok(())
    }
}
