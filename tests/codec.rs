//! Public codec contracts checked against caller values and independent bytes.

#![cfg(feature = "codec")]

use netindex::{
    codec::{Decoder, Encoder, Field, Kind, Schema, Value},
    Error, Limits,
};

#[test]
fn records_borrow_shared_values_and_preserve_missing_empty_and_scalar_bits() -> Result<(), Error> {
    let fields = [
        Field::Required(Kind::U32),
        Field::Optional(Kind::String),
        Field::Optional(Kind::Bytes),
        Field::Required(Kind::F64),
        Field::Absent(Kind::U16),
    ];
    let mut encoder = Encoder::new(Schema::new(&fields, Limits::default())?, Limits::default());
    let first = encoder.encode(&[
        Value::U32(64512),
        Value::String("Montréal"),
        Value::Bytes(&[0xff, 0]),
        Value::F64(-0.0),
        Value::Null,
    ])?;
    let second = encoder.encode(&[
        Value::U32(0),
        Value::String(""),
        Value::Bytes(&[]),
        Value::F64(f64::from_bits(0x7ff8_0000_0000_0042)),
        Value::Null,
    ])?;
    let missing = encoder.encode(&[
        Value::U32(1),
        Value::Null,
        Value::Null,
        Value::F64(1.5),
        Value::Null,
    ])?;
    let repeated = encoder.encode(&[
        Value::U32(64512),
        Value::String("Montréal"),
        Value::Bytes(&[0xff, 0]),
        Value::F64(-0.0),
        Value::Null,
    ])?;
    let metadata = encoder.into_metadata()?;
    let decoder = Decoder::open(&metadata, Limits::default())?;

    assert_eq!(first, repeated);
    assert_eq!(decoder.schema().record_length(), 21);
    assert_eq!(decoder.schema().field(4), Some(Field::Absent(Kind::U16)));
    assert_eq!(decoder.schema().field(5), None);

    let row = decoder.record(&first)?;
    row.validate()?;

    assert_eq!(row.get(0)?, Value::U32(64512));
    assert_eq!(row.u32(0)?, Some(64512));
    assert_eq!(row.string(1)?, Some("Montréal"));
    assert_eq!(row.bytes(2)?, Some([0xff, 0].as_slice()));
    assert_eq!(row.u16(4)?, None);
    assert!(matches!(
        row.string(0),
        Err(Error::CodecField {
            position: 0,
            reason: "incorrect kind"
        })
    ));
    assert!(matches!(
        row.u32(1),
        Err(Error::CodecField {
            position: 1,
            reason: "incorrect kind"
        })
    ));

    let Value::String(name) = row.get(1)? else {
        panic!("expected borrowed string")
    };

    assert_eq!(name, "Montréal");
    assert!(
        (metadata.as_ptr() as usize..metadata.as_ptr() as usize + metadata.len())
            .contains(&(name.as_ptr() as usize))
    );
    assert_eq!(row.get(2)?, Value::Bytes(&[0xff, 0]));

    let Value::F64(zero) = row.get(3)? else {
        panic!("expected floating-point value")
    };

    assert_eq!(zero.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(row.f64(3)?.map(f64::to_bits), Some((-0.0_f64).to_bits()));
    assert_eq!(row.get(4)?, Value::Null);

    let row = decoder.record(&second)?;
    assert_eq!(row.get(1)?, Value::String(""));
    assert_eq!(row.string(1)?, Some(""));
    assert_eq!(row.get(2)?, Value::Bytes(&[]));

    let Value::F64(nan) = row.get(3)? else {
        panic!("expected floating-point value")
    };

    assert_eq!(nan.to_bits(), 0x7ff8_0000_0000_0042);
    assert_eq!(row.f64(3)?.map(f64::to_bits), Some(0x7ff8_0000_0000_0042));

    let row = decoder.record(&missing)?;
    assert_eq!(row.get(1)?, Value::Null);
    assert_eq!(row.string(1)?, None);
    assert_eq!(row.get(2)?, Value::Null);
    assert_eq!(row.get(4)?, Value::Null);
    let error = row.get(5).unwrap_err();

    assert!(matches!(
        error,
        Error::CodecField {
            position: 5,
            reason: "position outside schema"
        }
    ));
    assert_eq!(
        error.to_string(),
        "invalid codec field 5: position outside schema"
    );

    Ok(())
}

#[test]
fn independent_bytes_reject_invalid_metadata_records_and_references() -> Result<(), Error> {
    let mut metadata = b"NETCODEC".to_vec();
    metadata.extend_from_slice(&2_u32.to_le_bytes());
    metadata.extend_from_slice(&[0, 0, 7, 1]);
    metadata.extend_from_slice(&21_u64.to_le_bytes());
    metadata.extend_from_slice(b"NIVALUES");
    metadata.extend_from_slice(&1_u32.to_le_bytes());
    metadata.extend_from_slice(&0_u32.to_le_bytes());
    metadata.extend_from_slice(&1_u32.to_le_bytes());
    metadata.push(b'x');
    metadata.extend_from_slice(b"NIVALUES");
    metadata.extend_from_slice(&0_u32.to_le_bytes());
    metadata.extend_from_slice(&0_u32.to_le_bytes());
    let decoder = Decoder::open(&metadata, Limits::default())?;

    let record = [1, 1, 0, 0, 0, 0];

    let row = decoder.record(&record)?;
    row.validate()?;

    assert_eq!(row.get(0)?, Value::Bool(true));
    assert_eq!(row.boolean(0)?, Some(true));
    assert_eq!(row.get(1)?, Value::String("x"));

    for length in 0..metadata.len() {
        assert!(
            Decoder::open(&metadata[..length], Limits::default()).is_err(),
            "truncation={length}"
        );
    }

    for offset in [0, 12, 13, 24, 36] {
        let mut changed = metadata.clone();
        changed[offset] = 255;

        assert!(
            Decoder::open(&changed, Limits::default()).is_err(),
            "offset={offset}"
        );
    }

    assert!(decoder.record(&record[..5]).is_err());

    let mut changed = record;
    changed[0] = 2;

    assert!(decoder.record(&changed).is_err());

    changed = record;
    changed[1] = 2;

    assert!(decoder.record(&changed)?.validate().is_err());
    assert!(decoder.record(&changed)?.boolean(0).is_err());

    changed = record;
    changed[2] = 1;

    assert!(decoder.record(&changed)?.get(1).is_err());

    let mut trailing = metadata.clone();
    trailing.push(0);

    assert!(Decoder::open(&trailing, Limits::default()).is_err());

    let mut invalid_utf8 = metadata;
    invalid_utf8[44] = 255;

    assert!(matches!(
        Decoder::open(&invalid_utf8, Limits::default()),
        Err(Error::Utf8(_))
    ));

    Ok(())
}

#[test]
fn schema_validation_limits_and_presence_bits_cross_byte_boundaries() -> Result<(), Error> {
    let fields = [Field::Optional(Kind::Bool); 9];
    let schema = Schema::new(&fields, Limits::default())?;
    let mut encoder = Encoder::new(schema, Limits::default());

    assert!(encoder.encode(&[Value::Bool(true)]).is_err());
    let mut wrong_kind = [Value::Bool(true); 9];
    wrong_kind[4] = Value::U8(1);
    let error = encoder.encode(&wrong_kind).unwrap_err();

    assert!(matches!(
        error,
        Error::CodecField {
            position: 4,
            reason: "unexpected or missing value"
        }
    ));
    assert_eq!(
        error.to_string(),
        "invalid codec field 4: unexpected or missing value"
    );

    let record = encoder.encode(&[Value::Bool(false); 9])?;
    let metadata = encoder.into_metadata()?;
    let decoder = Decoder::open(&metadata, Limits::default())?;

    decoder.record(&record)?.validate()?;

    assert_eq!(&record[..2], &[255, 1]);
    assert_eq!(decoder.record(&record)?.get(8)?, Value::Bool(false));

    assert!(Schema::new(
        &fields,
        Limits {
            records: 8,
            ..Limits::default()
        }
    )
    .is_err());

    assert!(Schema::new(
        &fields,
        Limits {
            bytes: 10,
            ..Limits::default()
        }
    )
    .is_err());

    assert!(Decoder::open(
        &metadata,
        Limits {
            bytes: metadata.len() - 1,
            ..Limits::default()
        }
    )
    .is_err());

    let schema = Schema::new(&[Field::Required(Kind::U64)], Limits::default())?;
    let mut encoder = Encoder::new(
        schema,
        Limits {
            bytes: 7,
            ..Limits::default()
        },
    );

    assert!(encoder.encode(&[Value::U64(0)]).is_err());
    assert!(encoder.into_metadata().is_err());

    let schema = Schema::new(&[Field::Required(Kind::U64); 20], Limits::default())?;
    let encoder = Encoder::new(
        schema,
        Limits {
            bytes: 100,
            ..Limits::default()
        },
    );

    assert!(matches!(
        encoder.into_metadata(),
        Err(Error::Limit("codec record bytes"))
    ));

    let fields = [
        Field::Required(Kind::U8),
        Field::Required(Kind::U16),
        Field::Required(Kind::U64),
        Field::Required(Kind::I64),
    ];
    let values = [
        Value::U8(u8::MAX),
        Value::U16(u16::MAX),
        Value::U64(u64::MAX),
        Value::I64(i64::MIN),
    ];
    let mut encoder = Encoder::new(Schema::new(&fields, Limits::default())?, Limits::default());
    assert!(encoder.encode(&[Value::Null; 4]).is_err());

    let bytes = encoder.encode(&values)?;
    let metadata = encoder.into_metadata()?;
    let decoder = Decoder::open(&metadata, Limits::default())?;
    let row = decoder.record(&bytes)?;

    assert_eq!(row.u8(0)?, Some(u8::MAX));
    assert_eq!(row.u16(1)?, Some(u16::MAX));
    assert_eq!(row.u64(2)?, Some(u64::MAX));
    assert_eq!(row.i64(3)?, Some(i64::MIN));

    for (position, value) in values.into_iter().enumerate() {
        assert_eq!(row.get(position)?, value);
    }

    let mut encoder = Encoder::new(Schema::new(&[], Limits::default())?, Limits::default());
    let record = encoder.encode(&[])?;
    let metadata = encoder.into_metadata()?;
    let decoder = Decoder::open(&metadata, Limits::default())?;
    assert!(decoder.schema().is_empty());
    assert_eq!(decoder.schema().len(), 0);
    decoder.record(&record)?.validate()?;
    assert!(decoder.record(&record)?.get(0).is_err());

    Ok(())
}
