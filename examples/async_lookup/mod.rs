use std::net::{IpAddr, Ipv4Addr};

use netindex::{Builder, Error, Limits, Reader, Target};

pub(super) const MAX_OUTSTANDING: usize = 2;
const COPIES_PER_BATCH: usize = 256;
const BATCHES: usize = 4;

pub(super) fn fixture_reader() -> Result<Reader<Vec<u8>>, Error> {
    let mut builder = Builder::new(Limits::default());
    builder.push(
        Target::Network {
            address: Ipv4Addr::new(203, 0, 113, 0).into(),
            prefix: 24,
        },
        b"network evidence",
    )?;
    builder.push(
        Target::Address(Ipv4Addr::new(203, 0, 113, 42).into()),
        b"address evidence",
    )?;

    let bytes = builder.into_bytes(b"example release")?;

    Reader::open(bytes, Limits::default())
}

pub(super) fn fixture_batches() -> Vec<Vec<IpAddr>> {
    let addresses = [
        Ipv4Addr::new(203, 0, 113, 42).into(),
        Ipv4Addr::new(203, 0, 113, 1).into(),
        Ipv4Addr::new(198, 51, 100, 1).into(),
    ];

    (0..BATCHES)
        .map(|_| addresses.repeat(COPIES_PER_BATCH))
        .collect()
}

pub(super) fn check_fixture_batch(
    reader: &Reader<Vec<u8>>,
    addresses: &[IpAddr],
) -> Result<usize, Error> {
    let mut count = 0;
    for &address in addresses {
        reader.visit_ip(address, |_| {
            count += 1;
            Ok::<_, Error>(())
        })?;
    }

    // In each copy of the fixture, one IP has two matches, one has one, and one has none.
    if count != 3 * COPIES_PER_BATCH {
        return Err(Error::Invalid("example batch match count"));
    }

    Ok(count)
}
