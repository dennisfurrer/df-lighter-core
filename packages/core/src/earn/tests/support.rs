//! What the earn and deploy tests share: a transaction held to its lighter-go vector, a vector's
//! fields by their Go names, and a client with a key against a wire that answers from a queue.

#![cfg(feature = "sign")]

use serde_json::Value;

use crate::account::keys::ApiKey;
use crate::client::{Client, Network, Response, TransportError};
use crate::test_support::{Wire, ok, replay, vector, vector_envelope, vector_key};
use crate::utils::sign::tx::{Tx, sign_drawing};
use crate::utils::{AccountIndex, ApiKeyIndex};

pub(crate) const NOW: u64 = 1_790_000_000_000;

fn clock() -> u64 {
    NOW
}

/// Signs `tx` with the case's key, envelope and draws, and holds its type, hash and `tx_info` to
/// what lighter-go wrote.
pub(crate) fn matches(name: &str, tx: &impl Tx) {
    let case = vector(name);
    let (chain_id, envelope) = vector_envelope(&case);
    let signed = sign_drawing(tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
    let out = &case["out"];
    assert_eq!(u64::from(signed.tx_type().code()), out["tx_type"].as_u64().unwrap(), "{name}: type");
    assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
    assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
    assert_eq!(signed.l1_message(), None, "{name}: no owner's signature");
}

/// A vector's fields, by their Go names.
pub(crate) fn fields(name: &str) -> Value {
    vector(name)["fields"].clone()
}

pub(crate) fn field(fields: &Value, name: &str) -> u64 {
    fields[name].as_u64().unwrap_or_else(|| panic!("no field {name}"))
}

/// A client for `account` at slot 4 on `network`, with the vectors' key, answering `answers`.
pub(crate) fn keyed(
    network: Network,
    account: u64,
    answers: impl IntoIterator<Item = Result<Response, TransportError>>,
) -> Client<Wire, ApiKey> {
    let key = ApiKey::new(
        AccountIndex::new(account).unwrap(),
        ApiKeyIndex::new(4).unwrap(),
        vector_key(&vector("register-key-a")),
    );
    Client::new(Wire::answering(answers), network, clock, key)
}

pub(crate) fn nonce(nonce: u64) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"code":200,"nonce":{nonce}}}"#))
}

pub(crate) fn taken(hash: &str) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"code":200,"tx_hash":"{hash}"}}"#))
}

/// Each request's path and query, the host cut.
pub(crate) fn paths(client: &Client<Wire, ApiKey>) -> Vec<String> {
    let host = client.network().api_url();
    client.http().sent().iter().map(|request| request.url.replacen(host, "", 1)).collect()
}

/// A form body's fields, decoded.
pub(crate) fn form(body: &[u8]) -> Vec<(String, String)> {
    let decode = |text: &str| {
        let bytes = text.replace('+', " ").into_bytes();
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'%' => {
                    out.push(u8::from_str_radix(std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap(), 16).unwrap());
                    at += 3;
                }
                byte => {
                    out.push(byte);
                    at += 1;
                }
            }
        }
        String::from_utf8(out).unwrap()
    };
    String::from_utf8_lossy(body)
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

/// The `tx_type` and `tx_info` the last `sendTx` carried.
pub(crate) fn last_sent(client: &Client<Wire, ApiKey>) -> (u64, Value) {
    let sent = client.http().sent();
    let fields = form(&sent.last().unwrap().body);
    let value = |key: &str| fields.iter().find(|(name, _)| name == key).unwrap().1.clone();
    (value("tx_type").parse().unwrap(), serde_json::from_str(&value("tx_info")).unwrap())
}
