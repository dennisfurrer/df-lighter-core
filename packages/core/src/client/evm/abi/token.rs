//! A [`Token`], a value of some [`Kind`](crate::client::evm::abi::Kind): built from Rust values
//! and read back into them.

use crate::client::evm::u256::U256;
use crate::utils::Address;

/// A value of some [`Kind`](crate::client::evm::abi::Kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Uint(U256),
    Address(Address),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    /// `T[]`.
    Array(Vec<Token>),
}

impl Token {
    #[must_use]
    pub fn uint(value: impl Into<U256>) -> Token {
        Token::Uint(value.into())
    }

    #[must_use]
    pub const fn address(address: Address) -> Token {
        Token::Address(address)
    }

    #[must_use]
    pub const fn as_u256(&self) -> Option<U256> {
        match self {
            Token::Uint(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_u128(&self) -> Option<u128> {
        self.as_u256().and_then(U256::to_u128)
    }

    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        self.as_u256().and_then(U256::to_u64)
    }

    #[must_use]
    pub fn as_u16(&self) -> Option<u16> {
        self.as_u64().and_then(|value| u16::try_from(value).ok())
    }

    #[must_use]
    pub fn as_u8(&self) -> Option<u8> {
        self.as_u64().and_then(|value| u8::try_from(value).ok())
    }

    #[must_use]
    pub const fn as_address(&self) -> Option<Address> {
        match self {
            Token::Address(address) => Some(*address),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Token::Bool(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Token::String(text) => Some(text),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Token::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_items(&self) -> Option<&[Token]> {
        match self {
            Token::Array(items) => Some(items),
            _ => None,
        }
    }

    /// What it is, for a refusal to name.
    pub(super) fn describe(&self) -> String {
        match self {
            Token::Uint(value) => format!("the integer {value}"),
            Token::Address(address) => format!("the address {address}"),
            Token::Bool(value) => format!("the boolean {value}"),
            Token::String(_) => "text".to_owned(),
            Token::Bytes(bytes) => format!("{} bytes", bytes.len()),
            Token::Array(items) => format!("a list of {}", items.len()),
        }
    }
}
