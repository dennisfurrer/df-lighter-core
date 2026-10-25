//! The frames sent: subscribe, unsubscribe, ping and pong, and a signed transaction or batch
//! (`jsonapi/sendtx`, `jsonapi/sendtxbatch`) sent over the socket instead of over HTTP. Each is
//! one text frame, as `JSON.stringify` would write it, keys in the docs' order. Every client frame
//! counts toward the venue's 200 a minute, transactions excepted.

use crate::client::http::MOST_IN_BATCH;
use crate::live::Subscription;
use crate::utils::Json;
use crate::utils::json::BadJson;

/// The heartbeat. The venue answers `{"type":"pong"}`, and closes a socket it has had no frame
/// from for two minutes.
pub const PING: &str = r#"{"type":"ping"}"#;

/// The answer to the venue's own `{"type":"ping"}`.
pub const PONG: &str = r#"{"type":"pong"}"#;

/// `{"type":"subscribe","channel":"order_book/1"}`, with `"auth"` for a channel that takes the
/// account's token ([`Subscription::takes_auth`]) when one is given; a public channel never
/// carries it. The venue answers a channel already subscribed with `Already Subscribed`
/// (30003) and no snapshot.
#[must_use]
pub fn subscribe(subscription: &Subscription, auth: Option<&str>) -> String {
    let mut fields = vec![text("type", "subscribe"), text("channel", &subscription.channel())];
    if let Some(token) = auth.filter(|_| subscription.takes_auth()) {
        fields.push(text("auth", token));
    }
    Json::Object(fields).compact()
}

/// `{"type":"unsubscribe","channel":"order_book/1"}`. The venue answers `unsubscribed`, or
/// `Not Subscribed` (30002).
#[must_use]
pub fn unsubscribe(subscription: &Subscription) -> String {
    Json::Object(vec![text("type", "unsubscribe"), text("channel", &subscription.channel())]).compact()
}

/// An unsubscribe and a subscribe, to send back to back: the venue answers the second with a
/// fresh snapshot, where a second subscribe alone is refused. What a book whose nonces stopped
/// chaining needs.
#[must_use]
pub fn resubscribe(subscription: &Subscription, auth: Option<&str>) -> [String; 2] {
    [unsubscribe(subscription), subscribe(subscription, auth)]
}

/// `{"type":"jsonapi/sendtx","data":{"id":..,"tx_type":..,"tx_info":{..}}}`: one signed
/// transaction, `tx_info` as signed (keys in its order, its digits as written), sent as the object
/// it is. The answer names `id`. Like `sendTx`, never sent twice: one left unanswered may still
/// have landed.
pub fn send_tx(id: &str, tx_type: u8, tx_info: &str) -> Result<String, BadJson> {
    let info = Json::parse(tx_info)?;
    let data = Json::Object(vec![
        text("id", id),
        ("tx_type".to_owned(), Json::Number(tx_type.to_string())),
        ("tx_info".to_owned(), info),
    ]);
    Ok(Json::Object(vec![text("type", "jsonapi/sendtx"), ("data".to_owned(), data)]).compact())
}

/// `{"type":"jsonapi/sendtxbatch","data":{"id":..,"tx_types":"[14,15]","tx_infos":"[..]"}}`: up
/// to fifty, one account and API key, consecutive nonces. As over HTTP, the types and the infos
/// go as JSON lists written into strings, each info itself a string. A frame is at most 64 KB,
/// enough for fifty orders; the venue refuses more than fifty with 21514.
#[must_use]
pub fn send_tx_batch(id: &str, txs: &[(u8, String)]) -> String {
    let types = Json::Array(txs.iter().map(|(tx_type, _)| Json::Number(tx_type.to_string())).collect());
    let infos = Json::Array(txs.iter().map(|(_, info)| Json::String(info.clone())).collect());
    let data =
        Json::Object(vec![text("id", id), text("tx_types", &types.compact()), text("tx_infos", &infos.compact())]);
    Json::Object(vec![text("type", "jsonapi/sendtxbatch"), ("data".to_owned(), data)]).compact()
}

/// Whether a batch of `count` is one the venue takes.
#[must_use]
pub const fn batch_fits(count: usize) -> bool {
    count >= 1 && count <= MOST_IN_BATCH
}

fn text(key: &str, value: &str) -> (String, Json) {
    (key.to_owned(), Json::String(value.to_owned()))
}

#[cfg(test)]
#[path = "tests/request.rs"]
mod tests;
