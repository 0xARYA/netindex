//! Linux-only memory experiment, separate from the library and its dependencies.
use memmap2::{Mmap, MmapOptions, UncheckedAdvice};
use netindex::{Error, Limits, Reader, Target};
use std::{
    fs::File,
    hint::black_box,
    net::{Ipv4Addr, Ipv6Addr},
    os::fd::AsRawFd,
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5
        || !["warm", "cold", "pressure"].contains(&args[1].as_str())
        || !["v4", "v6", "asn"].contains(&args[3].as_str())
    {
        return Err("expected MODE(warm/cold/pressure) FILE FAMILY(v4/v6/asn) RECORDS".into());
    }
    let mode = &args[1];
    let family = &args[3];
    let records: u32 = args[4].parse()?;
    if records == 0 || records > 20_000_000 {
        return Err("record count out of range".into());
    }
    let file = File::open(&args[2])?;
    let opening = Instant::now();
    // SAFETY: the runner owns this immutable private copy until the child exits.
    let mapping = unsafe { MmapOptions::new().map(&file) }?;
    let reader = Reader::open(
        mapping.as_ref(),
        Limits {
            bytes: 2 * 1024 * 1024 * 1024,
            records: records as usize,
        },
    )?;
    let open_ms = opening.elapsed().as_secs_f64() * 1000.;
    // Check result content independently before changing residency.
    for index in 0..256u32 {
        let id = index.wrapping_mul(2_654_435_761) % records;
        let key = id * 2 + 2 + index % 2;
        let mut actual = Vec::new();
        visit(&reader, family, key, |row| {
            actual.push((row.id, row.target, row.payload.to_vec()));
            Ok(())
        })?;
        assert_eq!(actual.len(), usize::from(index % 2 == 0));
        if index % 2 == 0 {
            let expected = if family == "asn" {
                Target::Asn(key)
            } else {
                Target::Address(address(family, key))
            };
            assert_eq!(
                actual,
                vec![(id, expected, (id % 4096).to_le_bytes().to_vec())]
            );
        }
    }
    if mode == "warm" {
        for page in mapping.chunks(4096) {
            black_box(page[0]);
        }
    } else if mode == "cold" {
        // SAFETY: this is a read-only file mapping with immutable backing; contents
        // repopulate from the same bytes. No anonymous or modified page is discarded.
        unsafe { mapping.unchecked_advise(UncheckedAdvice::DontNeed) }?;
        // SAFETY: fd is live and valid; fadvise only changes caching for this file.
        let code =
            unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code).into());
        }
    }
    let resident_before = resident(&mapping)?;
    if mode == "warm" {
        assert_eq!(resident_before.0, resident_before.1);
    }
    println!(
        "{}",
        serde_json::json!({"phase":"opened","mode":mode,"family":family,
        "records":records,"artifact_bytes":mapping.len(),"open_ms":open_ms,
        "resident_pages_before_queries":resident_before.0,"total_pages":resident_before.1})
    );
    for pass in 0..4u32 {
        let before = faults()?;
        let start = Instant::now();
        let mut checksum = 0u64;
        let mut hits = 0;
        for offset in 0..8192u32 {
            let index = pass * 8192 + offset;
            let id = index.wrapping_mul(2_654_435_761) % records;
            let key = id * 2 + 2 + index % 2;
            let mut emitted = 0;
            visit(&reader, family, black_box(key), |row| {
                assert_eq!(row.id, id);
                assert_eq!(row.payload, (id % 4096).to_le_bytes());
                checksum = checksum.wrapping_add(u64::from(row.id));
                hits += 1;
                emitted += 1;
                Ok(())
            })?;
            assert_eq!(emitted, usize::from(index % 2 == 0));
        }
        let elapsed_ns = start.elapsed().as_nanos();
        let after = faults()?;
        assert_eq!(hits, 4096);
        println!(
            "{}",
            serde_json::json!({"phase":"queries","pass":pass,"queries":8192,"hits":hits,
            "elapsed_ns":elapsed_ns,"ns_per_query":elapsed_ns as f64/8192.,
            "minor_faults":after.0-before.0,"major_faults":after.1-before.1,"checksum":checksum})
        );
    }
    let resident_after = resident(&mapping)?;
    println!(
        "{}",
        serde_json::json!({"phase":"finished","resident_pages_after_queries":resident_after.0,
        "total_pages":resident_after.1})
    );
    Ok(())
}

fn address(family: &str, key: u32) -> std::net::IpAddr {
    if family == "v6" {
        Ipv6Addr::from((0x2001_0db8u128 << 96) | u128::from(key)).into()
    } else {
        Ipv4Addr::from(key).into()
    }
}
fn visit(
    reader: &Reader<&[u8]>,
    family: &str,
    key: u32,
    mut emit: impl FnMut(netindex::Match<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    if family == "asn" {
        reader.visit_asn(key, &mut emit)
    } else {
        reader.visit_ip(address(family, key), &mut emit)
    }
}
fn faults() -> Result<(u64, u64), Box<dyn std::error::Error>> {
    let bytes = std::fs::read_to_string("/proc/self/stat")?;
    let (_, rest) = bytes.rsplit_once(") ").ok_or("invalid process stat")?;
    let fields: Vec<_> = rest.split_whitespace().collect();
    Ok((
        fields.get(7).ok_or("missing minor faults")?.parse()?,
        fields.get(9).ok_or("missing major faults")?.parse()?,
    ))
}
fn resident(mapping: &Mmap) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    // SAFETY: sysconf has no pointer arguments; check its error return before use.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size <= 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut pages = vec![0u8; mapping.len().div_ceil(size as usize)];
    // SAFETY: the mapping is live/page-aligned and the kernel writes exactly one
    // byte per page into the correctly sized residency buffer.
    if unsafe {
        libc::mincore(
            mapping.as_ptr() as *mut libc::c_void,
            mapping.len(),
            pages.as_mut_ptr(),
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((
        pages.iter().filter(|&&page| page & 1 != 0).count(),
        pages.len(),
    ))
}
