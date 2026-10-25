//! The L1 a network settles on: Ethereum for Lighter, Robinhood Chain for Lighter on Robinhood
//! Chain ([`crate::client::Network::l1_chain_id`]). Deposits are made there, secure withdrawals
//! arrive there, and priority transactions reach the venue from there. Calldata and transaction
//! encoding are pure functions; only [`rpc::Rpc`] does I/O, over any [`crate::client::Http`].
//! Quantities are exact integers ([`U256`]) and amounts convert to [`crate::utils::Dec`].
//!
//! - [`rpc`]: typed JSON-RPC, a transaction's nonce, gas and fees, its receipt.
//! - [`tx`]: EIP-1559 transactions, signed by the owner's
//!   [`LocalWallet`](crate::account::keys::LocalWallet) or a wallet elsewhere.
//! - [`zklighter`]: the ZkLighter contract's calls and answers.
//! - [`abi`]: Solidity's ABI encoding.
//! - [`rlp`]: the encoding a transaction is hashed and sent in.
//! - [`records`]: receipts, logs and fee history.
//! - [`u256`]: the EVM's word.

pub mod abi;
pub mod records;
pub mod rlp;
pub mod rpc;
pub mod tx;
pub mod u256;
pub mod zklighter;

pub use abi::{AbiError, Function, Kind, Token};
pub use records::{FeeHistory, Log, Receipt};
pub use rpc::{BlockTag, CallRequest, ErrorKind, L1Error, NodeError, Query, Revert, Rpc, RpcError, eth};
pub use tx::{AccessItem, Call, Eip1559, Fees, tx_hash};
pub use u256::{B256, BadAmount, BadQuantity, NATIVE_DECIMALS, U256};
