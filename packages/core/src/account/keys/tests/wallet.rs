//! The owner's signatures as the venue reads them: `personal_sign`, low-s with an Ethereum `v`,
//! read from a wallet in either form, recovered to their signer, and refused when malformed.

use super::*;
use crate::test_support::{block_on, vector_wallet_key};
use crate::utils::sign::keccak256;

const ONE: [u8; 32] = {
    let mut bytes = [0u8; 32];
    bytes[31] = 1;
    bytes
};
const ONE_ADDRESS: &str = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";

/// The secp256k1 group order, for making the high-s twin of a signature.
const ORDER: [u8; 32] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xba, 0xae, 0xdc,
    0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
];

fn wallet(bytes: &[u8; 32]) -> LocalWallet {
    LocalWallet::from_bytes(bytes).unwrap()
}

/// `ORDER - s`, big-endian.
fn negate(s: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut borrow = 0i16;
    for at in (0..32).rev() {
        let mut digit = i16::from(ORDER[at]) - i16::from(s[at]) - borrow;
        borrow = 0;
        if digit < 0 {
            digit += 256;
            borrow = 1;
        }
        out[at] = u8::try_from(digit).unwrap();
    }
    out
}

fn wallet_hex(r: &[u8; 32], s: &[u8; 32], v: u8) -> String {
    let mut bytes = [0u8; 65];
    bytes[..32].copy_from_slice(r);
    bytes[32..64].copy_from_slice(s);
    bytes[64] = v;
    hex::encode_prefixed(&bytes)
}

/// Published keys: secp256k1's generator, its double, and Hardhat's first account.
#[test]
fn the_published_keys_derive_the_published_addresses() {
    let mut two = [0u8; 32];
    two[31] = 2;
    let hardhat = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    for (wallet, address) in [
        (wallet(&ONE), ONE_ADDRESS),
        (wallet(&two), "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf"),
        (hardhat, "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"),
    ] {
        assert_eq!(wallet.address().to_hex(), address);
    }
}

/// A message signs and recovers through the trait as through the wallet; go-ethereum's own
/// signatures are matched byte for byte in `tests/register.rs`.
#[test]
fn a_message_signs_the_same_through_the_trait() {
    let hardhat = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    let signature = hardhat.sign_text("hello world").unwrap();
    assert_eq!(recover("hello world", &signature), Ok(hardhat.address()));
    assert_eq!(signature.to_hex().len(), 132);
    assert_eq!(block_on(Wallet::sign_message(&hardhat, "hello world")), Ok(signature));
}

#[test]
fn a_signature_recovers_to_its_signer_and_verify_names_any_other() {
    let wallet = wallet(&ONE);
    let signature = wallet.sign_text("Register Lighter Account").unwrap();
    assert_eq!(recover("Register Lighter Account", &signature), Ok(wallet.address()));
    assert_eq!(verify("Register Lighter Account", &signature, wallet.address()), Ok(()));
    assert_ne!(recover("Transfer", &signature).ok(), Some(wallet.address()));
    let expected: Address = "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf".parse().unwrap();
    assert_eq!(
        verify("Register Lighter Account", &signature, expected),
        Err(WalletError::WrongSigner { expected, recovered: ONE_ADDRESS.parse().unwrap() })
    );
}

#[test]
fn a_signature_is_low_s_and_carries_an_ethereum_v() {
    let wallet = wallet(&ONE);
    for index in 0u8..16 {
        let digest = keccak256(&[index; 7]);
        let signature = wallet.sign_digest(&digest).unwrap();
        assert!(signature.v() == 27 || signature.v() == 28, "v was {}", signature.v());
        let curve = CurveSignature::from_slice(&signature.to_compact()).unwrap();
        assert!(curve.normalize_s().is_none(), "signature {index} was not low-s normalised");
        assert_eq!(recover_digest(&digest, &signature).unwrap(), wallet.address());
        assert_eq!(L1Signature::from_hex(&signature.to_hex()), Ok(signature));
    }
}

/// `personal_sign` answers `r || s || v`, `v` as 0/1 or 27/28; a wallet that does not normalise
/// may send the high-s twin, folded here to the low one.
#[test]
fn a_wallet_s_hex_reads_with_either_v_and_a_high_s_folded_to_its_low_twin() {
    let wallet = wallet(&ONE);
    for index in 0u8..8 {
        let digest = keccak256(&[index; 3]);
        let signed = wallet.sign_digest(&digest).unwrap();
        let parity = signed.v() - 27;
        let flipped = 27 + (parity ^ 1);
        for (s, v) in [(*signed.s(), parity), (*signed.s(), parity + 27), (negate(signed.s()), flipped)] {
            let read = L1Signature::from_hex(&wallet_hex(signed.r(), &s, v)).unwrap();
            assert_eq!(read, signed, "v given as {v}");
            assert_eq!(recover_digest(&digest, &read), Ok(wallet.address()));
        }
    }
}

#[test]
fn a_malformed_signature_is_refused_rather_than_guessed_at() {
    let digest = keccak256(b"body");
    let signature = wallet(&ONE).sign_digest(&digest).unwrap();
    for (recovered, refused) in [
        (L1Signature::from_parts(*signature.r(), *signature.s(), 1), WalletError::BadV(1)),
        (L1Signature::from_parts([0u8; 32], [0u8; 32], 27), WalletError::Malformed),
    ] {
        assert_eq!(recover_digest(&digest, &recovered), Err(refused));
    }
    for (hex, refused) in [
        ("0x1234".to_owned(), WalletError::NotASignature),
        (wallet_hex(signature.r(), signature.s(), 29), WalletError::BadV(29)),
        (wallet_hex(&[0; 32], &[0; 32], 27), WalletError::Malformed),
    ] {
        assert_eq!(L1Signature::from_hex(&hex), Err(refused), "{hex}");
    }
    assert_eq!(LocalWallet::from_hex("0xnot-a-key").unwrap_err(), WalletError::NotAKey);
    assert_eq!(LocalWallet::from_bytes(&[0u8; 32]).unwrap_err(), WalletError::NotAKey);
}

#[test]
fn a_local_wallet_never_shows_its_key() {
    let key = vector_wallet_key();
    let wallet = LocalWallet::from_hex(&key).unwrap();
    let shown = format!("{wallet:?}");
    assert!(shown.contains("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"), "{shown}");
    assert!(!shown.contains(key.trim_start_matches("0x")), "{shown}");
}
