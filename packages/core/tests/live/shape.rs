//! The shape of a JSON answer: its key paths and the kind of value at each, whatever the values,
//! so a live answer can be held against the recording of the same read.
//!
//! Arrays read as `[]`. An object's keys read as `*` when they are data rather than field names
//! (ids, addresses, symbols: anything not snake_case or lowerCamelCase, or more than
//! [`MAX_FIELDS`] of them); a candle's one-letter keys (`t`, `V`) are fields. A path under an
//! empty array, an empty object or a `null` in the recording is not counted: the recording had
//! nothing there to compare with. A `null` in the live answer is no kind: it is the venue leaving
//! a value out.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// More keys than this and an object is a map, not a record: a market's details have 56.
const MAX_FIELDS: usize = 100;

#[derive(Debug, Default)]
struct Shape {
    /// Each key path, with the kinds of value found there.
    paths: BTreeMap<String, BTreeSet<&'static str>>,
    /// Where the recording held nothing to compare: empty arrays and objects, and `null`s.
    open: BTreeSet<String>,
}

/// How `live` differs from the recordings of the same read: each key path none of them has (what
/// the venue has added since), and each path holding a kind of value none of them had there.
/// Several recordings count as one, for a field the venue writes on some rows only.
pub fn drift(recorded: &[Value], live: &Value) -> Vec<String> {
    let (mut was, mut now) = (Shape::default(), Shape::default());
    for one in recorded {
        walk(one, "", &mut was);
    }
    walk(live, "", &mut now);
    let mut drift = Vec::new();
    for (path, kinds) in now.paths {
        if was.open.iter().any(|open| under(&path, open)) {
            continue;
        }
        match was.paths.get(&path) {
            None => drift.push(path),
            Some(was) if !kinds.is_subset(was) => {
                let now: Vec<&str> = kinds.into_iter().collect();
                let was: Vec<&str> = was.iter().copied().collect();
                drift.push(format!("{path}: {} where it was {}", now.join(" or "), was.join(" or ")));
            }
            Some(_) => {}
        }
    }
    drift
}

/// Whether `path` is `open` or inside it.
fn under(path: &str, open: &str) -> bool {
    open.is_empty()
        || path.strip_prefix(open).is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
}

fn walk(value: &Value, path: &str, shape: &mut Shape) {
    let kind = match value {
        Value::Null => {
            shape.open.insert(path.to_owned());
            shape.paths.entry(path.to_owned()).or_default();
            return;
        }
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    shape.paths.entry(path.to_owned()).or_default().insert(kind);
    match value {
        Value::Object(fields) => {
            if fields.is_empty() {
                shape.open.insert(path.to_owned());
            }
            let map = fields.len() > MAX_FIELDS || fields.keys().any(|key| !is_field(key));
            for (key, inner) in fields {
                let name = if map { "*" } else { key.as_str() };
                let inner_path = if path.is_empty() { name.to_owned() } else { format!("{path}.{name}") };
                walk(inner, &inner_path, shape);
            }
        }
        Value::Array(items) => {
            let inner_path = format!("{path}[]");
            if items.is_empty() {
                shape.open.insert(inner_path.clone());
            }
            for item in items {
                walk(item, &inner_path, shape);
            }
        }
        _ => {}
    }
}

/// A field name as the venue writes one: snake_case (`market_id`), lowerCamelCase (`chainId`), or
/// one letter (a candle's `V`).
fn is_field(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
        Some(first) => first.is_ascii_uppercase() && key.len() == 1,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::drift;

    #[test]
    fn a_new_field_and_a_value_of_a_new_kind_drift_and_new_values_do_not() {
        let recorded = json!({
            "order_books": [{"symbol": "BTC", "market_id": 1, "open_interest": "0"}, {"open_interest": 2.5}],
            "c": [{"t": 1, "V": 2.5}],
            "prices": {"0x47": "1"},
            "pool": null,
        });
        let live = json!({
            "order_books": [{"symbol": "ETH", "market_id": 0, "open_interest": 1, "is_frozen": false}],
            "c": [{"t": 2, "V": "2.5"}],
            "prices": {"0x48": "2", "0x49": "3"},
            "pool": {"name": "LLP"},
        });
        assert_eq!(drift(&[recorded], &live), ["c[].V: string where it was number", "order_books[].is_frozen"]);
        assert!(drift(&[live], &json!({"order_books": [], "c": null})).is_empty(), "nothing there is no drift");
    }

    #[test]
    fn a_field_any_recording_has_is_no_drift() {
        let some = json!({"trades": [{"trade_id": 1}]});
        let other = json!({"trades": [{"trade_id": 2, "taker_fee": 280}]});
        let live = json!({"trades": [{"trade_id": 3, "taker_fee": 245}, {"trade_id": 4}]});
        assert_eq!(drift(std::slice::from_ref(&some), &live), ["trades[].taker_fee"]);
        assert!(drift(&[some, other], &live).is_empty());
    }
}
