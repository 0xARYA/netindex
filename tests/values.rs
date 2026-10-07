//! Shared-value identity, borrowed access, fixed bytes, limits, and failed output.

#![cfg(feature = "shared-values")]

use std::io;

use netindex::{
    values::{StringPool, ValuePool, ValuePoolBuilder},
    Error, Limits,
};

#[test]
fn shared_values_preserve_binary_strings_empty_values_and_ids() {
    let mut builder = ValuePoolBuilder::new(Limits::default());
    let text = builder.intern("Montréal".as_bytes()).unwrap();
    let binary = builder.intern(&[0xff, 0, 0x80]).unwrap();
    let empty = builder.intern(&[]).unwrap();

    assert_eq!(builder.intern("Montréal".as_bytes()).unwrap(), text);
    assert_eq!(builder.intern(&[]).unwrap(), empty);
    assert_eq!(builder.len(), 3);

    let bytes = builder.into_bytes().unwrap();
    let pool = ValuePool::open(&bytes, Limits::default()).unwrap();

    assert_eq!(pool.len(), 3);
    assert!(!pool.is_empty());
    assert_eq!(pool.get(text).unwrap(), "Montréal".as_bytes());
    assert_eq!(pool.get(binary).unwrap(), &[0xff, 0, 0x80]);
    assert_eq!(pool.get(empty).unwrap(), &[]);
    assert!(bytes
        .as_ptr_range()
        .contains(&pool.get(text).unwrap().as_ptr()));
    assert!(matches!(pool.get(3), Err(Error::Invalid("value ID"))));
    assert!(pool.get(u32::MAX).is_err());
}

#[test]
fn independent_pool_fixture_rejects_invalid_offsets_and_truncation() {
    let bytes = [
        b'N', b'I', b'V', b'A', b'L', b'U', b'E', b'S', 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 3, 0,
        0, 0, b'a', b'b', b'c',
    ];
    let pool = ValuePool::open(&bytes, Limits::default()).unwrap();

    assert_eq!(pool.get(0).unwrap(), b"a");
    assert_eq!(pool.get(1).unwrap(), b"bc");

    for length in 0..bytes.len() {
        assert!(ValuePool::open(&bytes[..length], Limits::default()).is_err());
    }

    for (offset, value) in [(0, 0), (8, 3), (12, 1), (16, 4), (20, 2)] {
        let mut invalid = bytes;
        invalid[offset] = value;
        assert!(ValuePool::open(&invalid, Limits::default()).is_err());
    }

    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(ValuePool::open(&trailing, Limits::default()).is_err());
}

#[test]
fn limits_and_io_failures_leave_existing_ids_intact() {
    let mut builder = ValuePoolBuilder::new(Limits {
        records: 1,
        bytes: 21,
    });
    assert_eq!(builder.intern(b"a").unwrap(), 0);
    assert!(builder.intern(b"b").is_err());
    assert_eq!(builder.intern(b"a").unwrap(), 0);
    let bytes = builder.into_bytes().unwrap();

    assert!(ValuePool::open(
        &bytes,
        Limits {
            records: 0,
            ..Limits::default()
        }
    )
    .is_err());
    assert!(ValuePool::open(
        &bytes,
        Limits {
            bytes: bytes.len() - 1,
            ..Limits::default()
        }
    )
    .is_err());

    let mut bounded = ValuePoolBuilder::new(Limits {
        records: 10,
        bytes: 21,
    });
    assert!(bounded.intern(b"too large").is_err());
    assert!(bounded.is_empty());
    assert_eq!(bounded.intern(b"a").unwrap(), 0);

    let empty = ValuePoolBuilder::new(Limits::default())
        .into_bytes()
        .unwrap();
    assert!(ValuePool::open(&empty, Limits::default())
        .unwrap()
        .is_empty());

    assert!(matches!(bounded.write_to(Fail), Err(Error::Io(_))));
}

#[test]
fn typed_strings_validate_once_and_borrow_only_valid_utf8() {
    let mut builder = ValuePoolBuilder::new(Limits::default());
    let empty = builder.intern(b"").unwrap();
    let unicode = builder.intern("🦀 Montréal".as_bytes()).unwrap();
    let bytes = builder.into_bytes().unwrap();
    let strings = StringPool::open(&bytes, Limits::default()).unwrap();

    assert_eq!(strings.len(), 2);
    assert!(!strings.is_empty());
    assert_eq!(strings.get(empty).unwrap(), "");
    assert_eq!(strings.get(unicode).unwrap(), "🦀 Montréal");
    assert!(bytes
        .as_ptr_range()
        .contains(&strings.get(unicode).unwrap().as_ptr()));
    assert!(strings.get(u32::MAX).is_err());

    let mut invalid = ValuePoolBuilder::new(Limits::default());
    invalid.intern(&[0xff]).unwrap();
    let invalid = invalid.into_bytes().unwrap();
    assert!(matches!(
        StringPool::open(&invalid, Limits::default()),
        Err(Error::Utf8(_))
    ));
}

struct Fail;
impl io::Write for Fail {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("test write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
