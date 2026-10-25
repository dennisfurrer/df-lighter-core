//! How the socket writes a field, read leniently where the venue varies it: a decimal left blank
//! when there is none, rows sent as a list, a map by market or asset, one object or `null` as the
//! frame and the docs disagree, and a balance missing the fields it has nothing for. Each reads
//! from the text as written, so a figure never passes through a float.

use std::fmt;

use serde::de::{self, DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::account::Asset;
use crate::utils::{AssetId, Dec};

/// A decimal string, `None` when blank (`""`), `null` or missing. A JSON integer reads too; a
/// JSON fraction is refused, as [`Dec`] refuses it.
pub(crate) fn blank<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Dec>, D::Error> {
    struct Blank;

    impl<'de> Visitor<'de> for Blank {
        type Value = Option<Dec>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a decimal string, blank or null")
        }

        fn visit_str<E: de::Error>(self, text: &str) -> Result<Option<Dec>, E> {
            match text.trim().is_empty() {
                true => Ok(None),
                false => Dec::parse(text).map(Some).map_err(E::custom),
            }
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<Option<Dec>, E> {
            Ok(Some(Dec::new(i128::from(value), 0).unwrap_or(Dec::ZERO)))
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<Option<Dec>, E> {
            Ok(Some(Dec::from_int(value)))
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<Dec>, E> {
            Ok(None)
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<Dec>, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Option<Dec>, D::Error> {
            deserializer.deserialize_any(Blank)
        }
    }

    deserializer.deserialize_any(Blank)
}

/// Any JSON value as the text it was written as; empty when missing.
pub(crate) fn raw_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let raw = Box::<RawValue>::deserialize(deserializer)?;
    Ok(raw.get().to_owned())
}

/// An id the venue may write as a string or a number: as text.
pub(crate) fn id_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let raw = Option::<Box<RawValue>>::deserialize(deserializer)?;
    Ok(raw.map(|raw| {
        let text = raw.get();
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text.to_owned())
    }))
}

/// Rows however the venue nests them: `null`, a list, one row, or a map by market or asset whose
/// values are rows or lists of rows. In the order written.
pub(crate) fn rows<'de, D: Deserializer<'de>, T: DeserializeOwned>(deserializer: D) -> Result<Vec<T>, D::Error> {
    let raw = Option::<Box<RawValue>>::deserialize(deserializer)?;
    let mut out = Vec::new();
    if let Some(raw) = raw {
        collect(raw.get(), 0, &mut out).map_err(de::Error::custom)?;
    }
    Ok(out)
}

/// At most one row: the last, when the venue sends a list where the docs say one.
pub(crate) fn one<'de, D: Deserializer<'de>, T: DeserializeOwned>(deserializer: D) -> Result<Option<T>, D::Error> {
    Ok(rows::<D, T>(deserializer)?.pop())
}

/// Lists inside maps inside lists go this deep at most.
const DEEPEST: usize = 3;

fn collect<T: DeserializeOwned>(text: &str, depth: usize, out: &mut Vec<T>) -> Result<(), serde_json::Error> {
    let text = text.trim_start();
    match text.as_bytes().first() {
        Some(b'n') => Ok(()),
        Some(b'[') if depth < DEEPEST => {
            for item in serde_json::from_str::<Vec<Box<RawValue>>>(text)? {
                collect(item.get(), depth + 1, out)?;
            }
            Ok(())
        }
        Some(b'{') if depth < DEEPEST => {
            let Entries(entries) = serde_json::from_str(text)?;
            if entries.is_empty() {
                return Ok(());
            }
            if entries.iter().all(|(key, _)| !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit())) {
                for (_, value) in entries {
                    collect(value.get(), depth + 1, out)?;
                }
                return Ok(());
            }
            out.push(serde_json::from_str(text)?);
            Ok(())
        }
        _ => {
            out.push(serde_json::from_str(text)?);
            Ok(())
        }
    }
}

/// An object's entries in the order written.
struct Entries(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Ordered;

        impl<'de> Visitor<'de> for Ordered {
            type Value = Entries;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Entries, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, Box<RawValue>>()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }

        deserializer.deserialize_map(Ordered)
    }
}

/// A balance as the socket writes it. `account_all` sends every field; the docs' examples for
/// `account_all_assets` leave out the margin fields, which the venue omits when zero or empty.
#[derive(Deserialize)]
struct LooseAsset {
    #[serde(default)]
    symbol: String,
    asset_id: AssetId,
    balance: Dec,
    #[serde(default, deserialize_with = "blank")]
    locked_balance: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    margin_balance: Option<Dec>,
    #[serde(default)]
    margin_mode: String,
    #[serde(default, deserialize_with = "blank")]
    multiplier: Option<Dec>,
}

/// Balances, as [`rows`] reads them, into the [`Asset`] every read writes: a field left out reads
/// as zero or empty, as the venue omits it.
pub(crate) fn assets<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Asset>, D::Error> {
    let loose: Vec<LooseAsset> = rows(deserializer)?;
    Ok(loose
        .into_iter()
        .map(|asset| Asset {
            symbol: asset.symbol,
            asset_id: asset.asset_id,
            balance: asset.balance,
            locked_balance: asset.locked_balance.unwrap_or(Dec::ZERO),
            margin_balance: asset.margin_balance.unwrap_or(Dec::ZERO),
            margin_mode: asset.margin_mode,
            multiplier: asset.multiplier.unwrap_or(Dec::ZERO),
        })
        .collect())
}

#[cfg(test)]
#[path = "tests/fields.rs"]
mod tests;
