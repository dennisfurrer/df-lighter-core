#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadHex {
    #[error("hex must have an even number of digits, found {0}")]
    OddLength(usize),
    #[error("expected {expected} bytes of hex, found {found}")]
    WrongLength { expected: usize, found: usize },
    #[error("`{0}` is not a hex digit")]
    NotAHexDigit(char),
}

#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(nibble(byte >> 4));
        text.push(nibble(byte & 0x0f));
    }
    text
}

#[must_use]
pub fn encode_prefixed(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2 + 2);
    text.push_str("0x");
    text.push_str(&encode(bytes));
    text
}

/// Decodes hex into a fresh vector, `0x` prefix and whitespace optional.
pub fn decode(text: &str) -> Result<Vec<u8>, BadHex> {
    let body = body_of(text);
    if !body.len().is_multiple_of(2) {
        return Err(BadHex::OddLength(body.len()));
    }
    let mut bytes = vec![0u8; body.len() / 2];
    fill(body, &mut bytes)?;
    Ok(bytes)
}

pub fn decode_into(text: &str, out: &mut [u8]) -> Result<(), BadHex> {
    let body = body_of(text);
    if !body.len().is_multiple_of(2) {
        return Err(BadHex::OddLength(body.len()));
    }
    if body.len() / 2 != out.len() {
        return Err(BadHex::WrongLength { expected: out.len(), found: body.len() / 2 });
    }
    fill(body, out)
}

pub fn decode_array<const N: usize>(text: &str) -> Result<[u8; N], BadHex> {
    let mut bytes = [0u8; N];
    decode_into(text, &mut bytes)?;
    Ok(bytes)
}

fn body_of(text: &str) -> &str {
    let raw = text.trim();
    raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")).unwrap_or(raw)
}

fn fill(body: &str, out: &mut [u8]) -> Result<(), BadHex> {
    let digits = body.as_bytes();
    for &byte in digits {
        digit(byte)?;
    }
    for (index, slot) in out.iter_mut().enumerate() {
        let high = digit(digits[index * 2])?;
        let low = digit(digits[index * 2 + 1])?;
        *slot = (high << 4) | low;
    }
    Ok(())
}

fn digit(byte: u8) -> Result<u8, BadHex> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(BadHex::NotAHexDigit(char::from(byte))),
    }
}

const fn nibble(value: u8) -> char {
    (if value < 10 { b'0' + value } else { b'a' + value - 10 }) as char
}

#[cfg(test)]
#[path = "tests/hex.rs"]
mod tests;
