#![no_main]

use libfuzzer_sys::fuzz_target;
use netindex::{
    Limits,
    values::{StringPool, ValuePool, ValuePoolBuilder},
};

fuzz_target!(|bytes: &[u8]| {
    let limits = Limits {
        records: 4096,
        bytes: 65_536,
    };
    if let Ok(pool) = ValuePool::open(bytes, limits) {
        for id in 0..pool.len() {
            pool.get(id as u32).unwrap();
        }
        assert!(pool.get(u32::MAX).is_err());
    }

    if let Ok(strings) = StringPool::open(bytes, limits) {
        let values = ValuePool::open(bytes, limits).unwrap();
        for id in 0..strings.len() {
            let expected = std::str::from_utf8(values.get(id as u32).unwrap()).unwrap();
            let actual = strings.get(id as u32).unwrap();

            assert_eq!(actual, expected);
        }
    }

    let mut builder = ValuePoolBuilder::new(limits);
    let mut expected = Vec::new();
    for value in bytes.chunks(32).take(256) {
        let id = builder.intern(value).unwrap();
        assert_eq!(builder.intern(value).unwrap(), id);
        expected.push((id, value));
    }

    let encoded = builder.into_bytes().unwrap();
    let pool = ValuePool::open(&encoded, limits).unwrap();

    for (id, value) in expected {
        assert_eq!(pool.get(id).unwrap(), value);
    }
});
