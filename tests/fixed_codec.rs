//! Independent fixtures keep the benchmark-only fixed-codec baseline honest.

#![cfg(all(feature = "mmdb", feature = "shared-values", feature = "mmap"))]

use netindex::{values::ValuePoolBuilder, Limits};

use schema::{Coordinates, Fields, Geo};

#[path = "../benches/support/schema.rs"]
#[expect(
    dead_code,
    reason = "only the benchmark codec is exercised by these tests"
)]
mod schema;

#[path = "../benches/support/fixed_codec.rs"]
#[expect(
    dead_code,
    reason = "fixtures exercise baseline records rather than its metadata writer"
)]
mod fixed_codec;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn asn_baseline_decodes_independent_number_organization_and_absent_domain() -> Result<()> {
    let mut values = ValuePoolBuilder::new(Limits::default());
    values.intern(b"example network")?;

    let mut metadata = b"NISHAR\0\0".to_vec();
    values.write_to(&mut metadata)?;
    let pool = fixed_codec::pool(&metadata, "asn")?;

    let mut payload = 64512_u32.to_le_bytes().to_vec();
    payload.extend_from_slice(&0_u32.to_le_bytes());
    payload.extend_from_slice(&u32::MAX.to_le_bytes());

    let fields = fixed_codec::decode(&payload, "asn", pool)?;
    assert!(
        matches!(fields, Fields::Asn(row) if row.number == 64512 && row.organization == Some("example network") && row.domain.is_none())
    );

    Ok(())
}

#[test]
fn city_baseline_preserves_independent_fields_coordinates_and_radius() -> Result<()> {
    let mut values = ValuePoolBuilder::new(Limits::default());
    values.intern(b"US")?;
    values.intern(b"example city")?;

    let mut metadata = b"NISHAR\x01\0".to_vec();
    values.write_to(&mut metadata)?;
    let pool = fixed_codec::pool(&metadata, "city")?;

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

    assert_eq!(fixed_codec::decode(&payload, "city", pool)?, expected);
    assert_eq!(fixed_codec::Pool::default().encode(&expected)?, payload);

    Ok(())
}
