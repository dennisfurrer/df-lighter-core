//! A transaction signed outside this crate (the venue's Go signer run as WebAssembly, a key held
//! elsewhere), read back before anyone sends it. A positional signer signs a swapped pair of
//! arguments without complaint, so what it returns is held to what was asked: every field of
//! `tx_info` as the transaction writes it, a signature of 80 bytes, and the hash this crate
//! computes for the same transaction.

use crate::utils::base64;
use crate::utils::json::Json;
use crate::utils::sign::tx::{BadTx, Envelope, Tx, number, tx_hash};

/// Holds `tx_info` and `signed_hash`, as another signer returned them for `tx` on `chain_id` and
/// `envelope`, to that transaction: refused at the first field that differs.
pub fn read_back(
    tx: &(impl Tx + ?Sized),
    chain_id: u32,
    envelope: &Envelope,
    tx_info: &str,
    signed_hash: &str,
) -> Result<(), BadTx> {
    let info = Json::parse(tx_info).map_err(|error| BadTx::new("tx_info", format!("not JSON: {error}")))?;
    if info.as_object().is_none() {
        return Err(BadTx::new("tx_info", "not a JSON object"));
    }
    let mut asked: Vec<(&'static str, Json)> =
        vec![(tx.account_key(), number(envelope.account.get())), ("ApiKeyIndex", number(envelope.api_key.get()))];
    asked.extend(tx.info_fields());
    asked.extend([
        ("ExpiredAt", number(envelope.expired_at)),
        ("Nonce", number(envelope.nonce)),
        ("L2TxAttributes", tx.attributes().json()),
    ]);
    for (key, value) in asked {
        match info.get(key) {
            Some(signed) if *signed == value => {}
            Some(signed) => {
                return Err(BadTx::new(key, format!("signed as {}, asked {}", signed.compact(), value.compact())));
            }
            None => return Err(BadTx::new(key, "missing from what was signed")),
        }
    }
    let signature = info.get("Sig").and_then(Json::as_str).and_then(|text| base64::decode(text).ok());
    if signature.is_none_or(|bytes| bytes.len() != 80) {
        return Err(BadTx::new("Sig", "not a signature of 80 bytes"));
    }
    let hash = tx_hash(tx, chain_id, envelope).to_hex();
    if !signed_hash.eq_ignore_ascii_case(&hash) {
        return Err(BadTx::new("tx_hash", format!("signed {signed_hash}, the transaction hashes to {hash}")));
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/read_back.rs"]
mod tests;
