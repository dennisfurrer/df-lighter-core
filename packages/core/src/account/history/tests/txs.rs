//! Transactions: a recorded market order on Lighter (`tx.json`, by hash, which `tx` by sequence
//! answers the same), every field read and its JSON parsed; and the account's own list, asked
//! with the token (shaped from the docs' `Txs`, since it is not public).

use super::*;
use crate::test_support::{Recorded, block_on};

const TX: &str = include_str!("../../../../fixtures/lighter/tx.json");

#[test]
fn a_recorded_transaction_reads_whole_and_its_json_parses() {
    let transport = Recorded::answering(TX);
    let hash = "b5b63eaa396afe74c0bc3e812f5c05ad4f55b3b80e66c4284dea26f5351ed311320924c1e8514aaa";
    let tx = block_on(tx(&transport, hash)).unwrap();
    assert_eq!((tx.hash.as_str(), tx.kind(), tx.status), (hash, Some(TxType::CreateOrder), TxStatus::Executed));
    assert!(tx.status.is_executed() && !tx.status.is_pending());
    assert_eq!(
        (tx.account_index.map(AccountIndex::get), tx.slot().map(ApiKeyIndex::get), tx.nonce),
        (Some(731_664), Some(4), 459)
    );
    assert_eq!((tx.block_height, tx.sequence_index, tx.transaction_index), (365_159_121, 139_166_258_732, 440));
    assert_eq!((tx.queued_at, tx.executed_at, tx.expire_at), (1_792_918_063_880, 1_792_918_063_399, 1_792_918_662_523));
    assert_eq!((tx.committed_at, tx.verified_at, tx.parent_hash.as_str()), (0, 0, ""));
    assert_eq!(tx.l1_address, "0x3FE2B179dbAE3c5637f45E66248120122E45Ab06");
    let info = tx.info_json().unwrap();
    assert_eq!(info.get("ClientOrderIndex").and_then(Json::as_u64), Some(17_928_831_250_668));
    assert_eq!(tx.event_json().unwrap().get("m").and_then(Json::as_u64), Some(1));
    assert_eq!(tx.app_error(), None, "it executed");
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("tx", false));
    assert_eq!(call.fields, [("by", "hash".to_owned()), ("value", hash.to_owned())]);
}

#[test]
fn a_transaction_is_also_found_by_sequence_and_by_its_l1_hash() {
    let transport = Recorded::answering(TX);
    let by_sequence = block_on(tx_by_sequence(&transport, 139_166_258_732)).unwrap();
    assert_eq!(by_sequence.sequence_index, 139_166_258_732);
    let l1 = "0x8f1e5a2b7c3d4e6f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f7";
    block_on(tx_from_l1_tx_hash(&transport, l1)).unwrap();
    let sent = transport.sent();
    assert_eq!(sent[0].fields, [("by", "sequence_index".to_owned()), ("value", "139166258732".to_owned())]);
    assert_eq!((sent[1].path, sent[1].auth, sent[1].value("hash")), ("txFromL1TxHash", false, Some(l1)));
}

#[test]
fn statuses_read_by_number_and_a_failure_says_why() {
    for (code, status) in [
        (0, TxStatus::Failed),
        (1, TxStatus::Pending),
        (2, TxStatus::Executed),
        (3, TxStatus::Final),
        (9, TxStatus::Other(9)),
    ] {
        assert_eq!((TxStatus::from_code(code), TxStatus::from_code(code).code()), (status, code));
    }
    assert!(TxStatus::Final.is_executed() && !TxStatus::Final.is_pending() && TxStatus::Failed.is_failed());
    let failed = TX
        .replace(r#""status":2"#, r#""status":0"#)
        .replace(r#"\"ae\":\"\""#, r#"\"ae\":\"not enough margin\""#)
        .replace(r#""account_index":731664"#, r#""account_index":-1"#);
    let tx: Transaction = serde_json::from_str(&failed).unwrap();
    assert_eq!(
        (tx.status, tx.app_error().as_deref(), tx.account_index),
        (TxStatus::Failed, Some("not enough margin"), None)
    );
}

#[test]
fn the_account_s_transactions_are_asked_with_the_token_each_type_its_own_field() {
    let one = &TX[TX.find(",\"hash\"").unwrap() + 1..];
    let transport = Recorded::answering(&format!(r#"{{"code":200,"txs":[{{{one}]}}"#));
    let query = TxQuery {
        before: Some(139_166_258_800),
        types: vec![14, 15],
        limit: 3,
        ..TxQuery::new(AccountIndex::new(731_664).unwrap())
    };
    let txs = block_on(account_txs(&transport, &query)).unwrap();
    assert_eq!((txs.len(), txs[0].nonce), (1, 459));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("accountTxs", true));
    assert_eq!(
        call.fields,
        [
            ("index", "139166258800".to_owned()),
            ("limit", "3".to_owned()),
            ("by", "account_index".to_owned()),
            ("value", "731664".to_owned()),
            ("types", "14".to_owned()),
            ("types", "15".to_owned()),
        ]
    );
}
