//! Signing primitives: the Goldilocks field and its quintic extension, Poseidon2, the ECgFp5 curve
//! and Schnorr signatures an API key makes, the transaction envelope and hash every action shares,
//! and keccak and EIP-191 for what the owner's Ethereum wallet signs.
//!
//! Ported from elliottech/poseidon_crypto v0.0.15 (what lighter-go links) and Thomas Pornin's
//! ecgfp5, and held to lighter-go byte for byte (`vectors/lighter_go.json`).

pub(crate) mod curve;
pub mod eip191;
pub mod fp5;
pub mod goldilocks;
pub mod keccak;
pub mod poseidon2;
pub mod scalar;
pub mod schnorr;
pub mod tx;

pub use eip191::personal_message_hash;
pub use fp5::Fp5;
pub use goldilocks::Fp;
pub use keccak::keccak256;
pub use poseidon2::hash_to_fp5;
pub use schnorr::{BadKey, NoRandomness, PublicKey, SecretKey, Signature};
#[cfg(feature = "sign")]
pub use tx::sign;
pub use tx::{
    Attributes, BadTx, Envelope, FEE_TICK, SelfTrade, SelfTradeScope, SignTxError, Signed, Tx, TxHash, TxType,
    sign_drawing, tx_hash,
};
