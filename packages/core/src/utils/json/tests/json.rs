//! A value's accessors, none of which reads a value as another kind.

use super::*;

#[test]
fn each_accessor_reads_only_its_own_kind() {
    let json = Json::parse(r#"{"isSanctioned":false,"merge":true,"n":1,"s":"true","z":null,"a":[1],"o":{}}"#).unwrap();
    assert_eq!(json.get("isSanctioned").and_then(Json::as_bool), Some(false));
    assert_eq!(json.get("merge").and_then(Json::as_bool), Some(true));
    for key in ["n", "s", "z"] {
        assert_eq!(json.get(key).and_then(Json::as_bool), None, "{key}");
    }
    assert_eq!(json.get("n").and_then(Json::as_u64), Some(1));
    assert_eq!(json.get("s").and_then(Json::as_u64), None);
    assert_eq!(json.get("s").and_then(Json::as_str), Some("true"));
    assert_eq!(json.get("a").and_then(Json::as_array).map(<[Json]>::len), Some(1));
    assert_eq!(json.get("o").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
    assert_eq!((json.get("a").and_then(Json::as_object), json.get("o").and_then(Json::as_array)), (None, None));
    assert_eq!(json.get("missing"), None);
    assert_eq!(json.get("a").and_then(|array| array.get("n")), None, "only an object has keys");
    let kinds: Vec<&str> = json.as_object().unwrap().iter().map(|(_, value)| value.kind()).collect();
    assert_eq!(kinds, ["a boolean", "a boolean", "a number", "a string", "null", "an array", "an object"]);
}
