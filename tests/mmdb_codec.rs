//! Independent fixture checks for the comparison payload codec.

#![cfg(all(feature = "mmdb", feature = "shared-values", feature = "mmap"))]

use netindex::{values::ValuePoolBuilder, Limits};

use support::{
    dictionary,
    schema::{Coordinates, Fields, Geo},
};

#[path = "../benches/support/mod.rs"]
#[expect(
    dead_code,
    reason = "only the benchmark codec is exercised by these tests"
)]
mod support;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn asn_codec_decodes_independent_bytes_and_rejects_truncation() -> Result<()> {
    let mut values = ValuePoolBuilder::new(Limits::default());
    values.intern(b"example network")?;

    let mut metadata = b"NISHAR\0\0".to_vec();
    values.write_to(&mut metadata)?;
    let pool = dictionary::pool(&metadata, "asn")?;

    let mut payload = 64512_u32.to_le_bytes().to_vec();
    payload.extend_from_slice(&0_u32.to_le_bytes());
    payload.extend_from_slice(&u32::MAX.to_le_bytes());

    let fields = dictionary::decode(&payload, "asn", pool)?;
    assert!(
        matches!(fields, Fields::Asn(row) if row.number == 64512 && row.organization == Some("example network") && row.domain.is_none())
    );

    payload.pop();
    assert!(dictionary::decode(&payload, "asn", pool).is_err());

    let invalid_reference = [0, 0, 0, 0, 1, 0, 0, 0, 255, 255, 255, 255];
    assert!(dictionary::decode(&invalid_reference, "asn", pool).is_err());

    Ok(())
}

#[test]
fn city_codec_decodes_independent_fields_and_coordinates() -> Result<()> {
    let mut values = ValuePoolBuilder::new(Limits::default());
    values.intern(b"US")?;
    values.intern(b"example city")?;

    let mut metadata = b"NISHAR\x01\0".to_vec();
    values.write_to(&mut metadata)?;
    let pool = dictionary::pool(&metadata, "city")?;

    let mut payload = Vec::new();
    for id in [
        0,
        u32::MAX,
        u32::MAX,
        u32::MAX,
        u32::MAX,
        1,
        u32::MAX,
        u32::MAX,
    ] {
        payload.extend_from_slice(&id.to_le_bytes());
    }
    payload.extend_from_slice(&51.5_f64.to_le_bytes());
    payload.extend_from_slice(&(-0.125_f64).to_le_bytes());
    payload.extend_from_slice(&10_u16.to_le_bytes());
    payload.push(3);

    let expected = Fields::Geo(Geo {
        country_code: Some("US"),
        country_name: None,
        continent_code: None,
        region: None,
        region_code: None,
        city: Some("example city"),
        postal_code: None,
        coordinates: Some(Coordinates {
            latitude: 51.5,
            longitude: -0.125,
            accuracy_radius_km: Some(10),
        }),
        time_zone: None,
    });

    assert_eq!(dictionary::decode(&payload, "city", pool)?, expected);
    assert_eq!(dictionary::Pool::default().encode(&expected)?, payload);

    *payload.last_mut().ok_or("missing flags")? = 2;
    assert!(dictionary::decode(&payload, "city", pool).is_err());

    payload.pop();
    assert!(dictionary::decode(&payload, "city", pool).is_err());

    Ok(())
}
