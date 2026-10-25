//! Standard base64 with padding, as Go's `encoding/json` writes a `[]byte`: a transaction's
//! signature and a registered public key are carried so in `tx_info`.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not standard base64 with padding")]
pub struct BadBase64;

#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for at in 0..4 {
            match at <= chunk.len() {
                true => text.push(char::from(ALPHABET[((word >> (18 - 6 * at)) & 0x3f) as usize])),
                false => text.push('='),
            }
        }
    }
    text
}

pub fn decode(text: &str) -> Result<Vec<u8>, BadBase64> {
    let raw = text.as_bytes();
    if !raw.len().is_multiple_of(4) {
        return Err(BadBase64);
    }
    let mut bytes = Vec::with_capacity(raw.len() / 4 * 3);
    for (index, quad) in raw.chunks(4).enumerate() {
        let last = index + 1 == raw.len() / 4;
        let padding = quad.iter().rev().take_while(|byte| **byte == b'=').count();
        if padding > 2 || (padding > 0 && !last) {
            return Err(BadBase64);
        }
        let mut word = 0u32;
        for byte in &quad[..4 - padding] {
            word = (word << 6) | u32::from(value(*byte)?);
        }
        word <<= 6 * padding;
        let kept = 3 - padding;
        bytes.extend_from_slice(&word.to_be_bytes()[1..=kept]);
    }
    Ok(bytes)
}

fn value(byte: u8) -> Result<u8, BadBase64> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(BadBase64),
    }
}

#[cfg(test)]
#[path = "tests/base64.rs"]
mod tests;
