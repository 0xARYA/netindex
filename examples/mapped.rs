//! Write a private index file, reopen it as a mapping, and query it.

use std::{
    error::Error,
    fs::File,
    io::{self, Write},
};

use netindex::{Builder, Limits, MappedReader, Target};

fn main() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("membership.nidx");
    let limits = Limits::default();
    let range = Target::Range {
        start: "203.0.113.40".parse()?,
        end: "203.0.113.49".parse()?,
    };

    let mut builder = Builder::new(limits);
    builder.push(range, b"maintenance")?;
    builder.push(Target::Asn(64512), b"partner")?;
    builder.write_to(File::create(&path)?, b"example release")?;

    let file = File::open(&path)?;
    // SAFETY: The writer is closed. This private file stays unchanged until
    // the reader and its borrowed results are dropped.
    let reader = unsafe { MappedReader::map_file(&file, limits) }?;
    let ip = "203.0.113.42".parse()?;
    let matches = reader.lookup_ip(ip)?;
    let entry = matches.first().ok_or("expected one IP match")?;

    assert_eq!(matches.len(), 1);
    assert_eq!(entry.target, range);
    assert_eq!(entry.payload, b"maintenance");
    assert!(reader.contains_asn(64512)?);
    assert!(!reader.contains_ip("203.0.113.50".parse()?)?);
    writeln!(
        io::stdout().lock(),
        "{ip}: {}",
        std::str::from_utf8(entry.payload)?
    )?;

    Ok(())
}
