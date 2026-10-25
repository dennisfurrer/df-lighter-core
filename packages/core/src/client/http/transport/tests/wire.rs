//! Requests and responses as the wire carries them: the method, the body, headers found whatever
//! their case, and `Retry-After` in whole seconds.

use super::*;

#[test]
fn requests_and_answers_carry_their_headers_whatever_their_case() {
    let fields = [("tx_type", "14".to_owned())];
    let post = Request::post_form("https://example.test/api/v1/sendTx", &fields).with_header("X-Key", "k");
    assert_eq!((post.method, post.url.as_str()), (Method::Post, "https://example.test/api/v1/sendTx"));
    assert_eq!(
        (post.header("content-type"), post.header("x-key")),
        (Some("application/x-www-form-urlencoded"), Some("k"))
    );
    assert_eq!(post.body, b"tx_type=14".to_vec());
    let json = Request::post_json("https://rpc.example.test", r#"{"id":1}"#.to_owned());
    assert_eq!(
        (json.header("Content-Type"), json.body.as_slice()),
        (Some("application/json"), br#"{"id":1}"#.as_slice())
    );
    let get = Request::get("https://example.test/api/v1/trades").with_header("Authorization", "t");
    assert_eq!((get.method, get.body.len()), (Method::Get, 0));
    assert_eq!(get.headers, [("Authorization".to_owned(), "t".to_owned())]);

    for (status, retry_after, success, wait) in [
        (429, Some("3"), false, Some(3_000)),
        (503, Some("Wed, 21 Oct 2026 07:28:00 GMT"), false, None),
        (204, None, true, None),
    ] {
        let headers = retry_after.map(|value| vec![("retry-after".to_owned(), value.to_owned())]).unwrap_or_default();
        let answer = Response { status, headers, body: b"null".to_vec() };
        assert_eq!(answer.header("Retry-After"), retry_after);
        assert_eq!((answer.is_success(), answer.retry_after_ms()), (success, wait), "{status}");
        assert_eq!(answer.text(), "null");
    }
}

/// `URLSearchParams` encoding: what a form post's body and a query string carry.
#[test]
fn fields_are_written_as_url_search_params_writes_them() {
    let fields = [
        ("tx_info", r#"{"Sig":"a+b/c=","L1Sig":""}"#.to_owned()),
        ("plain", "Az09*-._".to_owned()),
        ("space", "a b".to_owned()),
        ("unicode", "\u{e9}".to_owned()),
    ];
    assert_eq!(
        form(&fields),
        "tx_info=%7B%22Sig%22%3A%22a%2Bb%2Fc%3D%22%2C%22L1Sig%22%3A%22%22%7D&plain=Az09*-._&space=a+b&unicode=%C3%A9"
    );
    assert_eq!(form(&[]), "");
}

/// What carries a signature is a post that sends a transaction, or a raw EVM transaction, and
/// only that is refused, and only by an `offline` build.
#[test]
fn what_is_signed_is_a_post_that_sends_a_transaction_or_a_raw_transaction() {
    let raw = r#"{"jsonrpc":"2.0","id":1,"method":"eth_sendRawTransaction","params":["0x02"]}"#;
    let call = r#"{"jsonrpc":"2.0","id":1,"method":"eth_call","params":[]}"#;
    let api = "https://mainnet.zklighter.elliot.ai/api/v1";
    let form_post = |path: &str| Request::post_form(format!("{api}/{path}"), &[("tx_type", "14".to_owned())]);
    for (request, signed) in [
        (form_post("sendTx"), true),
        (form_post("sendTxBatch"), true),
        (form_post("fastwithdraw"), true),
        (form_post("litLease"), true),
        (form_post("sendTx/"), true),
        (Request::post_json("https://ethereum-rpc.publicnode.com", raw.to_owned()), true),
        (form_post("tokens/create"), false),
        (Request::post_json("https://ethereum-rpc.publicnode.com", call.to_owned()), false),
        (Request::get(format!("{api}/sendTx")), false),
        (Request::get(format!("{api}/fastwithdraw/info")), false),
    ] {
        assert_eq!(request.is_signed(), signed, "{}", request.url);
        let refused = matches!(request.refuse_if_offline(), Err(TransportError::Offline(_)));
        assert_eq!(refused, OFFLINE && signed, "{}", request.url);
    }
}
