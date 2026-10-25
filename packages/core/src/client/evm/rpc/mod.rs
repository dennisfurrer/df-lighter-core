//! Typed Ethereum JSON-RPC: each method is a [`Query`] that writes its request body and reads its
//! answer, and [`Rpc`] sends one over any [`Http`](crate::client::Http) to the network's L1
//! ([`Network::l1_rpc_url`](crate::client::Network::l1_rpc_url)) or a node the caller brings.
//!
//! - [`query`]: one method with its params, and how its answer reads.
//! - [`eth`]: every method this crate asks, as a [`Query`].
//! - [`client`]: [`Rpc`], which sends them, prepares a transaction and waits for its receipt.
//! - [`error`]: a node's refusal, a revert's reason, and [`RpcError`].
//!
//! A raw transaction is the one signed request: an `offline` build's wires refuse it.

pub mod client;
pub mod error;
pub mod eth;
pub mod query;

use std::fmt;

pub use client::{RECEIPT_POLL_MS, RECEIPT_TRIES, Rpc};
pub use error::{ErrorKind, L1Error, NodeError, Revert, RpcError, read_revert};
pub use query::Query;

use crate::client::evm::tx::Call;
use crate::client::evm::u256::U256;
use crate::utils::{Address, Json, hex};

/// Which block a read is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockTag {
    #[default]
    Latest,
    /// Counting what waits in the pool: the next nonce to use.
    Pending,
    Safe,
    Finalized,
    Number(u64),
}

impl fmt::Display for BlockTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockTag::Latest => f.write_str("latest"),
            BlockTag::Pending => f.write_str("pending"),
            BlockTag::Safe => f.write_str("safe"),
            BlockTag::Finalized => f.write_str("finalized"),
            BlockTag::Number(number) => write!(f, "{number:#x}"),
        }
    }
}

/// The call object `eth_call` and `eth_estimateGas` take.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CallRequest {
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub gas: Option<u64>,
    pub value: Option<U256>,
    pub data: Vec<u8>,
}

impl CallRequest {
    /// `data` sent to a contract: a read.
    #[must_use]
    pub fn to(to: Address, data: Vec<u8>) -> CallRequest {
        CallRequest { to: Some(to), data, ..CallRequest::default() }
    }

    /// What `call` does, as sent by `from`: what a transaction's gas is estimated with.
    #[must_use]
    pub fn of(from: Option<Address>, call: &Call) -> CallRequest {
        CallRequest {
            from,
            to: call.to,
            gas: None,
            value: (!call.value.is_zero()).then_some(call.value),
            data: call.data.clone(),
        }
    }

    pub(super) fn json(&self) -> Json {
        let mut fields = Vec::new();
        if let Some(from) = self.from {
            fields.push(("from".to_owned(), Json::String(from.to_hex())));
        }
        if let Some(to) = self.to {
            fields.push(("to".to_owned(), Json::String(to.to_hex())));
        }
        if let Some(gas) = self.gas {
            fields.push(("gas".to_owned(), Json::String(U256::from_u64(gas).to_quantity())));
        }
        if let Some(value) = self.value {
            fields.push(("value".to_owned(), Json::String(value.to_quantity())));
        }
        if !self.data.is_empty() {
            fields.push(("data".to_owned(), Json::String(hex::encode_prefixed(&self.data))));
        }
        Json::Object(fields)
    }
}

#[cfg(test)]
#[path = "tests/rpc.rs"]
mod tests;
