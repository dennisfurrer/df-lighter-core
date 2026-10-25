//! Writing JSON: one line as `JSON.stringify` writes it, or two spaces a level for a person.

use std::fmt::Write as _;

use crate::utils::json::Json;

impl Json {
    /// One line, no spaces: what `JSON.stringify` writes, byte for byte.
    #[must_use]
    pub fn compact(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, None, 0);
        out
    }

    /// Two spaces a level, for showing a body to a person.
    #[must_use]
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(2), 0);
        out
    }

    fn write(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        let open = |out: &mut String, depth: usize| {
            if let Some(step) = indent {
                out.push('\n');
                out.push_str(&" ".repeat(step * depth));
            }
        };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Number(digits) => out.push_str(digits),
            Json::String(text) => quote(text, out),
            Json::Array(items) if items.is_empty() => out.push_str("[]"),
            Json::Array(items) => {
                out.push('[');
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    open(out, depth + 1);
                    item.write(out, indent, depth + 1);
                }
                open(out, depth);
                out.push(']');
            }
            Json::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Json::Object(entries) => {
                out.push('{');
                for (at, (key, value)) in entries.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    open(out, depth + 1);
                    quote(key, out);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    value.write(out, indent, depth + 1);
                }
                open(out, depth);
                out.push('}');
            }
        }
    }
}

fn quote(text: &str, out: &mut String) {
    out.push('"');
    for letter in text.chars() {
        match letter {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
#[path = "tests/write.rs"]
mod tests;
