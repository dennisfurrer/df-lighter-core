//! Read-only tokens and maker-only keys. Every one of these needs a token, so the answers here are
//! shaped from the docs' schemas (`RespGetApiTokens`, `RespPostApiToken`, `RespRevokeApiToken`,
//! `RespGetMakerOnlyApiKeys`), not recorded; the posts are only built and recorded, never sent.

use super::*;
use crate::client::Method;
use crate::client::http::form;
use crate::test_support::{Recorded, block_on};

const TOKEN: &str = r#"{"token_id":7,"api_token":"ro:100001:all:2105000000:4f2a","name":"dashboard",
    "account_index":100001,"expiry":2105000000,"sub_account_access":true,"revoked":false,"scopes":"read.*"}"#;

fn account() -> AccountIndex {
    AccountIndex::new(100_001).unwrap()
}

#[test]
fn the_account_s_tokens_read_and_never_print_the_secret() {
    let transport = Recorded::answering(&format!(r#"{{"code":200,"api_tokens":[{TOKEN}]}}"#));
    let tokens = block_on(read_tokens(&transport, account())).unwrap();
    let token = &tokens[0];
    assert_eq!((token.token_id, token.name.as_str(), token.scopes.as_str()), (7, "dashboard", READ_SCOPE));
    assert!(token.api_token.starts_with("ro:100001:all:") && token.sub_account_access);
    assert!(token.is_live(2_104_999_999) && !token.is_live(2_105_000_000));
    assert!(!format!("{token:?}").contains("ro:"), "Debug leaves the token out");
    let call = transport.only();
    assert_eq!((call.method, call.path, call.auth), (Method::Get, "tokens", true));
    assert_eq!(call.fields, [("account_index", "100001".to_owned())]);
}

#[test]
fn a_token_is_made_with_the_read_scope_and_revoked_by_its_id() {
    let transport = Recorded::answering(&format!(r#"{{"code":200,{}"#, &TOKEN[1..]));
    let wanted = NewReadToken {
        account: account(),
        name: "tax tool".to_owned(),
        expiry: 2_105_000_000,
        sub_account_access: false,
    };
    let made = block_on(create_read_token(&transport, &wanted)).unwrap();
    assert_eq!(made.token_id, 7);
    let call = transport.only();
    assert_eq!((call.method, call.path, call.auth), (Method::Post, "tokens/create", true));
    assert_eq!(
        form(&call.fields),
        "name=tax+tool&account_index=100001&expiry=2105000000&sub_account_access=false&scopes=read.*"
    );

    let transport = Recorded::answering(r#"{"code":200,"token_id":7,"revoked":true}"#);
    assert_eq!(block_on(revoke_read_token(&transport, account(), 7)), Ok(true));
    let call = transport.only();
    assert_eq!((call.method, call.path), (Method::Post, "tokens/revoke"));
    assert_eq!(call.fields, [("token_id", "7".to_owned()), ("account_index", "100001".to_owned())]);
    const { assert!(MIN_READ_TOKEN_S < MAX_READ_TOKEN_S) };
}

#[test]
fn maker_only_keys_are_read_and_replaced_as_a_json_list() {
    let transport = Recorded::answering(r#"{"code":200,"api_key_indexes":[4,5,300]}"#);
    let slots = block_on(maker_only_api_keys(&transport, account())).unwrap();
    assert_eq!(slots.iter().map(|slot| slot.get()).collect::<Vec<_>>(), [4, 5], "a slot past 254 is left out");
    assert_eq!(transport.only().path, "getMakerOnlyApiKeys");

    let transport = Recorded::answering(r#"{"code":200}"#);
    let slots = [ApiKeyIndex::new(4).unwrap(), ApiKeyIndex::new(5).unwrap()];
    block_on(set_maker_only_api_keys(&transport, account(), &slots)).unwrap();
    block_on(set_maker_only_api_keys(&transport, account(), &[])).unwrap();
    let sent = transport.sent();
    assert_eq!((sent[0].method, sent[0].path, sent[0].auth), (Method::Post, "setMakerOnlyApiKeys", true));
    assert_eq!(sent[0].value("api_key_indexes"), Some("[4,5]"));
    assert_eq!(sent[1].value("api_key_indexes"), Some("[]"), "an empty list clears it");
}
