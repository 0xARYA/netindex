//! File-backed validation, lifetime, sharing, and resource limits.

#![cfg(feature = "mmap")]

use std::{io::Write, sync::Arc};

use netindex::{Builder, Error, Limits, MappedReader, Target};

#[test]
fn mapped_reader_outlives_handle_and_shares_borrowed_matches() {
    let mut builder = Builder::new(Limits::default());
    builder.push(Target::Asn(64512), b"evidence").unwrap();
    builder.push(Target::Asn(64512), b"evidence").unwrap();

    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&builder.into_bytes(b"metadata").unwrap())
        .unwrap();

    // SAFETY: This private temporary file is never changed after mapping.
    let reader = unsafe { MappedReader::map_file(&file, Limits::default()) }.unwrap();
    drop(file);

    let reader = Arc::new(reader);

    std::thread::scope(|scope| {
        for _ in 0..4 {
            let reader = Arc::clone(&reader);
            scope.spawn(move || {
                let rows = reader.lookup_asn(64512).unwrap();
                let [first, second] = rows.as_slice() else {
                    panic!("expected both assertions");
                };

                assert_eq!(first.payload, b"evidence");
                assert_eq!(first.payload.as_ptr(), second.payload.as_ptr());
                assert_eq!(reader.metadata().unwrap(), b"metadata");
                assert!(!reader.mapping().is_empty());
            });
        }
    });
}

#[test]
fn mapping_rejects_limits_short_files_and_invalid_indexes() {
    let mut file = tempfile::tempfile().unwrap();

    // SAFETY: Each private file remains unchanged while its mapping could exist.
    let result = unsafe { MappedReader::map_file(&file, Limits::default()) };
    assert!(matches!(result, Err(Error::Invalid("index header"))));

    file.write_all(&[0; 80]).unwrap();
    let limits = Limits {
        bytes: 79,
        ..Limits::default()
    };

    // SAFETY: The private file is unchanged until this attempt has returned.
    let result = unsafe { MappedReader::map_file(&file, limits) };
    assert!(matches!(result, Err(Error::Limit("artifact bytes"))));

    // SAFETY: The private file is unchanged until this attempt has returned.
    let result = unsafe { MappedReader::map_file(&file, Limits::default()) };
    assert!(matches!(result, Err(Error::Invalid(_))));
}

#[test]
fn mapping_enforces_record_limits_and_accepts_exact_byte_limit() {
    let mut builder = Builder::new(Limits::default());
    builder.push(Target::Asn(64512), b"value").unwrap();
    let bytes = builder.into_bytes(b"").unwrap();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();

    let limits = Limits {
        records: 0,
        bytes: bytes.len(),
    };
    // SAFETY: The private file stays unchanged during both mapping attempts.
    let result = unsafe { MappedReader::map_file(&file, limits) };
    assert!(matches!(result, Err(Error::Limit("records"))));

    // SAFETY: The private file stays unchanged until the reader is dropped.
    let reader = unsafe {
        MappedReader::map_file(
            &file,
            Limits {
                records: 1,
                ..limits
            },
        )
    }
    .unwrap();

    assert_eq!(reader.mapping().len(), bytes.len());
    assert_eq!(reader.lookup_asn(64512).unwrap().len(), 1);
}
