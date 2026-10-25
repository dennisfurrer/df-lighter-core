//! Solidity's ABI encoding, as far as the ZkLighter contract and an ERC-20 need it: unsigned
//! integers to 256 bits, addresses, booleans, `bytes`, strings and arrays of them. Types are read
//! from a signature's text, and every value and word is checked against its type.
//!
//! - [`kind`]: the types, read from their text.
//! - [`token`]: the values.
//! - [`encode`](mod@encode) and [`decode`](mod@decode): `abi.encode` and `abi.decode`.
//! - [`function`]: a function's selector, calldata and answer.
//! - [`error`]: why a type, a value or a word is refused.

pub mod decode;
pub mod encode;
pub mod error;
pub mod function;
pub mod kind;
pub mod token;

pub use decode::decode;
pub use encode::encode;
pub use error::AbiError;
pub use function::{Function, selector};
pub use kind::{Kind, kinds};
pub use token::Token;

const WORD: usize = 32;

/// The most a length or offset inside the data is believed before it is refused.
const LONGEST: usize = 1 << 20;

#[cfg(test)]
#[path = "tests/abi.rs"]
mod tests;
