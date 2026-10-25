//! [`B256`], thirty-two bytes: a hash, a log topic, a storage slot, or an
//! address as a word.

use std::{fmt, str::FromStr};

use crate::client::evm::u256::U256;
use crate::utils::{Address, hex};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Thirty-two bytes: a transaction or block hash, a log topic, a storage slot.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct B256(pub [u8; 32]);

impl B256 {
    pub const ZERO: B256 = B256([0u8; 32]);

    /// An address as a word: twelve zero bytes, then its twenty.
    #[must_use]
    pub fn from_address(address: Address) -> B256 {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(address.bytes());
        B256(word)
    }

    /// The address in the low twenty bytes, when the high twelve are zero:
    /// how an indexed `address` sits in a log topic.
    #[must_use]
    pub fn to_address(&self) -> Option<Address> {
        if self.0[..12].iter().any(|byte| *byte != 0) {
            return None;
        }
        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(&self.0[12..]);
        Some(Address::from_bytes(bytes))
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode_prefixed(&self.0)
    }
}

impl From<[u8; 32]> for B256 {
    fn from(bytes: [u8; 32]) -> B256 {
        B256(bytes)
    }
}

impl From<U256> for B256 {
    fn from(value: U256) -> B256 {
        B256(value.to_be_bytes())
    }
}

impl FromStr for B256 {
    type Err = crate::utils::BadHex;

    fn from_str(text: &str) -> Result<B256, Self::Err> {
        hex::decode_array(text).map(B256)
    }
}

impl fmt::Display for B256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for B256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B256({})", self.to_hex())
    }
}

impl Serialize for B256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for B256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<B256, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

impl From<B256> for U256 {
    fn from(word: B256) -> U256 {
        U256::from_be_bytes(word.0)
    }
}

#[cfg(test)]
#[path = "tests/b256.rs"]
mod tests;
