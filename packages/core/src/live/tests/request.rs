//! The frames sent, byte for byte as the docs and the Python SDK write them.

use super::*;
use crate::utils::{AccountIndex, MarketId};

fn book() -> Subscription {
    Subscription::OrderBook { market: MarketId::new(1).unwrap() }
}

fn orders() -> Subscription {
    Subscription::AccountAllOrders { account: AccountIndex::new(10).unwrap() }
}

#[test]
fn a_subscribe_carries_the_token_only_where_the_channel_takes_it() {
    assert_eq!(subscribe(&book(), None), r#"{"type":"subscribe","channel":"order_book/1"}"#);
    assert_eq!(subscribe(&book(), Some("token")), r#"{"type":"subscribe","channel":"order_book/1"}"#, "never sent");
    assert_eq!(
        subscribe(&orders(), Some("1792925000:10:4:abc")),
        r#"{"type":"subscribe","channel":"account_all_orders/10","auth":"1792925000:10:4:abc"}"#
    );
    assert_eq!(subscribe(&orders(), None), r#"{"type":"subscribe","channel":"account_all_orders/10"}"#);
    assert_eq!(unsubscribe(&orders()), r#"{"type":"unsubscribe","channel":"account_all_orders/10"}"#);
    assert_eq!(resubscribe(&book(), None), [unsubscribe(&book()), subscribe(&book(), None)]);
    assert_eq!((PING, PONG), (r#"{"type":"ping"}"#, r#"{"type":"pong"}"#));
}

/// `tx_info` goes as the object it is, keys in their order and digits as written, as the Python
/// example sends `json.loads(tx_info)`.
#[test]
fn a_transaction_goes_with_its_info_as_signed() {
    let info = r#"{"AccountIndex":10,"ApiKeyIndex":4,"MarketIndex":1,"Index":404,"ExpiredAt":1700000000000,"Nonce":1234,"Sig":"c2ln"}"#;
    assert_eq!(
        send_tx("7", 15, info).unwrap(),
        format!(r#"{{"type":"jsonapi/sendtx","data":{{"id":"7","tx_type":15,"tx_info":{info}}}}}"#)
    );
    assert!(send_tx("7", 15, "not json").is_err());
}

/// The types and the infos go as JSON lists written into strings, each info itself a string, as
/// the docs write them and the REST batch sends them.
#[test]
fn a_batch_goes_with_its_lists_written_into_strings() {
    let txs = [(14, r#"{"Nonce":1}"#.to_owned()), (15, r#"{"Nonce":2}"#.to_owned())];
    assert_eq!(
        send_tx_batch("8", &txs),
        r#"{"type":"jsonapi/sendtxbatch","data":{"id":"8","tx_types":"[14,15]","tx_infos":"[\"{\\\"Nonce\\\":1}\",\"{\\\"Nonce\\\":2}\"]"}}"#
    );
    assert!(batch_fits(1) && batch_fits(50));
    assert!(!batch_fits(0) && !batch_fits(51));
}
