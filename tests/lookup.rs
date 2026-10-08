//! Large packed sections preserve matches at directory and address boundaries.

use std::net::{IpAddr, Ipv4Addr};

use netindex::{Builder, Limits, Reader, Target};

#[test]
fn large_packed_ipv4_sections_preserve_hits_gaps_and_extrema() {
    for count in [65_537u32, 1_048_577] {
        let mut builder = Builder::new(Limits {
            records: count as usize,
            ..Limits::default()
        });
        for id in 0..count {
            builder
                .push(
                    Target::Network {
                        address: Ipv4Addr::from(id * 512).into(),
                        prefix: 24,
                    },
                    &id.to_le_bytes(),
                )
                .unwrap();
        }

        let bytes = builder.into_bytes(b"").unwrap();
        let reader = Reader::open(
            bytes,
            Limits {
                records: count as usize,
                ..Limits::default()
            },
        )
        .unwrap();

        for id in (0..count).step_by(127).chain([count - 1]) {
            for offset in [0, 255, 256, 511] {
                let address = Ipv4Addr::from(id * 512 + offset);
                let matches = reader.lookup_ip(address.into()).unwrap();

                if offset <= 255 {
                    assert_eq!(matches.len(), 1);
                    assert_eq!(matches[0].id, id);
                    assert_eq!(matches[0].payload, id.to_le_bytes());
                } else {
                    assert!(matches.is_empty());
                }
            }
        }

        assert!(
            reader
                .lookup_ip(IpAddr::V4(Ipv4Addr::from(u32::MAX)))
                .unwrap()
                .is_empty()
        );
        assert!(reader.lookup_ip("::".parse().unwrap()).unwrap().is_empty());
    }
}
