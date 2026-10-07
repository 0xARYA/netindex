use std::fmt;

use maxminddb::LookupResult;
use serde::{
    de::{IgnoredAny, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};

use super::Result;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) enum Fields<'a> {
    Asn(#[serde(borrow)] Asn<'a>),
    Geo(#[serde(borrow)] Geo<'a>),
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Asn<'a> {
    pub(crate) number: u32,
    #[serde(borrow)]
    pub(crate) organization: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) domain: Option<&'a str>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Geo<'a> {
    #[serde(borrow)]
    pub(crate) country_code: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) country_name: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) continent_code: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) region: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) region_code: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) city: Option<&'a str>,
    #[serde(borrow)]
    pub(crate) postal_code: Option<&'a str>,
    pub(crate) coordinates: Option<Coordinates>,
    #[serde(borrow)]
    pub(crate) time_zone: Option<&'a str>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Coordinates {
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    pub(crate) accuracy_radius_km: Option<u16>,
}

#[derive(Deserialize)]
struct NativeAsn<'a> {
    autonomous_system_number: u32,
    #[serde(borrow)]
    autonomous_system_organization: Option<&'a str>,
}

#[derive(Deserialize)]
struct Names<'a> {
    #[serde(borrow)]
    en: Option<&'a str>,
}

#[derive(Deserialize)]
struct Named<'a> {
    #[serde(borrow)]
    iso_code: Option<&'a str>,
    #[serde(borrow)]
    names: Option<Names<'a>>,
}

#[derive(Deserialize)]
struct Code<'a> {
    #[serde(borrow)]
    code: Option<&'a str>,
}

#[derive(Deserialize)]
struct Location<'a> {
    latitude: Option<f64>,
    longitude: Option<f64>,
    accuracy_radius: Option<u16>,
    #[serde(borrow)]
    time_zone: Option<&'a str>,
}

#[derive(Deserialize)]
struct NativeGeo<'a> {
    #[serde(borrow)]
    country: Option<Named<'a>>,
    #[serde(borrow)]
    continent: Option<Code<'a>>,
    #[serde(borrow, default, deserialize_with = "first_subdivision")]
    subdivisions: Option<Named<'a>>,
    #[serde(borrow)]
    city: Option<Named<'a>>,
    #[serde(borrow)]
    postal: Option<Code<'a>>,
    #[serde(borrow)]
    location: Option<Location<'a>>,
}

pub(crate) fn decode_native<'a>(
    result: &LookupResult<'a, memmap2::Mmap>,
    kind: &str,
) -> Result<Option<Fields<'a>>> {
    if kind == "asn" {
        return Ok(result.decode::<NativeAsn<'a>>()?.map(|row| {
            Fields::Asn(Asn {
                number: row.autonomous_system_number,
                organization: text(row.autonomous_system_organization),
                domain: None,
            })
        }));
    }

    let Some(row) = result.decode::<NativeGeo<'a>>()? else {
        return Ok(None);
    };

    let undefined = row
        .country
        .as_ref()
        .is_some_and(|value| value.iso_code == Some("ZZ"));
    let country = row.country.as_ref().filter(|_| !undefined);
    let location = row.location.as_ref();
    let coordinates = match location {
        Some(location) => match (location.latitude, location.longitude) {
            (Some(0.0), Some(0.0)) if undefined => None,
            (Some(latitude), Some(longitude))
                if latitude.is_finite()
                    && longitude.is_finite()
                    && (-90.0..=90.0).contains(&latitude)
                    && (-180.0..=180.0).contains(&longitude) =>
            {
                Some(Coordinates {
                    latitude,
                    longitude,
                    accuracy_radius_km: location.accuracy_radius,
                })
            }
            (None, None) => None,
            _ => return Err("invalid serving coordinates".into()),
        },
        None => None,
    };

    Ok(Some(Fields::Geo(Geo {
        country_code: country.and_then(|value| value.iso_code),
        country_name: english(country),
        continent_code: row
            .continent
            .and_then(|value| value.code)
            .filter(|code| *code != "ZZ"),
        region: english(row.subdivisions.as_ref()),
        region_code: text(row.subdivisions.and_then(|value| value.iso_code)),
        city: english(row.city.as_ref()),
        postal_code: text(row.postal.and_then(|value| value.code)),
        coordinates,
        time_zone: text(location.and_then(|value| value.time_zone)),
    })))
}

fn english<'a>(row: Option<&Named<'a>>) -> Option<&'a str> {
    text(
        row.and_then(|row| row.names.as_ref())
            .and_then(|names| names.en),
    )
}

fn text(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && !matches!(*value, "-" | "--"))
}

fn first_subdivision<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Named<'de>>, D::Error> {
    struct First;

    impl<'de> Visitor<'de> for First {
        type Value = Option<Named<'de>>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a subdivision array or null")
        }

        fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            deserializer.deserialize_seq(self)
        }

        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let first = sequence.next_element()?;

            while sequence.next_element::<IgnoredAny>()?.is_some() {}

            Ok(first)
        }
    }

    deserializer.deserialize_option(First)
}
