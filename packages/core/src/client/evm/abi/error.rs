//! Why a type, a signature, a value or a word is refused.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AbiError {
    #[error("`{0}` is not a type this reads")]
    Type(String),
    #[error("`{0}` is not a signature this reads; expected name(types)")]
    Signature(String),
    #[error("expected {wants} values and there are {got}")]
    Count { wants: usize, got: usize },
    #[error("a {kind} cannot hold {token}")]
    Mismatch { kind: String, token: String },
    #[error("the data ends at byte {0}, before the shape it was declared with")]
    Short(usize),
    #[error("the word at byte {at} does not fit {kind}")]
    Misfit { at: usize, kind: String },
    #[error("the calldata is for selector {found}, not {wants}")]
    Selector { wants: String, found: String },
}
