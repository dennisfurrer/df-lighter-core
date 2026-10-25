//! The type grammar: a [`Kind`] read from its text and written back canonically.

use std::fmt;

use crate::client::evm::abi::{AbiError, WORD};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// `uint8` to `uint256`, in steps of eight bits; `uint` is `uint256`.
    Uint(u16),
    Address,
    Bool,
    String,
    Bytes,
    /// `T[]`.
    Array(Box<Kind>),
}

/// How deep arrays may nest in a written type.
const DEEPEST: usize = 8;

impl Kind {
    /// One type, as written: `uint48`, `address[]`, `bytes`.
    pub fn parse(text: &str) -> Result<Kind, AbiError> {
        kind_of(text.trim(), 0).ok_or_else(|| AbiError::Type(text.trim().to_owned()))
    }

    /// Whether it is encoded behind an offset, in the tail.
    #[must_use]
    pub const fn is_dynamic(&self) -> bool {
        matches!(self, Kind::String | Kind::Bytes | Kind::Array(_))
    }

    /// Bytes this takes in the head of whatever holds it: one word, every kind here.
    pub(super) const fn head(&self) -> usize {
        WORD
    }

    /// The canonical type, as a signature or a selector spells it.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Kind::Uint(bits) => format!("uint{bits}"),
            Kind::Address => "address".to_owned(),
            Kind::Bool => "bool".to_owned(),
            Kind::String => "string".to_owned(),
            Kind::Bytes => "bytes".to_owned(),
            Kind::Array(inner) => format!("{}[]", inner.canonical()),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// A list of types, `uint48,uint16`, each perhaps followed by a parameter name, which is dropped:
/// `address owner, uint16 assetIndex` reads the same. Empty text is no types.
pub fn kinds(text: &str) -> Result<Vec<Kind>, AbiError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|one| {
            let one = one.trim();
            let kind = one.split_whitespace().next().unwrap_or_default();
            Kind::parse(kind)
        })
        .collect()
}

fn kind_of(text: &str, depth: usize) -> Option<Kind> {
    if depth > DEEPEST {
        return None;
    }
    if let Some(inner) = text.strip_suffix("[]") {
        return kind_of(inner, depth + 1).map(|inner| Kind::Array(Box::new(inner)));
    }
    Some(match text {
        "address" => Kind::Address,
        "bool" => Kind::Bool,
        "string" => Kind::String,
        "bytes" => Kind::Bytes,
        "uint" => Kind::Uint(256),
        _ => {
            let bits: u16 = text.strip_prefix("uint")?.parse().ok()?;
            (bits > 0 && bits <= 256 && bits.is_multiple_of(8)).then_some(Kind::Uint(bits))?
        }
    })
}
