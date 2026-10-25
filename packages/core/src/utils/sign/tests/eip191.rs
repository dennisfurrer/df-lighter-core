//! `personal_sign`'s digest, held to viem's `hashMessage`.

use super::*;
use crate::utils::hex;

#[test]
fn the_digest_is_what_viem_hashes() {
    // viem 2.48.2: hashMessage("hello world").
    assert_eq!(
        hex::encode(&personal_message_hash(b"hello world")),
        "d9eba16ed0ecae432b71fe008c98cc872bb4cc214d3220a36f365326cf807d68"
    );
    // The length is counted in bytes, written in decimal.
    assert_eq!(
        hex::encode(&personal_message_hash("é".as_bytes())),
        hex::encode(&crate::utils::sign::keccak256(b"\x19Ethereum Signed Message:\n2\xc3\xa9"))
    );
}
