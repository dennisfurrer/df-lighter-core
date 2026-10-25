//! Transactions as the venue recorded them: one by its hash or its sequence index (`tx`), the
//! L2 transaction an L1 one (a deposit, a priority transaction) became (`txFromL1TxHash`), and an
//! account's own, newest first (`accountTxs`, with the auth token). `tx` and `txFromL1TxHash` are
//! public: they are how a program learns whether a transaction it sent executed.
//!
//! `info` is the transaction as it was signed, JSON in a string (`tx_info`); `event_info` what the
//! sequencer did with it, in the venue's short keys (see the docs' Data Structures).

use serde::{Deserialize, Deserializer};

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::json::{BadJson, Json};
use crate::utils::lenient::{list, maybe_account, text, unsigned};
use crate::utils::sign::tx::TxType;
use crate::utils::{AccountIndex, ApiKeyIndex};

/// The most transactions one `accountTxs` page holds.
pub const MOST_TXS: u8 = 100;

/// Where a transaction stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TxStatus {
    /// 0: refused by the sequencer. Whether it used its nonce depends on where it failed.
    Failed,
    /// 1: taken, not yet executed.
    Pending,
    /// 2: executed.
    Executed,
    /// 3: executed and final. The docs name it "Pending - Final State"; an executed transaction
    /// reads 3 once its block is final (seen live: 2 when executed, 3 a day later).
    Final,
    /// A code this build does not know.
    Other(u8),
}

impl TxStatus {
    #[must_use]
    pub const fn from_code(code: u8) -> TxStatus {
        match code {
            0 => TxStatus::Failed,
            1 => TxStatus::Pending,
            2 => TxStatus::Executed,
            3 => TxStatus::Final,
            code => TxStatus::Other(code),
        }
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            TxStatus::Failed => 0,
            TxStatus::Pending => 1,
            TxStatus::Executed => 2,
            TxStatus::Final => 3,
            TxStatus::Other(code) => code,
        }
    }

    /// Executed, final or not yet.
    #[must_use]
    pub const fn is_executed(self) -> bool {
        matches!(self, TxStatus::Executed | TxStatus::Final)
    }

    #[must_use]
    pub const fn is_failed(self) -> bool {
        matches!(self, TxStatus::Failed)
    }

    /// Taken and not executed yet.
    #[must_use]
    pub const fn is_pending(self) -> bool {
        matches!(self, TxStatus::Pending)
    }
}

impl<'de> Deserialize<'de> for TxStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = unsigned(deserializer)?;
        Ok(TxStatus::from_code(u8::try_from(code).unwrap_or(u8::MAX)))
    }
}

/// One transaction.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Transaction {
    /// 80 hex digits for an L2 transaction.
    pub hash: String,
    /// lighter-go's `TxType` number; [`Transaction::kind`] names the ones this crate signs.
    #[serde(rename = "type")]
    pub tx_type: u8,
    /// The transaction as signed: JSON in a string.
    #[serde(default, deserialize_with = "text")]
    pub info: String,
    /// What it did: JSON in a string, in the venue's short keys.
    #[serde(default, deserialize_with = "text")]
    pub event_info: String,
    pub status: TxStatus,
    #[serde(default, deserialize_with = "unsigned")]
    pub transaction_index: u64,
    /// The account owner's L1 address as the venue writes it; may be empty.
    #[serde(default, deserialize_with = "text")]
    pub l1_address: String,
    /// The account that signed it; `None` when the venue writes none.
    #[serde(default, deserialize_with = "maybe_account")]
    pub account_index: Option<AccountIndex>,
    #[serde(default, deserialize_with = "unsigned")]
    pub nonce: u64,
    /// When it lapses, unix milliseconds.
    #[serde(default, deserialize_with = "unsigned")]
    pub expire_at: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub block_height: u64,
    /// When the API took it and when it ran, unix milliseconds.
    #[serde(default, deserialize_with = "unsigned")]
    pub queued_at: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub executed_at: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub sequence_index: u64,
    /// The batch transaction it came in, if any.
    #[serde(default, deserialize_with = "text")]
    pub parent_hash: String,
    #[serde(default)]
    pub api_key_index: Option<u8>,
    /// Unix microseconds.
    #[serde(default, deserialize_with = "unsigned")]
    pub transaction_time: u64,
    /// When its block was committed and verified on L1, unix milliseconds; 0 until then.
    #[serde(default, deserialize_with = "unsigned")]
    pub committed_at: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub verified_at: u64,
}

impl Transaction {
    /// The type, when it is one this crate signs.
    #[must_use]
    pub fn kind(&self) -> Option<TxType> {
        TxType::from_code(self.tx_type)
    }

    /// The API key slot that signed it.
    #[must_use]
    pub fn slot(&self) -> Option<ApiKeyIndex> {
        self.api_key_index.and_then(|slot| ApiKeyIndex::new(slot).ok())
    }

    /// `info` read, keys in the order signed.
    pub fn info_json(&self) -> Result<Json, BadJson> {
        Json::parse(&self.info)
    }

    /// `event_info` read.
    pub fn event_json(&self) -> Result<Json, BadJson> {
        Json::parse(&self.event_info)
    }

    /// The application error the sequencer wrote (`ae`), when there is one: why it failed.
    #[must_use]
    pub fn app_error(&self) -> Option<String> {
        let event = self.event_json().ok()?;
        event.get("ae").and_then(Json::as_str).filter(|error| !error.is_empty()).map(str::to_owned)
    }
}

/// What `tx` asks: one by its hash.
#[must_use]
pub fn tx_call(hash: &str) -> Call {
    Call::get("tx").field("by", "hash").field("value", hash.trim_start_matches("0x"))
}

/// What `tx` asks: one by its sequence index.
#[must_use]
pub fn tx_by_sequence_call(sequence_index: u64) -> Call {
    Call::get("tx").field("by", "sequence_index").field("value", sequence_index)
}

/// What `txFromL1TxHash` asks. Weight 50.
#[must_use]
pub fn tx_from_l1_tx_hash_call(l1_hash: &str) -> Call {
    Call::get("txFromL1TxHash").field("hash", l1_hash)
}

/// A transaction by its hash, as `sendTx` answered it.
pub async fn tx(transport: &impl Transport, hash: &str) -> Result<Transaction, ApiError> {
    read(transport, tx_call(hash)).await
}

pub async fn tx_by_sequence(transport: &impl Transport, sequence_index: u64) -> Result<Transaction, ApiError> {
    read(transport, tx_by_sequence_call(sequence_index)).await
}

/// The L2 transaction an L1 transaction (`0x` and 64 hex digits) became.
pub async fn tx_from_l1_tx_hash(transport: &impl Transport, l1_hash: &str) -> Result<Transaction, ApiError> {
    read(transport, tx_from_l1_tx_hash_call(l1_hash)).await
}

/// What `accountTxs` asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxQuery {
    pub account: AccountIndex,
    /// Where the page starts, by sequence index (`index`): the oldest of the last page, to go
    /// further back. `None` for the newest.
    pub before: Option<u64>,
    /// Only these types (each by its number); empty for every type.
    pub types: Vec<u8>,
    /// 1 to 100.
    pub limit: u8,
}

impl TxQuery {
    /// The newest hundred.
    #[must_use]
    pub const fn new(account: AccountIndex) -> TxQuery {
        TxQuery { account, before: None, types: Vec::new(), limit: MOST_TXS }
    }
}

#[derive(Deserialize)]
struct Txs {
    #[serde(default, deserialize_with = "list")]
    txs: Vec<Transaction>,
}

/// What `accountTxs` asks for `query`; each type its own `types` field.
#[must_use]
pub fn account_txs_call(query: &TxQuery) -> Call {
    let call = Call::get("accountTxs")
        .maybe("index", query.before)
        .field("limit", query.limit.clamp(1, MOST_TXS))
        .field("by", "account_index")
        .field("value", query.account);
    query.types.iter().fold(call, |call, tx_type| call.field("types", tx_type)).authed()
}

/// A page of the account's transactions, newest first.
pub async fn account_txs(transport: &impl Transport, query: &TxQuery) -> Result<Vec<Transaction>, ApiError> {
    Ok(read::<Txs>(transport, account_txs_call(query)).await?.txs)
}

#[cfg(test)]
#[path = "tests/txs.rs"]
mod tests;
