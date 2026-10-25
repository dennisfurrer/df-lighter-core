//! Exact 256-bit integers, the EVM's word: balances and allowances in a token's L1 units, gas
//! prices in wei, ABI integers, and ETH's 18 decimals. JSON-RPC writes them as quantities, `0x`
//! and hex digits without leading zeros. Nothing here passes through a float.
//!
//! - [`uint`]: [`U256`] and its checked arithmetic.
//! - [`text`]: quantities and decimal digits.
//! - [`amount`]: units to and from an exact [`crate::utils::Dec`].
//! - [`b256`]: [`B256`], thirty-two bytes.

pub mod amount;
pub mod b256;
pub mod text;
pub mod uint;

pub use amount::{BadAmount, NATIVE_DECIMALS};
pub use b256::B256;
pub use text::BadQuantity;
pub use uint::U256;
