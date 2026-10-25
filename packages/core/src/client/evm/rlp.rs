//! Recursive length prefix, the encoding an EIP-1559 transaction is hashed and sent in. Only
//! encoding: nothing here reads one back.

use crate::client::evm::u256::U256;

/// A string of bytes: a single byte under `0x80` is itself, anything else is
/// prefixed with its length.
#[must_use]
pub fn bytes(payload: &[u8]) -> Vec<u8> {
    if let [only] = payload
        && *only < 0x80
    {
        return vec![*only];
    }
    let mut out = prefix(payload.len(), 0x80);
    out.extend_from_slice(payload);
    out
}

/// An integer, big-endian with no leading zeros, so zero is the empty string.
#[must_use]
pub fn uint(value: u128) -> Vec<u8> {
    bytes(trimmed(&value.to_be_bytes()))
}

/// A 256-bit integer, as [`uint`] writes a narrower one.
#[must_use]
pub fn u256(value: U256) -> Vec<u8> {
    bytes(&value.to_be_trimmed())
}

/// A 32-byte word read as an integer, as a signature's `r` and `s` are.
#[must_use]
pub fn word(value: &[u8; 32]) -> Vec<u8> {
    bytes(trimmed(value))
}

/// A list of items that are each already encoded.
#[must_use]
pub fn list(items: &[Vec<u8>]) -> Vec<u8> {
    let length = items.iter().map(Vec::len).sum();
    let mut out = prefix(length, 0xc0);
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn trimmed(big_endian: &[u8]) -> &[u8] {
    let first = big_endian.iter().position(|byte| *byte != 0).unwrap_or(big_endian.len());
    &big_endian[first..]
}

/// Short payloads carry their length in the tag; from 56 bytes on, the tag
/// says how many bytes the length itself takes.
fn prefix(length: usize, base: u8) -> Vec<u8> {
    if let Ok(short) = u8::try_from(length)
        && short < 56
    {
        return vec![base + short];
    }
    let written = trimmed(&(length as u64).to_be_bytes()).to_vec();
    // At most eight bytes of length, so the tag stays under 0xc0 and 0x100.
    let mut out = vec![base + 55 + written.len() as u8];
    out.extend_from_slice(&written);
    out
}

#[cfg(test)]
#[path = "tests/rlp.rs"]
mod tests;
