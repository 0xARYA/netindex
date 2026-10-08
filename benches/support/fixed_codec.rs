//! Benchmark-only ASN/city fixed layout used as the pre-public-codec baseline.

use netindex::{
    Limits,
    values::{StringPool, ValuePoolBuilder},
};

use super::{
    Result,
    schema::{Asn, Coordinates, Fields, Geo},
};

const MAGIC: &[u8; 6] = b"NISHAR";
const NULL: u32 = u32::MAX;

pub(crate) struct Pool {
    values: ValuePoolBuilder,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            values: ValuePoolBuilder::new(Limits::default()),
        }
    }
}

impl Pool {
    pub(crate) fn encode(&mut self, fields: &Fields<'_>) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        match fields {
            Fields::Asn(row) => {
                bytes.extend_from_slice(&row.number.to_le_bytes());

                for value in [row.organization, row.domain] {
                    bytes.extend_from_slice(&self.intern(value)?.to_le_bytes());
                }
            }
            Fields::Geo(row) => {
                for value in [
                    row.country_code,
                    row.country_name,
                    row.continent_code,
                    row.region,
                    row.region_code,
                    row.city,
                    row.postal_code,
                    row.time_zone,
                ] {
                    bytes.extend_from_slice(&self.intern(value)?.to_le_bytes());
                }

                let coordinates = row.coordinates.as_ref();
                bytes.extend_from_slice(
                    &coordinates.map(|v| v.latitude).unwrap_or(0.0).to_le_bytes(),
                );
                bytes.extend_from_slice(
                    &coordinates
                        .map(|v| v.longitude)
                        .unwrap_or(0.0)
                        .to_le_bytes(),
                );

                let radius = coordinates.and_then(|v| v.accuracy_radius_km);
                bytes.extend_from_slice(&radius.unwrap_or(0).to_le_bytes());

                bytes.push(u8::from(coordinates.is_some()) | (u8::from(radius.is_some()) << 1));
            }
        }

        Ok(bytes)
    }

    pub(crate) fn metadata(self, kind: &str) -> Result<Vec<u8>> {
        let mut bytes = Vec::from(MAGIC.as_slice());
        bytes.extend_from_slice(&[u8::from(kind == "city"), 0]);
        self.values.write_to(&mut bytes)?;

        Ok(bytes)
    }

    fn intern(&mut self, value: Option<&str>) -> Result<u32> {
        Ok(match value {
            Some(value) => self.values.intern(value.as_bytes())?,
            None => NULL,
        })
    }
}

pub(crate) fn pool<'a>(metadata: &'a [u8], kind: &str) -> Result<StringPool<'a>> {
    if metadata.get(..6) != Some(MAGIC.as_slice())
        || metadata.get(6) != Some(&u8::from(kind == "city"))
        || metadata.get(7) != Some(&0)
    {
        return Err("invalid dictionary schema".into());
    }

    Ok(StringPool::open(
        metadata.get(8..).ok_or("missing pool")?,
        Limits::default(),
    )?)
}

pub(crate) fn decode<'a>(bytes: &[u8], kind: &str, pool: StringPool<'a>) -> Result<Fields<'a>> {
    if kind == "asn" {
        if bytes.len() != 12 {
            return Err("invalid dictionary ASN record".into());
        }

        return Ok(Fields::Asn(Asn {
            number: u32_at(bytes, 0)?,
            organization: string(pool, u32_at(bytes, 4)?)?,
            domain: string(pool, u32_at(bytes, 8)?)?,
        }));
    }

    if bytes.len() != 51 {
        return Err("invalid dictionary city record".into());
    }

    let flags = *bytes.get(50).ok_or("missing coordinate flags")?;

    if flags > 3 || flags == 2 {
        return Err("invalid coordinate flags".into());
    }

    let coordinates = if flags & 1 == 0 {
        None
    } else {
        Some(Coordinates {
            latitude: f64::from_le_bytes(bytes.get(32..40).ok_or("missing latitude")?.try_into()?),
            longitude: f64::from_le_bytes(
                bytes.get(40..48).ok_or("missing longitude")?.try_into()?,
            ),
            accuracy_radius_km: if flags & 2 == 0 {
                None
            } else {
                Some(u16::from_le_bytes(
                    bytes.get(48..50).ok_or("missing radius")?.try_into()?,
                ))
            },
        })
    };

    Ok(Fields::Geo(Geo {
        country_code: string(pool, u32_at(bytes, 0)?)?,
        country_name: string(pool, u32_at(bytes, 4)?)?,
        continent_code: string(pool, u32_at(bytes, 8)?)?,
        region: string(pool, u32_at(bytes, 12)?)?,
        region_code: string(pool, u32_at(bytes, 16)?)?,
        city: string(pool, u32_at(bytes, 20)?)?,
        postal_code: string(pool, u32_at(bytes, 24)?)?,
        time_zone: string(pool, u32_at(bytes, 28)?)?,
        coordinates,
    }))
}

#[inline]
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(4).ok_or("offset overflow")?)
            .ok_or("truncated integer")?
            .try_into()?,
    ))
}

#[inline]
fn string(pool: StringPool<'_>, id: u32) -> Result<Option<&str>> {
    if id == NULL {
        return Ok(None);
    }

    Ok(Some(pool.get(id)?))
}
