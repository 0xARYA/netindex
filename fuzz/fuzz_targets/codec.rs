#![no_main]

use libfuzzer_sys::fuzz_target;
use netindex::{
    Limits,
    codec::{Decoder, Field, Kind, Value},
};

fuzz_target!(|input: &[u8]| {
    let Some((&prefix, bytes)) = input.split_first() else {
        return;
    };

    let length = usize::from(prefix).min(bytes.len());
    let (metadata, record) = bytes.split_at(length);
    let limits = Limits {
        bytes: 65536,
        records: 256,
    };

    let Ok(decoder) = Decoder::open(metadata, limits) else {
        return;
    };

    if let Ok(row) = decoder.record(record) {
        for position in 0..decoder.schema().len() {
            let Some(Field::Required(kind) | Field::Optional(kind) | Field::Absent(kind)) =
                decoder.schema().field(position)
            else {
                panic!("schema field missing inside its declared length");
            };
            let typed = match kind {
                Kind::Bool => row.boolean(position).map(|v| v.map(Value::Bool)),
                Kind::U8 => row.u8(position).map(|v| v.map(Value::U8)),
                Kind::U16 => row.u16(position).map(|v| v.map(Value::U16)),
                Kind::U32 => row.u32(position).map(|v| v.map(Value::U32)),
                Kind::U64 => row.u64(position).map(|v| v.map(Value::U64)),
                Kind::I64 => row.i64(position).map(|v| v.map(Value::I64)),
                Kind::F64 => row.f64(position).map(|v| v.map(Value::F64)),
                Kind::String => row.string(position).map(|v| v.map(Value::String)),
                Kind::Bytes => row.bytes(position).map(|v| v.map(Value::Bytes)),
            }
            .map(|value| value.unwrap_or(Value::Null));

            match (row.get(position), typed) {
                (Ok(Value::F64(expected)), Ok(Value::F64(actual))) => {
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }
                (Ok(expected), Ok(actual)) => assert_eq!(actual, expected),
                (Err(_), Err(_)) => {}
                _ => panic!("typed and dynamic field access disagree"),
            }
        }

        drop(row.validate());
    }
});
