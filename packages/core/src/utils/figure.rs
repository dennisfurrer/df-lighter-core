//! Many reads send a number as a bare JSON float rather than a decimal string:
//! `publicPoolsMetadata` writes `"annual_percentage_yield":30.733724772746875`,
//! a candle `"o":80687`, the market stats their volumes. serde_json would put
//! that through an f64 and keep whatever digits came back out; this reads the
//! text the venue wrote instead.

use std::fmt;

use serde::{Deserialize, Deserializer, de};
use serde_json::value::RawValue;

use crate::utils::dec::{Dec, MAX_SCALE};

/// A number exactly as the venue wrote it, float or string. No f64 holds it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Figure(String);

impl Figure {
    /// The digits as written: `0.037456007367970744`, `1e-7`, `0.0`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The value, an exponent applied exactly by moving the point. Past eighteen places it is
    /// the nearest at eighteen (`0.00009599999999999999` is `0.000096`); `None` past what a
    /// [`Dec`] counts.
    #[must_use]
    pub fn to_dec(&self) -> Option<Dec> {
        let (mantissa, exponent) = match self.0.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i64>().ok()?),
            None => (self.0.as_str(), 0),
        };
        let (negative, mantissa) = match mantissa.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let digits = format!("{whole}{fraction}");
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let point = i64::try_from(whole.len()).ok()?.checked_add(exponent)?;
        let length = i64::try_from(digits.len()).ok()?;
        let (whole, fraction) = match point {
            point if point <= 0 => ("0".to_owned(), format!("{}{digits}", "0".repeat(usize::try_from(-point).ok()?))),
            point if point >= length => {
                (format!("{digits}{}", "0".repeat(usize::try_from(point - length).ok()?)), String::new())
            }
            point => {
                let (whole, fraction) = digits.split_at(usize::try_from(point).ok()?);
                (whole.to_owned(), fraction.to_owned())
            }
        };
        let places = usize::try_from(MAX_SCALE).ok()?;
        let (kept, rest) = fraction.split_at(fraction.len().min(places));
        let cut = Dec::parse(&format!("{whole}.{kept}0")).ok()?;
        let value = match rest.as_bytes().first() {
            Some(b'5'..=b'9') => cut.checked_add(Dec::new(1, MAX_SCALE)?)?,
            _ => cut,
        };
        Some(match negative {
            true => value.negate(),
            false => value,
        })
    }

    fn read(text: &str) -> Option<Figure> {
        let plain = |text: &str| {
            !text.is_empty()
                && text.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E'))
        };
        match text.strip_prefix('"') {
            Some(_) => serde_json::from_str::<String>(text).ok().filter(|inner| plain(inner)).map(Figure),
            None => plain(text).then(|| Figure(text.to_owned())),
        }
    }
}

impl fmt::Display for Figure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// From the raw JSON text, so it reads only through `serde_json::from_str`
/// (which is how every read is read), not through a `serde_json::Value`.
impl<'de> Deserialize<'de> for Figure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Figure::read(raw.get().trim()).ok_or_else(|| de::Error::custom(format!("`{}` is not a number", raw.get())))
    }
}

#[cfg(test)]
#[path = "tests/figure.rs"]
mod tests;
