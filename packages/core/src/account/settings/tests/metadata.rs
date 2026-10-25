//! Metadata and notifications. `accountMetadata` is refused without a token (recorded), so its
//! answer is shaped from the docs' schema (`AccountMetadatas`); the posts are only built and
//! recorded, never sent.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};
use crate::utils::Address;

const METADATA: &str = r##"{"code":200,"account_metadatas":[{"account_index":281474976699106,"name":"desk",
    "description":"market making","can_invite":true,"referral_points_percentage":"","created_at":1782506002,
    "can_rfq":true,"can_rfq_market_ids":["1","2"],"metadata":{"color":"#4f8cff"},"agent_enabled":false}],
    "next_cursor":""}"##;

#[test]
fn an_owner_s_metadata_reads_by_address_with_the_token() {
    let transport = Recorded::answering(METADATA);
    let owner: Address = "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap();
    let page = block_on(account_metadata(&transport, Lookup::L1Address(owner), None)).unwrap();
    let desk = &page.account_metadatas[0];
    assert_eq!((desk.account_index.get(), desk.name.as_str()), (281_474_976_699_106, "desk"));
    assert_eq!((desk.metadata.color.as_str(), desk.can_rfq_market_ids.len()), ("#4f8cff", 2));
    assert_eq!(page.next_cursor.as_deref(), Some(""));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("accountMetadata", true));
    assert_eq!(
        call.fields,
        [("by", "l1_address".to_owned()), ("value", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned())]
    );
}

#[test]
fn without_a_token_the_venue_refuses_it() {
    let transport = Recorded::answering(include_str!("../../../../fixtures/lighter/accountMetadata_no_token.json"));
    let index = AccountIndex::new(10).unwrap();
    let error = block_on(account_metadata(&transport, index.into(), Some("next"))).unwrap_err();
    assert_eq!(error.code(), Some(crate::client::http::codes::INVALID_PARAM));
    assert_eq!(transport.only().value("cursor"), Some("next"));
}

#[test]
fn metadata_and_acknowledgements_are_posted_with_the_token() {
    let transport = Recorded::answering(r#"{"code":200}"#);
    let master = AccountIndex::new(10).unwrap();
    let desk = AccountIndex::new(281_474_976_699_106).unwrap();
    block_on(set_account_metadata(&transport, master, desk, ApiKeyIndex::new(4).unwrap(), r#"{"color":"red"}"#))
        .unwrap();
    block_on(ack_notification(&transport, desk, "liq-42")).unwrap();
    let sent = transport.sent();
    assert_eq!((sent[0].method, sent[0].path, sent[0].auth), (Method::Post, "setAccountMetadata", true));
    let body: serde_json::Value = serde_json::from_str(sent[0].json.as_deref().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "master_account_index": 10,
            "target_account_index": 281_474_976_699_106_u64,
            "api_key_index": 4,
            "metadata": r#"{"color":"red"}"#,
        })
    );
    assert!(sent[0].fields.is_empty(), "no form beside it");
    assert_eq!((sent[1].method, sent[1].path, sent[1].auth), (Method::Post, "notification/ack", true));
    assert_eq!(sent[1].fields, [("notif_id", "liq-42".to_owned()), ("account_index", "281474976699106".to_owned())]);
}
