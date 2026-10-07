//! Independent fixture checks for the comparison payload codec.

#![cfg(all(feature = "mmdb", feature = "shared-values", feature = "mmap"))]

use netindex::{values::ValuePoolBuilder, Limits};

#[path = "../benches/support/mod.rs"]
#[allow(
    dead_code,
    reason = "only the benchmark codec is exercised by these tests"
)]
mod support;

use support::{dictionary, schema::Fields};

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

    Ok(())
}

#[test]
fn city_codec_rejects_radius_without_coordinates() -> Result<()> {
    let mut encoder = dictionary::Pool::default();
    let fields = Fields::Geo(support::schema::Geo {
        country_code: Some("US"),
        country_name: None,
        continent_code: None,
        region: None,
        region_code: None,
        city: None,
        postal_code: None,
        coordinates: None,
        time_zone: None,
    });
    let mut bytes = encoder.encode(&fields)?;
    let metadata = encoder.metadata("city")?;
    let pool = dictionary::pool(&metadata, "city")?;

    assert_eq!(dictionary::decode(&bytes, "city", pool)?, fields);
    *bytes.last_mut().ok_or("missing flags")? = 2;
    assert!(dictionary::decode(&bytes, "city", pool).is_err());

    Ok(())
}
