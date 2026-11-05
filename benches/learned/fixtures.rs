use netindex::{Builder, Limits, Target};
use std::{
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("expected fresh output directory")?,
    );
    fs::create_dir(&output)?;
    for distribution in ["curved", "clustered"] {
        for family in ["v4", "v6"] {
            for payload in ["shared", "unique"] {
                let mut b = Builder::new(Limits::default());
                for id in 0..65_536u32 {
                    let n = u64::from(id);
                    let raw = if distribution == "curved" {
                        n * (n + 1)
                    } else {
                        (n / 1024) * (1 << 26) + (n % 1024) * 2
                    };
                    let address: IpAddr = if family == "v4" {
                        Ipv4Addr::from(u32::try_from(raw)?).into()
                    } else {
                        Ipv6Addr::from(
                            (u128::from(u64::MAX) << 64) + u128::from(raw * ((1u64 << 32) + 15)),
                        )
                        .into()
                    };
                    let value = if payload == "shared" { 42u64 } else { n };
                    b.push(Target::Address(address), &value.to_le_bytes())?;
                }
                fs::write(
                    output.join(format!("{distribution}-{family}-{payload}.nidx")),
                    b.into_bytes(&[])?,
                )?;
            }
        }
    }
    Ok(())
}
