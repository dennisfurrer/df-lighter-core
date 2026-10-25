//! A call's path and fields, and reading the venue's answer: the value, or its refusal by code,
//! with what each refusal means for a transaction that was sent.

use serde_json::Value;

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn a_call_is_its_path_under_api_v1_and_its_fields_in_order() {
    let call =
        Call::get("orderBookDetails").field("market_id", 1).maybe("filter", None::<&str>).maybe("limit", Some(5));
    assert_eq!(call.url_path(), "/api/v1/orderBookDetails");
    assert_eq!(call.fields, [("market_id", "1".to_owned()), ("limit", "5".to_owned())]);
    assert_eq!((call.value("limit"), call.value("filter"), call.auth), (Some("5"), None, false));
    assert_eq!(Call::get("/info").url_path(), "/info", "from the root");
    assert_eq!(Call::get("/").url_path(), "/");
    let post = Call::post("tokens/create").authed();
    assert_eq!((post.method, post.auth), (Method::Post, true));
}

#[test]
fn an_answer_is_its_value_unless_its_code_refuses() {
    for text in [r#"{"code":200,"nonce":4}"#, r#"{"nonce":4}"#, r#"{"code":0,"nonce":4}"#] {
        let value: Value = decode("/api/v1/nextNonce", text).unwrap();
        assert_eq!(value["nonce"], 4, "{text}");
    }
    let refused = decode::<Value>("/api/v1/sendTx", r#"{"code":21120,"message":"invalid signature"}"#).unwrap_err();
    assert_eq!(refused, ApiError::Refused { code: 21120, message: "invalid signature".to_owned() });
    assert_eq!(refused.code(), Some(codes::INVALID_SIGNATURE));
    let unreadable = decode::<Vec<u8>>("/api/v1/orderBooks", "{not json").unwrap_err();
    assert!(matches!(&unreadable, ApiError::Unreadable { path, .. } if path == "/api/v1/orderBooks"), "{unreadable}");
}

#[test]
fn a_refusal_with_an_error_status_is_read_from_its_body() {
    let body = r#"{"code":21100,"message":"account not found"}"#.to_owned();
    let transport = Recorded::failing(TransportError::Status { status: 400, body });
    let error = block_on(read::<Value>(&transport, Call::get("account"))).unwrap_err();
    assert_eq!(error.code(), Some(codes::ACCOUNT_NOT_FOUND));
    let transport = Recorded::failing(TransportError::Status { status: 403, body: "<html>".to_owned() });
    let error = block_on(read::<Value>(&transport, Call::get("txs"))).unwrap_err();
    assert!(matches!(error, ApiError::Transport(TransportError::Status { status: 403, .. })), "{error}");
}

/// What a refusal says about a transaction: whether to wait, to ask the nonce again, to
/// register the key, and whether it may have landed anyway.
#[test]
fn each_refusal_says_whether_the_transaction_may_have_landed() {
    let refused = |code| ApiError::Refused { code, message: String::new() };
    let unreachable = ApiError::Transport(TransportError::Unreachable("reset".to_owned()));
    let gate = ApiError::Transport(TransportError::RateLimited { retry_after_ms: Some(2_000) });
    let server = ApiError::Transport(TransportError::Status { status: 502, body: String::new() });
    let unreadable = ApiError::Unreadable { path: "/api/v1/sendTx".to_owned(), detail: String::new() };
    for (error, landed, rate, nonce, key) in [
        (unreachable, true, false, false, false),
        (server, true, false, false, false),
        (unreadable, true, false, false, false),
        (refused(codes::INTERNAL), true, false, false, false),
        (gate, false, true, false, false),
        (refused(codes::TOO_MANY_REQUESTS), false, true, false, false),
        (refused(codes::INVALID_NONCE), false, false, true, false),
        (refused(codes::NON_INCREASING_NONCE), false, false, true, false),
        (refused(codes::API_KEY_NOT_FOUND), false, false, false, true),
        (refused(codes::INVALID_SIGNATURE), false, false, false, true),
        (refused(codes::NOT_ENOUGH_ORDER_MARGIN), false, false, false, false),
    ] {
        assert_eq!(
            (error.may_have_landed(), error.is_rate_limited(), error.is_nonce(), error.is_unregistered_key()),
            (landed, rate, nonce, key),
            "{error}"
        );
    }
}
