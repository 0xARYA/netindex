use crate::{Error, target::Entry};

const BITS: u8 = 12;
const BUCKETS: usize = 1 << BITS;
const PRESENT: u64 = 1 << 63;

#[derive(Debug)]
pub(crate) struct BroadRoutes {
    families: [Option<Vec<u64>>; 2],
}

impl BroadRoutes {
    pub(crate) fn empty() -> Self {
        Self {
            families: [None, None],
        }
    }

    // The caller admits entries only from a validated disjoint packed section.
    pub(crate) fn insert(&mut self, entry: Entry, ipv6: bool) -> Result<(), Error> {
        if entry.kind != 1 || entry.prefix > BITS {
            return Ok(());
        }

        let family = self
            .families
            .get_mut(usize::from(ipv6))
            .ok_or(Error::Invalid("broad route family"))?;
        if family.is_none() {
            let mut routes = Vec::new();
            routes.try_reserve_exact(BUCKETS)?;
            routes.resize(BUCKETS, 0);
            *family = Some(routes);
        }

        let routes = family.as_mut().ok_or(Error::Invalid("broad route table"))?;
        let shift = if ipv6 { 128 - BITS } else { 32 - BITS };
        let first = usize::try_from(entry.start >> shift)
            .map_err(|_| Error::Invalid("broad route bucket"))?;
        let count = 1usize << (BITS - entry.prefix);
        let end = first
            .checked_add(count)
            .ok_or(Error::Invalid("broad route bucket"))?;
        let slots = routes
            .get_mut(first..end)
            .ok_or(Error::Invalid("broad route bucket"))?;

        if slots.iter().any(|&slot| slot != 0) {
            return Err(Error::Invalid("overlapping broad routes"));
        }

        let route = PRESENT | u64::from(entry.id) | (u64::from(entry.prefix) << 32);
        slots.fill(route);

        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.families.iter().all(Option::is_none)
    }

    pub(crate) fn lookup(&self, query: u128, ipv6: bool) -> Result<Option<Entry>, Error> {
        let Some(routes) = self
            .families
            .get(usize::from(ipv6))
            .and_then(Option::as_ref)
        else {
            return Ok(None);
        };

        let shift = if ipv6 { 128 - BITS } else { 32 - BITS };
        let bucket =
            usize::try_from(query >> shift).map_err(|_| Error::Invalid("broad route query"))?;
        let route = *routes
            .get(bucket)
            .ok_or(Error::Invalid("broad route query"))?;

        if route == 0 {
            return Ok(None);
        }

        let prefix = ((route >> 32) & 0xff) as u8;
        let mask = if ipv6 {
            u128::MAX
        } else {
            u128::from(u32::MAX)
        };
        let host = mask >> prefix;
        let start = query & !host;
        let end = start | host;

        Ok(Some(Entry {
            start,
            end,
            maximum: end,
            id: route as u32,
            prefix,
            kind: 1,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use crate::Target;

    use super::*;

    #[test]
    fn default_network_routes_both_boundaries_and_all_record_ids() {
        let mut routes = BroadRoutes::empty();
        let target = Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        };

        routes
            .insert(target.entry(u32::MAX).unwrap().0, false)
            .unwrap();

        for query in [0, 1, u128::from(u32::MAX)] {
            let entry = routes.lookup(query, false).unwrap().unwrap();

            assert_eq!(entry.id, u32::MAX);
            assert_eq!(entry.target(false).unwrap(), target);
        }

        assert!(routes.lookup(0, true).unwrap().is_none());
    }

    #[test]
    fn ipv6_routes_preserve_adjacent_targets_and_leave_other_buckets_unrouted() {
        let mut routes = BroadRoutes::empty();
        let targets = [
            ("2000::", "200f:ffff:ffff:ffff:ffff:ffff:ffff:ffff", 7),
            ("2010::", "201f:ffff:ffff:ffff:ffff:ffff:ffff:ffff", 11),
        ];

        for (start, _, id) in targets {
            let target = Target::Network {
                address: start.parse().unwrap(),
                prefix: 12,
            };
            routes.insert(target.entry(id).unwrap().0, true).unwrap();
        }

        for (start, end, id) in targets {
            let target = Target::Network {
                address: start.parse().unwrap(),
                prefix: 12,
            };

            for address in [start, end] {
                let query = u128::from(address.parse::<Ipv6Addr>().unwrap());
                let entry = routes.lookup(query, true).unwrap().unwrap();

                assert_eq!(entry.id, id);
                assert_eq!(entry.target(true).unwrap(), target);
            }
        }

        let outside = u128::from("2020::".parse::<Ipv6Addr>().unwrap());

        assert!(routes.lookup(outside, true).unwrap().is_none());
        assert!(routes.lookup(0, false).unwrap().is_none());
    }

    #[test]
    fn ineligible_targets_leave_the_table_empty_and_conflicts_fail() {
        let mut routes = BroadRoutes::empty();
        let address: IpAddr = "203.0.113.0".parse().unwrap();
        let targets = [
            Target::Address(address),
            Target::Network {
                address,
                prefix: 24,
            },
            Target::Range {
                start: address,
                end: "203.0.113.255".parse().unwrap(),
            },
        ];

        for target in targets {
            routes.insert(target.entry(0).unwrap().0, false).unwrap();
        }

        assert!(routes.is_empty());

        let target = Target::Network {
            address: "192.0.0.0".parse().unwrap(),
            prefix: 8,
        };
        routes.insert(target.entry(1).unwrap().0, false).unwrap();
        let result = routes.insert(target.entry(2).unwrap().0, false);

        assert!(matches!(
            result,
            Err(Error::Invalid("overlapping broad routes"))
        ));
    }
}
