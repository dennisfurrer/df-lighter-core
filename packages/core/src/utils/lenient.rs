//! Reading the venue's answers leniently where it is loose: a list it writes as `null`
//! (Robinhood Chain does), a decimal it leaves as `""`, an index it writes as `-1` for none.

use serde::{Deserialize, Deserializer};

use crate::utils::dec::Dec;
use crate::utils::{AccountIndex, MarketId};

/// A list, `null` or absent read as empty.
pub(crate) fn list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// A decimal string, `""` or `null` read as `None`.
pub(crate) fn blank_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Dec>, D::Error> {
    match Option::<Text>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Text::Number(units)) => Ok(Some(Dec::from_int(units))),
        Some(Text::Text(text)) if text.trim().is_empty() => Ok(None),
        Some(Text::Text(text)) => Dec::parse(&text).map(Some).map_err(serde::de::Error::custom),
    }
}

/// A decimal string, `""` or `null` read as zero.
pub(crate) fn zero_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Dec, D::Error> {
    Ok(blank_dec(deserializer)?.unwrap_or(Dec::ZERO))
}

/// An account index the venue may write as `-1` (or anything below zero) for none.
pub(crate) fn maybe_account<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<AccountIndex>, D::Error> {
    let value = Option::<i64>::deserialize(deserializer)?;
    Ok(value.and_then(|value| u64::try_from(value).ok()).and_then(|value| AccountIndex::new(value).ok()))
}

/// A market id where the venue may write 255 (no market) or `-1`: those read as `None`.
pub(crate) fn maybe_market<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<MarketId>, D::Error> {
    let value = Option::<i64>::deserialize(deserializer)?;
    Ok(value.and_then(|value| u64::try_from(value).ok()).and_then(|value| MarketId::new(value).ok()))
}

/// A string that may be absent or `null`, read as empty.
pub(crate) fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A whole number the venue writes signed: below zero (`-1` for none) reads as 0.
pub(crate) fn unsigned<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = Option::<i64>::deserialize(deserializer)?;
    Ok(value.and_then(|value| u64::try_from(value).ok()).unwrap_or(0))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Text {
    Number(i64),
    Text(String),
}

/// An answer with nothing in it but `code`, which [`crate::client::read`] has checked.
#[derive(Deserialize)]
pub(crate) struct Done {}
