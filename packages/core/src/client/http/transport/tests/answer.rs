//! `sendTx` and `sendTxBatch`: the form they post and what they answer.

use super::*;
use crate::client::http::transport::codes;
use crate::test_support::{Recorded, block_on};

#[test]
fn a_transaction_posts_its_type_and_info_and_answers_its_hash() {
    let transport = Recorded::answering(
        r#"{"code":200,"message":"{\"ratelimit\": \"didn't use volume quota\"}","tx_hash":"ab12","predicted_execution_time_ms":1790000000123,"volume_quota_remaining":42}"#,
    );
    let sent = block_on(send_tx(&transport, 14, r#"{"Nonce":1}"#.to_owned())).unwrap();
    assert_eq!(
        sent,
        Sent {
            tx_hash: "ab12".to_owned(),
            predicted_execution_time_ms: Some(1_790_000_000_123),
            volume_quota_remaining: Some(42)
        }
    );
    let call = transport.only();
    assert_eq!((call.method, call.path), (crate::client::Method::Post, "sendTx"));
    assert_eq!(call.fields, [("tx_type", "14".to_owned()), ("tx_info", r#"{"Nonce":1}"#.to_owned())]);
}

#[test]
fn a_batch_posts_its_types_and_infos_as_json_lists() {
    let transport = Recorded::answering(r#"{"code":200,"tx_hash":["aa","bb"]}"#);
    let txs = [(14, r#"{"Nonce":1}"#.to_owned()), (15, r#"{"Nonce":2}"#.to_owned())];
    let sent = block_on(send_tx_batch(&transport, &txs)).unwrap();
    assert_eq!(sent.tx_hashes, ["aa", "bb"]);
    let call = transport.only();
    assert_eq!(call.path, "sendTxBatch");
    assert_eq!(
        call.fields,
        [("tx_types", "[14,15]".to_owned()), ("tx_infos", r#"["{\"Nonce\":1}","{\"Nonce\":2}"]"#.to_owned())]
    );
}

#[test]
fn a_refused_transaction_is_its_code() {
    let transport = Recorded::answering(r#"{"code":21739,"message":"not enough margin to create the order"}"#);
    let error = block_on(send_tx(&transport, 14, "{}".to_owned())).unwrap_err();
    assert_eq!(error.code(), Some(codes::NOT_ENOUGH_ORDER_MARGIN));
    assert!(!error.may_have_landed());
}
