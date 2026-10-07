use std::net::{IpAddr, Ipv4Addr};

use netindex::{Builder, Error, Limits, Reader, Target};

pub(super) fn reader() -> Result<Reader<Vec<u8>>, Error> {
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

    let mut bytes = Vec::new();
    builder.write_to(&mut bytes, b"example release")?;

    Reader::open(bytes, Limits::default())
}

pub(super) fn batches() -> Vec<Vec<IpAddr>> {
    let addresses = [
        Ipv4Addr::new(203, 0, 113, 42).into(),
        Ipv4Addr::new(203, 0, 113, 1).into(),
        Ipv4Addr::new(198, 51, 100, 1).into(),
    ];

    (0..4).map(|_| addresses.repeat(256)).collect()
}

pub(super) fn lookup_batch(reader: &Reader<Vec<u8>>, addresses: &[IpAddr]) -> Result<usize, Error> {
    let mut count = 0;
    for &address in addresses {
        reader.visit_ip(address, |_| {
            count += 1;
            Ok::<_, Error>(())
        })?;
    }

    if count != 768 {
        return Err(Error::Invalid("example batch match count"));
    }

    Ok(count)
}
