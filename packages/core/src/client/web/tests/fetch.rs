//! Run as wasm in Node (`just web`), through Node's own `fetch`. `data:` URLs answer without any
//! network; a server on 127.0.0.1 started from the test (`echo.js`) hands back what it was sent; a
//! port nothing listens on is no answer.

use crate::client::{Http, Request, TransportError};
use js_sys::Promise;
use serde_json::Value;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

use super::*;

#[wasm_bindgen(module = "/fixtures/web/echo.js")]
extern "C" {
    fn echo() -> Promise;
}

/// What the echo server was sent by `request` to its URL, as it read it.
async fn echoed(request: impl FnOnce(&str) -> Request) -> Value {
    let url = JsFuture::from(echo()).await.unwrap().as_string().unwrap();
    let response = Web.send(request(&url)).await.unwrap();
    serde_json::from_slice(&response.body).unwrap()
}

#[wasm_bindgen_test]
async fn an_answer_comes_back_whole_its_status_headers_and_bytes() {
    for request in [
        Request::post_json(r#"data:application/json,{"ok":true}"#, "{}".to_owned()),
        Request::get(r#"data:application/json,{"ok":true}"#),
    ] {
        let response = Web.send(request).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.header("Content-Type"), Some("application/json"));
        assert_eq!(response.text(), r#"{"ok":true}"#);
    }
    let request = Request::post_json("data:application/octet-stream;base64,AAH/gA==", "{}".to_owned());
    assert_eq!(Web.send(request).await.unwrap().body, vec![0x00, 0x01, 0xff, 0x80]);
}

#[wasm_bindgen_test]
async fn each_request_goes_out_with_its_method_headers_and_body() {
    let fields = [("tx_type", "14".to_owned()), ("tx_info", r#"{"A":1}"#.to_owned())];
    let post =
        echoed(|url| Request::post_form(format!("{url}/api/v1/sendTx"), &fields).with_header("X-Key", "k1")).await;
    assert_eq!((&post["method"], &post["url"]), (&Value::from("POST"), &Value::from("/api/v1/sendTx")));
    assert_eq!(
        (&post["headers"]["content-type"], &post["headers"]["x-key"]),
        (&Value::from("application/x-www-form-urlencoded"), &Value::from("k1"))
    );
    assert_eq!(post["body"], "tx_type=14&tx_info=%7B%22A%22%3A1%7D");
    let get =
        echoed(|url| Request::get(format!("{url}/api/v1/trades?market_id=1")).with_header("Authorization", "t")).await;
    assert_eq!((&get["method"], &get["url"]), (&Value::from("GET"), &Value::from("/api/v1/trades?market_id=1")));
    assert_eq!(get["headers"]["authorization"], "t");
    assert_eq!((get["headers"].get("content-type"), &get["body"]), (None, &Value::from("")));
}

#[wasm_bindgen_test]
async fn no_answer_is_unreachable() {
    let error = Web.send(Request::get("http://127.0.0.1:9/api/v1/orderBooks")).await.unwrap_err();
    assert!(matches!(error, TransportError::Unreachable(_)), "{error:?}");
}

#[wasm_bindgen_test]
fn the_clock_reads_unix_milliseconds() {
    let now = clock();
    assert!(now > 1_790_000_000_000, "after 2026-09: {now}");
    assert!(now < 4_102_444_800_000, "before 2100: {now}");
}
