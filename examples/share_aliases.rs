//! Fold explicitly declared, fully materialized IPv6 alias windows.
//! Usage: share_aliases INPUT OUTPUT IP/PREFIX,ID,STRIDE [...]

#[cfg(feature = "mmap")]
mod example {
    use netindex::{Builder, Error, Limits, MappedReader, Match, Reader, SharedAlias, Target};
    use std::{
        fs::{File, OpenOptions},
        net::{IpAddr, Ipv6Addr},
        time::Instant,
    };

    type Failure = Box<dyn std::error::Error>;

    fn parse(text: &str) -> Result<SharedAlias, Failure> {
        let (network, rest) = text.split_once(',').ok_or("missing ID and stride")?;
        let (id, stride) = rest.split_once(',').ok_or("missing stride")?;
        let (address, prefix) = network.split_once('/').ok_or("missing prefix")?;
        let interleaved = match stride {
            "1" => false,
            "2" => true,
            _ => return Err("stride must be 1 or 2".into()),
        };
        let alias = SharedAlias {
            address: address.parse()?,
            prefix: prefix.parse()?,
            first_id: id.parse()?,
            interleaved,
        };
        alias.project(Target::Network {
            address: "0.0.0.0".parse()?,
            prefix: 0,
        })?;
        Ok(alias)
    }

    fn ipv6_start(row: Match<'_>) -> Option<u128> {
        let address = match row.target {
            Target::Address(address) | Target::Network { address, .. } => address,
            Target::Range { start, .. } => start,
            Target::Asn(_) => return None,
        };
        if let IpAddr::V6(address) = address {
            Some(address.into())
        } else {
            None
        }
    }

    fn window_at(windows: &[SharedAlias], value: u128) -> Option<usize> {
        windows.iter().position(|window| {
            let start = u128::from(window.address);
            let end = start | u128::MAX.checked_shr(u32::from(window.prefix)).unwrap_or(0);
            (start..=end).contains(&value)
        })
    }

    #[expect(clippy::print_stdout, reason = "report replay results")]
    pub(super) fn run() -> Result<(), Failure> {
        let mut args = std::env::args().skip(1);
        let source = args.next().ok_or("missing input file")?;
        let destination = args.next().ok_or("missing output file")?;
        let mut windows = args.map(|arg| parse(&arg)).collect::<Result<Vec<_>, _>>()?;
        windows.sort_unstable_by_key(|window| window.address);
        let limits = Limits {
            records: 30_000_000,
            bytes: 2 << 30,
        };
        let file = File::open(source)?;
        // SAFETY: input must remain immutable throughout the replay.
        let original = unsafe { MappedReader::map_file(&file, limits) }?;
        let started = Instant::now();
        let mut rows = Vec::new();
        let mut counts = vec![0usize; windows.len()];
        let mut source_count = 0;
        original.visit_all(|row| -> Result<(), Failure> {
            if let Some(value) = ipv6_start(row)
                && let Some(at) = window_at(&windows, value)
            {
                let index = counts.get_mut(at).ok_or("missing count")?;
                let window = *windows.get(at).ok_or("missing window")?;
                let source: &Match<'_> = rows
                    .get(*index)
                    .filter(|_| *index < source_count)
                    .ok_or("extra alias row")?;
                let id = u32::try_from(*index)?
                    .checked_mul(if window.interleaved { 2 } else { 1 })
                    .and_then(|delta| window.first_id.checked_add(delta))
                    .ok_or("alias ID overflow")?;
                if row.id != id
                    || row.target != window.project(source.target)?
                    || row.payload != source.payload
                {
                    return Err(format!("alias mismatch at ID {}", row.id).into());
                }
                *index += 1;
            } else {
                if matches!(
                    row.target,
                    Target::Network {
                        address: IpAddr::V4(_),
                        ..
                    }
                ) {
                    let stride = if windows.iter().any(|window| window.interleaved) {
                        2
                    } else {
                        1
                    };
                    if row.id as usize != source_count * stride {
                        return Err("unsupported source ID order".into());
                    }
                    source_count += 1;
                }
                rows.try_reserve(1)?;
                rows.push(row);
            }
            Ok(())
        })?;
        if counts.iter().any(|&count| count != source_count) {
            return Err("incomplete alias window".into());
        }
        rows.sort_unstable_by_key(|row| row.id);
        let mut builder = Builder::new(limits);
        for row in &rows {
            builder.push(row.target, row.payload)?;
        }
        let base = Reader::open(builder.into_bytes(original.metadata()?)?, limits)?;
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        base.write_shared_aliases(&windows, &mut out, limits)?;
        let file_bytes = out.metadata()?.len();
        drop(out);
        let build_ms = started.elapsed().as_secs_f64() * 1_000.0;

        // SAFETY: this new private artifact stays immutable during verification.
        let compact = unsafe { MappedReader::map_file(&File::open(destination)?, limits) }?;
        assert_eq!(original.metadata()?, compact.metadata()?);
        assert_eq!(original.len(), compact.len());
        let mut seen = vec![false; compact.len()];
        compact.visit_all(|row| -> Result<(), Failure> {
            let seen = seen
                .get_mut(row.id as usize)
                .ok_or("unexpected output ID")?;
            assert!(!*seen);
            *seen = true;
            if let Some(at) = ipv6_start(row).and_then(|value| window_at(&windows, value)) {
                let window = windows.get(at).ok_or("missing window")?;
                let delta = row
                    .id
                    .checked_sub(window.first_id)
                    .ok_or("invalid alias ID")?;
                let stride = if window.interleaved { 2 } else { 1 };
                assert_eq!(delta % stride, 0);
                let source = rows
                    .get((delta / stride) as usize)
                    .filter(|_| ((delta / stride) as usize) < source_count)
                    .ok_or("invalid source ID")?;
                assert_eq!(row.target, window.project(source.target)?);
                assert_eq!(row.payload, source.payload);
            } else {
                let at = rows
                    .binary_search_by_key(&row.id, |row| row.id)
                    .map_err(|_| "missing base ID")?;
                assert_eq!(Some(&row), rows.get(at));
            }
            Ok(())
        })?;
        assert!(seen.into_iter().all(|value| value));

        let mut random = 0xd1b5_4a32_d192_ed03u64;
        let mut queries = 0usize;
        for _ in 0..25_000 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let ipv4 = random as u32;
            let mut check = |ip| -> Result<(), Error> {
                assert_eq!(original.lookup_ip(ip)?, compact.lookup_ip(ip)?);
                queries += 1;
                Ok(())
            };
            check(std::net::Ipv4Addr::from(ipv4).into())?;
            check(
                Ipv6Addr::from((u128::from(random) << 64) | u128::from(random.rotate_left(29)))
                    .into(),
            )?;
            for window in &windows {
                let trailing = 96 - window.prefix;
                let low = u128::from(random) & ((1u128 << trailing) - 1);
                check(
                    Ipv6Addr::from(
                        u128::from(window.address) | (u128::from(ipv4) << trailing) | low,
                    )
                    .into(),
                )?;
            }
        }
        println!(
            "{}",
            serde_json::json!({"logical_records":compact.len(),"physical_records":rows.len(),"shared_rows":counts,"file_bytes":file_bytes,"build_ms":build_ms,"verified_queries":queries})
        );
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "mmap")]
    {
        example::run()
    }
    #[cfg(not(feature = "mmap"))]
    {
        Err("enable mmap".into())
    }
}
