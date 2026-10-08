//! DB-IP field selection layered over the public schema-neutral codec.

use netindex::{
    Limits,
    codec::{Decoder, Encoder, Field, Kind, Value},
};

use super::{
    Result,
    schema::{Asn, Coordinates, Fields, Geo},
};

pub(crate) fn encoder(kind: &str) -> Result<Encoder> {
    let fields = if kind == "asn" {
        vec![
            Field::Required(Kind::U32),
            Field::Optional(Kind::String),
            Field::Absent(Kind::String),
        ]
    } else {
        vec![
            Field::Optional(Kind::String),
            Field::Optional(Kind::String),
            Field::Optional(Kind::String),
            Field::Optional(Kind::String),
            Field::Absent(Kind::String),
            Field::Optional(Kind::String),
            Field::Absent(Kind::String),
            Field::Absent(Kind::String),
            Field::Optional(Kind::F64),
            Field::Optional(Kind::F64),
            Field::Absent(Kind::U16),
        ]
    };

    Ok(Encoder::new(
        netindex::codec::Schema::new(&fields, Limits::default())?,
        Limits::default(),
    ))
}

pub(crate) fn encode(encoder: &mut Encoder, fields: &Fields<'_>) -> Result<Vec<u8>> {
    Ok(match fields {
        Fields::Asn(row) => encoder.encode(&[
            Value::U32(row.number),
            text(row.organization),
            text(row.domain),
        ])?,
        Fields::Geo(row) => encoder.encode(&[
            text(row.country_code),
            text(row.country_name),
            text(row.continent_code),
            text(row.region),
            text(row.region_code),
            text(row.city),
            text(row.postal_code),
            text(row.time_zone),
            row.coordinates
                .as_ref()
                .map_or(Value::Null, |v| Value::F64(v.latitude)),
            row.coordinates
                .as_ref()
                .map_or(Value::Null, |v| Value::F64(v.longitude)),
            row.coordinates
                .as_ref()
                .and_then(|v| v.accuracy_radius_km)
                .map_or(Value::Null, Value::U16),
        ])?,
    })
}

pub(crate) fn decode<'a>(decoder: &Decoder<'a>, bytes: &[u8], kind: &str) -> Result<Fields<'a>> {
    let row = decoder.record(bytes)?;

    if kind == "asn" {
        return Ok(Fields::Asn(Asn {
            number: row.u32(0)?.ok_or("missing ASN number")?,
            organization: row.string(1)?,
            domain: row.string(2)?,
        }));
    }

    let coordinates = match (row.f64(8)?, row.f64(9)?) {
        (None, None) => None,
        (Some(latitude), Some(longitude)) => Some(Coordinates {
            latitude,
            longitude,
            accuracy_radius_km: row.u16(10)?,
        }),
        _ => return Err("incomplete coordinate pair".into()),
    };

    Ok(Fields::Geo(Geo {
        country_code: row.string(0)?,
        country_name: row.string(1)?,
        continent_code: row.string(2)?,
        region: row.string(3)?,
        region_code: row.string(4)?,
        city: row.string(5)?,
        postal_code: row.string(6)?,
        time_zone: row.string(7)?,
        coordinates,
    }))
}

fn text(value: Option<&str>) -> Value<'_> {
    value.map_or(Value::Null, Value::String)
}
