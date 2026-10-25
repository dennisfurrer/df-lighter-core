//! Reading JSON text: keys kept in the order written, numbers kept as written, and a mistake placed
//! by line and column.

use crate::utils::json::Json;

/// Where the text stopped being JSON, counted from one as an editor counts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}, column {column}: {message}")]
pub struct BadJson {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl Json {
    pub fn parse(text: &str) -> Result<Json, BadJson> {
        let mut reader = Reader { text: text.as_bytes(), at: 0, source: text };
        reader.space();
        let value = reader.value(0)?;
        reader.space();
        if reader.at < reader.text.len() {
            return Err(reader.fail("more after the value ends"));
        }
        Ok(value)
    }
}

/// Nesting deeper than this is refused rather than followed off the stack.
const DEEPEST: usize = 128;

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
    source: &'a str,
}

impl Reader<'_> {
    fn fail(&self, message: &str) -> BadJson {
        let before = self.source.get(..self.at).unwrap_or(self.source);
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |tail| tail.chars().count()) + 1;
        BadJson { line, column, message: message.to_owned() }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, word: &str) -> Result<(), BadJson> {
        match self.text[self.at..].starts_with(word.as_bytes()) {
            true => {
                self.at += word.len();
                Ok(())
            }
            false => Err(self.fail(&format!("expected {word}"))),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, BadJson> {
        if depth > DEEPEST {
            return Err(self.fail("nested too deep"));
        }
        match self.peek() {
            None => Err(self.fail("the text ends where a value should be")),
            Some(b'n') => self.expect("null").map(|()| Json::Null),
            Some(b't') => self.expect("true").map(|()| Json::Bool(true)),
            Some(b'f') => self.expect("false").map(|()| Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.fail("not the start of a value")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, BadJson> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.space();
            items.push(self.value(depth + 1)?);
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.fail("expected , or ] in an array")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, BadJson> {
        self.at += 1;
        let mut entries: Vec<(String, Json)> = Vec::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return Err(self.fail("expected a key in quotes"));
            }
            let key = self.string()?;
            if entries.iter().any(|(seen, _)| *seen == key) {
                return Err(self.fail(&format!("the key {key:?} is written twice")));
            }
            self.space();
            if self.peek() != Some(b':') {
                return Err(self.fail("expected : after a key"));
            }
            self.at += 1;
            self.space();
            let value = self.value(depth + 1)?;
            entries.push((key, value));
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(self.fail("expected , or } in an object")),
            }
        }
    }

    fn number(&mut self) -> Result<Json, BadJson> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let digits = |reader: &mut Self| {
            let from = reader.at;
            while matches!(reader.peek(), Some(b'0'..=b'9')) {
                reader.at += 1;
            }
            reader.at - from
        };
        let whole = self.at;
        if digits(self) == 0 {
            return Err(self.fail("a number needs digits"));
        }
        if self.text[whole] == b'0' && self.at - whole > 1 {
            return Err(self.fail("a number does not start with 0"));
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if digits(self) == 0 {
                return Err(self.fail("digits must follow the point"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if digits(self) == 0 {
                return Err(self.fail("digits must follow the exponent"));
            }
        }
        Ok(Json::Number(self.source[start..self.at].to_owned()))
    }

    fn string(&mut self) -> Result<String, BadJson> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let start = self.at;
            while !matches!(self.peek(), None | Some(b'"' | b'\\')) {
                if self.text[self.at] < 0x20 {
                    return Err(self.fail("a raw control character inside a string"));
                }
                self.at += 1;
            }
            out.push_str(&self.source[start..self.at]);
            match self.peek() {
                None => return Err(self.fail("a string that never closes")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                _ => {
                    self.at += 1;
                    let escaped = self.peek().ok_or_else(|| self.fail("a string that never closes"))?;
                    self.at += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode()?),
                        _ => return Err(self.fail("not an escape JSON knows")),
                    }
                }
            }
        }
    }

    fn unicode(&mut self) -> Result<char, BadJson> {
        let first = self.four()?;
        if (0xd800..0xdc00).contains(&first) {
            if !self.text[self.at..].starts_with(b"\\u") {
                return Err(self.fail("half of a surrogate pair"));
            }
            self.at += 2;
            let second = self.four()?;
            if !(0xdc00..0xe000).contains(&second) {
                return Err(self.fail("half of a surrogate pair"));
            }
            let joined = 0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00);
            return char::from_u32(joined).ok_or_else(|| self.fail("not a character"));
        }
        char::from_u32(first).ok_or_else(|| self.fail("half of a surrogate pair"))
    }

    fn four(&mut self) -> Result<u32, BadJson> {
        let digits = self.source.get(self.at..self.at + 4).ok_or_else(|| self.fail("\\u needs four hex digits"))?;
        let value = u32::from_str_radix(digits, 16).map_err(|_| self.fail("\\u needs four hex digits"))?;
        self.at += 4;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "tests/parse.rs"]
mod tests;
