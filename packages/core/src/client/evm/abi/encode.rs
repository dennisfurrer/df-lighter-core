//! `abi.encode`: each value checked against its type, then heads, then the tails they point into.

use crate::client::evm::abi::{AbiError, Kind, Token, WORD};
use crate::client::evm::u256::{B256, U256};

/// `abi.encode(values...)`: heads, then the tails they point into. Each value is checked against
/// its type first, an integer against its width.
pub fn encode(kinds: &[Kind], tokens: &[Token]) -> Result<Vec<u8>, AbiError> {
    if kinds.len() != tokens.len() {
        return Err(AbiError::Count { wants: kinds.len(), got: tokens.len() });
    }
    for (kind, token) in kinds.iter().zip(tokens) {
        if !fits(kind, token) {
            return Err(AbiError::Mismatch { kind: kind.canonical(), token: token.describe() });
        }
    }
    let kinds: Vec<&Kind> = kinds.iter().collect();
    Ok(sequence(&kinds, tokens))
}

/// Whether `token` is a value of `kind`, its integers within their width.
fn fits(kind: &Kind, token: &Token) -> bool {
    match (kind, token) {
        (Kind::Uint(bits), Token::Uint(value)) => value.bits() <= u32::from(*bits),
        (Kind::Address, Token::Address(_))
        | (Kind::Bool, Token::Bool(_))
        | (Kind::String, Token::String(_))
        | (Kind::Bytes, Token::Bytes(_)) => true,
        (Kind::Array(inner), Token::Array(items)) => items.iter().all(|item| fits(inner, item)),
        _ => false,
    }
}

fn sequence(kinds: &[&Kind], tokens: &[Token]) -> Vec<u8> {
    let heads: usize = kinds.iter().map(|kind| kind.head()).sum();
    let mut head = Vec::with_capacity(heads);
    let mut tail = Vec::new();
    for (kind, token) in kinds.iter().zip(tokens) {
        let encoded = one(kind, token);
        match kind.is_dynamic() {
            true => {
                head.extend_from_slice(&word_of(heads + tail.len()));
                tail.extend(encoded);
            }
            false => head.extend(encoded),
        }
    }
    head.extend(tail);
    head
}

/// One value checked by [`fits`]: its kind and token agree.
fn one(kind: &Kind, token: &Token) -> Vec<u8> {
    match (kind, token) {
        (_, Token::Uint(value)) => value.to_be_bytes().to_vec(),
        (_, Token::Address(address)) => B256::from_address(*address).0.to_vec(),
        (_, Token::Bool(value)) => word_of(usize::from(*value)).to_vec(),
        (_, Token::String(text)) => packed(text.as_bytes()),
        (_, Token::Bytes(bytes)) => packed(bytes),
        (Kind::Array(inner), Token::Array(items)) => {
            let mut out = word_of(items.len()).to_vec();
            out.extend(sequence(&vec![inner.as_ref(); items.len()], items));
            out
        }
        (_, Token::Array(items)) => sequence(&vec![kind; items.len()], items),
    }
}

fn packed(bytes: &[u8]) -> Vec<u8> {
    let mut out = word_of(bytes.len()).to_vec();
    out.extend(padded(bytes));
    out
}

fn padded(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(bytes.len().div_ceil(WORD) * WORD, 0);
    out
}

fn word_of(value: usize) -> [u8; WORD] {
    U256::from_u64(value as u64).to_be_bytes()
}
