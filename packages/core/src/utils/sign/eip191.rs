//! EIP-191 `personal_sign`, the only Ethereum signature Lighter asks of an account's owner: the
//! text a key registration, a transfer to another owner or an integrator approval carries. A
//! wallet signs `keccak256("\x19Ethereum Signed Message:\n" + len + text)`; no chain is in it, so a
//! wallet signs on whatever chain it is on.

use crate::utils::sign::keccak::Keccak256;

/// The digest a wallet signs for `message`.
#[must_use]
pub fn personal_message_hash(message: &[u8]) -> [u8; 32] {
    Keccak256::new()
        .chain(b"\x19Ethereum Signed Message:\n")
        .chain(message.len().to_string().as_bytes())
        .chain(message)
        .finalize()
}

#[cfg(test)]
#[path = "tests/eip191.rs"]
mod tests;
