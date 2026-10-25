//! What `sendTx` and `sendTxBatch` answer, and the posts themselves: a transaction taken, never
//! executed yet. The sequencer may still refuse it; the account's `account_tx` channel or a read
//! of the transaction by its hash says how it went.

use serde::Deserialize;

use crate::client::http::transport::calls::{ApiError, Call, Transport, read};

/// A transaction the API took.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Sent {
    /// 80 hex digits, the hash the API key signed.
    pub tx_hash: String,
    /// When the sequencer expects to run it, unix milliseconds: a prediction, not a time it ran.
    #[serde(default)]
    pub predicted_execution_time_ms: Option<u64>,
    /// What a Plus or Premium account's volume quota has left after it.
    #[serde(default)]
    pub volume_quota_remaining: Option<u64>,
}

/// A batch the API took, every transaction in it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SentBatch {
    /// One hash a transaction, in the order sent.
    #[serde(rename = "tx_hash")]
    pub tx_hashes: Vec<String>,
    #[serde(default)]
    pub predicted_execution_time_ms: Option<u64>,
    #[serde(default)]
    pub volume_quota_remaining: Option<u64>,
}

/// The most transactions one batch carries.
pub const MOST_IN_BATCH: usize = 50;

/// `sendTx`: one signed transaction, `tx_info` as it was signed.
#[must_use]
pub fn send_tx_call(tx_type: u8, tx_info: String) -> Call {
    Call::post("sendTx").field("tx_type", tx_type).field("tx_info", tx_info)
}

/// `sendTxBatch`: up to fifty, one account and API key, consecutive nonces. The types and infos
/// go as JSON lists, each info a string.
#[must_use]
pub fn send_tx_batch_call(txs: &[(u8, String)]) -> Call {
    let types: Vec<u8> = txs.iter().map(|(tx_type, _)| *tx_type).collect();
    let infos: Vec<&str> = txs.iter().map(|(_, info)| info.as_str()).collect();
    Call::post("sendTxBatch")
        .field("tx_types", serde_json::to_string(&types).unwrap_or_default())
        .field("tx_infos", serde_json::to_string(&infos).unwrap_or_default())
}

/// Posts a signed transaction once. Never retried, here or by a caller, unless the venue refused
/// it with a code: an answer that never came may mean it was taken.
pub async fn send_tx(transport: &impl Transport, tx_type: u8, tx_info: String) -> Result<Sent, ApiError> {
    read(transport, send_tx_call(tx_type, tx_info)).await
}

/// Posts a signed batch once.
pub async fn send_tx_batch(transport: &impl Transport, txs: &[(u8, String)]) -> Result<SentBatch, ApiError> {
    read(transport, send_tx_batch_call(txs)).await
}

#[cfg(test)]
#[path = "tests/answer.rs"]
mod tests;
