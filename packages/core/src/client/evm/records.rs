//! Receipts, logs and fee history as JSON-RPC answers them. Quantities are exact integers; a
//! block's gas used ratio, the one float a node writes, is kept as the digits it wrote.

use serde::{Deserialize, Deserializer, de};

use crate::client::evm::u256::{B256, U256};
use crate::utils::{Address, Figure};

/// What `eth_getTransactionReceipt` answers once a transaction is mined.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub transaction_hash: B256,
    #[serde(default, deserialize_with = "opt_quantity")]
    pub transaction_index: Option<u64>,
    #[serde(default)]
    pub block_hash: Option<B256>,
    #[serde(default, deserialize_with = "opt_quantity")]
    pub block_number: Option<u64>,
    pub from: Address,
    /// `None` for a creation.
    #[serde(default)]
    pub to: Option<Address>,
    #[serde(deserialize_with = "quantity")]
    pub gas_used: u64,
    #[serde(deserialize_with = "quantity")]
    pub cumulative_gas_used: u64,
    /// What each unit of gas cost, in wei.
    #[serde(default)]
    pub effective_gas_price: Option<U256>,
    /// `true` when it succeeded, `false` when it reverted.
    #[serde(default, deserialize_with = "opt_status")]
    pub status: Option<bool>,
    #[serde(default)]
    pub logs: Vec<Log>,
    /// The transaction type: 2 for EIP-1559.
    #[serde(default, rename = "type", deserialize_with = "opt_quantity")]
    pub kind: Option<u64>,
}

impl Receipt {
    /// Mined and not reverted.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.status == Some(true)
    }
}

/// One log, from a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    pub address: Address,
    pub topics: Vec<B256>,
    #[serde(deserialize_with = "bytes")]
    pub data: Vec<u8>,
    #[serde(default, deserialize_with = "opt_quantity")]
    pub log_index: Option<u64>,
    /// Dropped by a reorganisation.
    #[serde(default)]
    pub removed: bool,
}

/// What `eth_feeHistory` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistory {
    #[serde(deserialize_with = "quantity")]
    pub oldest_block: u64,
    /// One per block asked for, and one more: the next block's.
    pub base_fee_per_gas: Vec<U256>,
    /// Each block's gas used over its limit, as the node wrote it.
    #[serde(default)]
    pub gas_used_ratio: Vec<Figure>,
    /// Per block, the tip at each percentile asked for.
    #[serde(default)]
    pub reward: Vec<Vec<U256>>,
}

impl FeeHistory {
    /// The base fee of the block after the newest one asked for.
    #[must_use]
    pub fn next_base_fee(&self) -> Option<U256> {
        self.base_fee_per_gas.last().copied()
    }
}

/// A quantity that fits a `u64`: block numbers, nonces, gas.
fn quantity<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = U256::deserialize(deserializer)?;
    value.to_u64().ok_or_else(|| de::Error::custom(format!("{value} does not fit 64 bits")))
}

fn opt_quantity<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    match Option::<U256>::deserialize(deserializer)? {
        Some(value) => {
            value.to_u64().map(Some).ok_or_else(|| de::Error::custom(format!("{value} does not fit 64 bits")))
        }
        None => Ok(None),
    }
}

fn opt_status<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    match opt_quantity(deserializer)? {
        Some(0) => Ok(Some(false)),
        Some(1) => Ok(Some(true)),
        Some(other) => Err(de::Error::custom(format!("{other} is not a receipt status"))),
        None => Ok(None),
    }
}

/// `0x` and hex bytes.
fn bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
    let digits = text.strip_prefix("0x").ok_or_else(|| de::Error::custom(format!("`{text}` is not 0x and hex")))?;
    crate::utils::hex::decode(digits).map_err(de::Error::custom)
}

#[cfg(test)]
#[path = "tests/records.rs"]
mod tests;
