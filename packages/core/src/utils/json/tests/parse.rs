//! Text read as written, and a mistake placed where it is.

use super::*;

#[test]
fn keys_and_numbers_stay_as_they_were_written() {
    let json = Json::parse(r#"{ "type": "cancel", "b": 1, "a": 2 }"#).unwrap();
    assert_eq!(json.compact(), r#"{"type":"cancel","b":1,"a":2}"#);
    let keys: Vec<&str> = json.as_object().unwrap().iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["type", "b", "a"]);
    let numbers = Json::parse("[123456789012345678901234567890, -1.50e3, 0, 2E+2]").unwrap();
    assert_eq!(numbers.compact(), "[123456789012345678901234567890,-1.50e3,0,2E+2]");
    assert_eq!(Json::parse(r#""\ud83d\ude00\/""#), Ok(Json::String("\u{1f600}/".to_owned())), "a surrogate pair");
}

#[test]
fn a_mistake_is_placed_by_line_and_column_and_named() {
    let wrong = Json::parse("{\n  \"type\": outcomeMeta\n}").unwrap_err();
    assert_eq!((wrong.line, wrong.column), (2, 11));
    let deep = format!("{}{}", "[".repeat(130), "]".repeat(130));
    for (text, said) in [
        ("{\"a\":1,\"a\":2}", "twice"),
        ("[1,]", "not the start of a value"),
        ("[1 2]", "expected , or ]"),
        ("{1:2}", "expected a key in quotes"),
        ("{\"a\" 1}", "expected : after a key"),
        ("{\"a\":1 \"b\":2}", "expected , or }"),
        ("01", "does not start with 0"),
        ("-", "a number needs digits"),
        ("1.", "digits must follow the point"),
        ("1e", "digits must follow the exponent"),
        ("{} x", "more after the value ends"),
        ("", "the text ends where a value should be"),
        ("tru", "expected true"),
        ("\"open", "never closes"),
        ("\"a\\", "never closes"),
        ("\"a\nb\"", "a raw control character"),
        ("\"\\x\"", "not an escape JSON knows"),
        ("\"\\u12\"", "four hex digits"),
        ("\"\\ud800\"", "half of a surrogate pair"),
        ("\"\\ud800\\u0041\"", "half of a surrogate pair"),
        ("\"\\udc00\"", "half of a surrogate pair"),
        (deep.as_str(), "nested too deep"),
    ] {
        let error = Json::parse(text).unwrap_err();
        assert!(error.message.contains(said), "{text:?}: {error}");
    }
}
