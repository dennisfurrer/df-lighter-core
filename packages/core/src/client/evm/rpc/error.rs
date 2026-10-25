//! Why a query failed: a node's JSON-RPC error and what its code says, the reason a call
//! reverted, [`RpcError`], and [`L1Error`] for a transaction signed and sent here.

use std::fmt;

use crate::account::keys::WalletError;
use crate::client::TransportError;
use crate::client::evm::abi::{self, Kind, Token};
use crate::client::evm::u256::U256;
use crate::utils::hex;

/// A JSON-RPC error object, as the node wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeError {
    pub code: i64,
    pub message: String,
    /// `data`: a string's text, or the JSON as written. A revert's data is hex.
    pub data: Option<String>,
}

/// What a [`NodeError`]'s code says, per JSON-RPC and the Ethereum nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// -32700: the body was not JSON.
    Parse,
    /// -32600.
    InvalidRequest,
    /// -32601: the node does not serve this method.
    MethodNotFound,
    /// -32602.
    InvalidParams,
    /// -32603.
    Internal,
    /// 3, or a server error that says `execution reverted`.
    Reverted,
    /// -32000 to -32099: the node's own, `insufficient funds` and `nonce too low` among them.
    Server,
    Other,
}

impl NodeError {
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self.code {
            -32700 => ErrorKind::Parse,
            -32600 => ErrorKind::InvalidRequest,
            -32601 => ErrorKind::MethodNotFound,
            -32602 => ErrorKind::InvalidParams,
            -32603 => ErrorKind::Internal,
            3 => ErrorKind::Reverted,
            -32099..=-32000 if self.message.starts_with("execution reverted") => ErrorKind::Reverted,
            -32099..=-32000 => ErrorKind::Server,
            _ => ErrorKind::Other,
        }
    }

    /// Why a call reverted, when the node passed the revert data on.
    #[must_use]
    pub fn revert(&self) -> Option<Revert> {
        let data = self.data.as_deref()?.strip_prefix("0x")?;
        hex::decode(data).ok().map(|bytes| read_revert(&bytes))
    }
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)?;
        match &self.data {
            Some(data) => write!(f, ": {data}"),
            None => Ok(()),
        }
    }
}

/// What a revert's data says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revert {
    /// `Error(string)`, from `require` and `revert("...")`: an allowance too small says
    /// `ERC20: transfer amount exceeds allowance`.
    Message(String),
    /// `Panic(uint256)`: 0x11 is an overflow, 0x01 a failed assert.
    Panic(U256),
    /// A custom error, or nothing at all: the bytes as they came.
    Custom(Vec<u8>),
}

/// `Error(string)`.
const ERROR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];
/// `Panic(uint256)`.
const PANIC: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];

/// Reads revert data: a message, a panic code, or the bytes of anything else.
#[must_use]
pub fn read_revert(data: &[u8]) -> Revert {
    let custom = || Revert::Custom(data.to_vec());
    let Some((selector, rest)) = data.split_first_chunk::<4>() else {
        return custom();
    };
    let shape = match *selector {
        ERROR => Kind::String,
        PANIC => Kind::Uint(256),
        _ => return custom(),
    };
    match abi::decode(&[shape], rest).as_deref() {
        Ok([Token::String(text)]) => Revert::Message(text.clone()),
        Ok([Token::Uint(code)]) => Revert::Panic(*code),
        _ => custom(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RpcError {
    /// No answer, a rate limit, or an HTTP status with no JSON-RPC error in it.
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("{method}: the node refused: {error}")]
    Node { method: &'static str, error: NodeError },
    #[error("{method}: the node's answer could not be read: {detail}")]
    Unreadable { method: &'static str, detail: String },
}

impl RpcError {
    /// Whether a raw transaction this answered may have been taken: no answer, or one that could
    /// not be read. Look for its receipt before sending it again.
    #[must_use]
    pub const fn may_have_landed(&self) -> bool {
        matches!(self, RpcError::Transport(TransportError::Unreachable(_)) | RpcError::Unreadable { .. })
            || matches!(self, RpcError::Transport(TransportError::Status { status, .. }) if *status >= 500)
    }
}

/// Why a transaction signed here was not sent, or not confirmed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum L1Error {
    #[error(transparent)]
    Rpc(#[from] RpcError),
    /// The wallet did not sign: nothing was sent.
    #[error(transparent)]
    Wallet(#[from] WalletError),
    /// The node is on another chain than the transaction is for: nothing was sent.
    #[error("the node is on chain {found}, not {expected}; nothing was sent")]
    WrongChain { expected: u64, found: u64 },
}
