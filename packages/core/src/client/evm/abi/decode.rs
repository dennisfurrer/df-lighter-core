//! `abi.decode`: data read back against declared types, each word checked against its width and
//! each length and offset bounded.

use crate::client::evm::abi::{AbiError, Kind, LONGEST, Token, WORD};
use crate::client::evm::u256::{B256, U256};

/// `abi.decode(data, (types...))`, each word checked against its declared width.
pub fn decode(kinds: &[Kind], data: &[u8]) -> Result<Vec<Token>, AbiError> {
    let kinds: Vec<&Kind> = kinds.iter().collect();
    unsequence(&kinds, data, 0)
}

fn unsequence(kinds: &[&Kind], data: &[u8], start: usize) -> Result<Vec<Token>, AbiError> {
    let mut at = start;
    let mut tokens = Vec::with_capacity(kinds.len().min(LONGEST / WORD));
    for kind in kinds {
        match kind.is_dynamic() {
            true => {
                let offset = length(data, at)?;
                tokens.push(unone(kind, data, start + offset)?);
            }
            false => tokens.push(unone(kind, data, at)?),
        }
        at += kind.head();
    }
    Ok(tokens)
}

fn word(data: &[u8], at: usize) -> Result<[u8; WORD], AbiError> {
    data.get(at..at.saturating_add(WORD)).and_then(|slice| slice.try_into().ok()).ok_or(AbiError::Short(data.len()))
}

/// A length or an offset: small, or refused.
fn length(data: &[u8], at: usize) -> Result<usize, AbiError> {
    let value = U256::from_be_bytes(word(data, at)?);
    value
        .to_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value <= LONGEST)
        .ok_or_else(|| AbiError::Misfit { at, kind: "a length".to_owned() })
}

fn unone(kind: &Kind, data: &[u8], at: usize) -> Result<Token, AbiError> {
    let misfit = || AbiError::Misfit { at, kind: kind.canonical() };
    Ok(match kind {
        Kind::Uint(bits) => {
            let value = U256::from_be_bytes(word(data, at)?);
            if value.bits() > u32::from(*bits) {
                return Err(misfit());
            }
            Token::Uint(value)
        }
        Kind::Address => B256(word(data, at)?).to_address().map(Token::Address).ok_or_else(misfit)?,
        Kind::Bool => match U256::from_be_bytes(word(data, at)?).to_u64() {
            Some(0) => Token::Bool(false),
            Some(1) => Token::Bool(true),
            _ => return Err(misfit()),
        },
        Kind::String | Kind::Bytes => {
            let length = length(data, at)?;
            let start = at + WORD;
            let bytes = data.get(start..start + length).ok_or(AbiError::Short(data.len()))?.to_vec();
            match kind {
                Kind::String => Token::String(String::from_utf8(bytes).map_err(|_| misfit())?),
                _ => Token::Bytes(bytes),
            }
        }
        Kind::Array(inner) => {
            let count = length(data, at)?;
            if count.saturating_mul(inner.head()) > data.len() {
                return Err(AbiError::Short(data.len()));
            }
            Token::Array(unsequence(&vec![inner.as_ref(); count], data, at + WORD)?)
        }
    })
}
