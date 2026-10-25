//! JSON that keeps what a signed transaction depends on: keys in the order they were written and
//! numbers as the digits they were written with. A transaction's `tx_info` is the struct as Go's
//! `encoding/json` writes it, keys in field order, where `serde_json` sorts keys.

mod parse;
mod write;

pub use parse::BadJson;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    /// The number exactly as written: no float ever holds it.
    Number(String),
    String(String),
    Array(Vec<Json>),
    /// Keys in the order they were written.
    Object(Vec<(String, Json)>),
}

impl Json {
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    /// `true` or `false`; `None` for anything else, a `"true"` string too.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// A whole non-negative number, as the venue writes counts and ids.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(digits) => digits.parse().ok(),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Object(entries) => Some(entries),
            _ => None,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "a boolean",
            Json::Number(_) => "a number",
            Json::String(_) => "a string",
            Json::Array(_) => "an array",
            Json::Object(_) => "an object",
        }
    }
}

#[cfg(test)]
#[path = "tests/json.rs"]
mod tests;
