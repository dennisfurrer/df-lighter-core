//! Output as `JSON.stringify` writes it, and indented output that reads back the same.

use super::*;

#[test]
fn compact_is_json_stringify_s_and_pretty_reads_back_the_same() {
    let json = Json::parse(r#""a\"b\\c\n\r\t\b\f\u0001é😀/""#).unwrap();
    assert_eq!(json, Json::String("a\"b\\c\n\r\t\u{8}\u{c}\u{1}\u{e9}\u{1f600}/".to_owned()));
    assert_eq!(json.compact(), "\"a\\\"b\\\\c\\n\\r\\t\\b\\f\\u0001\u{e9}\u{1f600}/\"");

    let text =
        "{\n  \"type\": \"outcomeMeta\",\n  \"list\": [\n    1,\n    {},\n    true,\n    null\n  ],\n  \"none\": []\n}";
    let json = Json::parse(text).unwrap();
    assert_eq!(json.pretty(), text);
    assert_eq!(Json::parse(&json.pretty()).unwrap(), json);
    assert_eq!(json.compact(), r#"{"type":"outcomeMeta","list":[1,{},true,null],"none":[]}"#);
}
