//! A [`U256`] as text: a JSON-RPC quantity (`0x` and hex digits without leading
//! zeros), decimal digits, and the JSON string a quantity is written as.

use std::{fmt, str::FromStr};

use crate::client::evm::u256::U256;
use crate::utils::hex;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadQuantity {
    #[error("`{0}` is not a hex quantity; expected 0x and hex digits")]
    NotHex(String),
    #[error("`{0}` is not a whole decimal number")]
    NotDecimal(String),
    #[error("`{0}` does not fit in 256 bits")]
    TooLarge(String),
}

/// The largest power of ten in a `u64`, the step decimal text is cut in.
const TEN_19: u64 = 10_000_000_000_000_000_000;

impl U256 {
    /// `0x` and hex digits, as JSON-RPC writes a quantity. Leading zeros are
    /// read (`eth_call` answers a whole word) and `0x` alone is zero.
    pub fn from_quantity(text: &str) -> Result<U256, BadQuantity> {
        let bad = || BadQuantity::NotHex(text.to_owned());
        let digits = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).ok_or_else(bad)?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let digits = digits.trim_start_matches('0');
        if digits.len() > 64 {
            return Err(BadQuantity::TooLarge(text.to_owned()));
        }
        let bytes: [u8; 32] = hex::decode_array(&format!("{digits:0>64}")).map_err(|_| bad())?;
        Ok(U256::from_be_bytes(bytes))
    }

    /// `0x` and the hex digits without leading zeros; zero is `0x0`.
    #[must_use]
    pub fn to_quantity(self) -> String {
        let digits = hex::encode(&self.to_be_bytes());
        let trimmed = digits.trim_start_matches('0');
        match trimmed.is_empty() {
            true => "0x0".to_owned(),
            false => format!("0x{trimmed}"),
        }
    }

    /// Decimal digits and nothing else.
    pub fn from_decimal(text: &str) -> Result<U256, BadQuantity> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(BadQuantity::NotDecimal(text.to_owned()));
        }
        let ten = U256::from_u64(10);
        text.bytes().try_fold(U256::ZERO, |value, digit| {
            value
                .checked_mul(ten)
                .and_then(|value| value.checked_add(U256::from_u64(u64::from(digit - b'0'))))
                .ok_or_else(|| BadQuantity::TooLarge(text.to_owned()))
        })
    }
}

/// Decimal, the way a person reads it.
impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut chunks = Vec::new();
        let mut left = *self;
        while let Some((quotient, chunk)) = left.div_rem_u64(TEN_19).filter(|_| !left.is_zero()) {
            chunks.push(chunk);
            left = quotient;
        }
        let Some((first, rest)) = chunks.split_last() else {
            return f.write_str("0");
        };
        write!(f, "{first}")?;
        for chunk in rest.iter().rev() {
            write!(f, "{chunk:019}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U256({self})")
    }
}

/// A quantity when it starts with `0x`, decimal digits otherwise.
impl FromStr for U256 {
    type Err = BadQuantity;

    fn from_str(text: &str) -> Result<U256, BadQuantity> {
        let text = text.trim();
        match text.starts_with("0x") || text.starts_with("0X") {
            true => U256::from_quantity(text),
            false => U256::from_decimal(text),
        }
    }
}

impl Serialize for U256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_quantity())
    }
}

/// A quantity string. A bare JSON number is refused: it may have been a float.
impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<U256, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        U256::from_quantity(&text).map_err(de::Error::custom)
    }
}

#[cfg(test)]
#[path = "tests/text.rs"]
mod tests;
