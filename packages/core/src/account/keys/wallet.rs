//! The owner's Ethereum wallet: the address that owns the accounts, and the `personal_sign`
//! (EIP-191) it gives a key registration, a transfer to another owner and an integrator approval
//! with fees. A browser wallet implements [`Wallet`] through its `personal_sign`; a native tool
//! holds a [`LocalWallet`]. Every signature is recovered and must be the wallet's before it is
//! sent, since a wallet can sign as another account than expected. Without the `sign` feature a
//! signature can still be carried, but nothing can make or check one.

#[cfg(feature = "sign")]
use std::fmt;
use std::future::Future;

#[cfg(feature = "sign")]
use k256::ecdsa::{RecoveryId, Signature as CurveSignature, SigningKey, VerifyingKey};
#[cfg(feature = "sign")]
use zeroize::{ZeroizeOnDrop, Zeroizing};

use crate::utils::address::Address;
use crate::utils::hex;
#[cfg(feature = "sign")]
use crate::utils::sign::{keccak256, personal_message_hash};

#[cfg(feature = "sign")]
const V_OFFSET: u8 = 27;

/// `r || s || v`, 65 bytes, low-s, `v = 27 + recovery id`: what `L1Sig` carries, as `0x` and 130 hex
/// digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L1Signature {
    r: [u8; 32],
    s: [u8; 32],
    v: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WalletError {
    #[error("the key could not sign this digest")]
    Sign,
    #[error("a signature is 65 bytes of hex")]
    NotASignature,
    #[error("the signature components are not a valid curve point")]
    Malformed,
    #[error("no public key recovers from this signature")]
    Unrecoverable,
    #[error("`{0}` is not an Ethereum recovery parameter; expected 27 or 28")]
    BadV(u8),
    #[error("the signature is from {recovered}, not {expected}")]
    WrongSigner { expected: Address, recovered: Address },
    #[error("the key is not a valid secp256k1 private key")]
    NotAKey,
    /// The wallet would not sign: the person declined, or it failed in its own words.
    #[error("the wallet did not sign: {0}")]
    Declined(String),
}

impl L1Signature {
    #[must_use]
    pub const fn from_parts(r: [u8; 32], s: [u8; 32], v: u8) -> Self {
        L1Signature { r, s, v }
    }

    /// What a wallet's `personal_sign` answers: `0x` and 65 bytes. `v` may be 0/1 or 27/28 and comes
    /// out 27/28; a high `s` is folded to its low twin, the same signature to the curve.
    #[cfg(feature = "sign")]
    pub fn from_hex(text: &str) -> Result<L1Signature, WalletError> {
        let bytes: [u8; 65] = hex::decode_array(text).map_err(|_| WalletError::NotASignature)?;
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..64]);
        let parity = match bytes[64] {
            0 | 27 => 0,
            1 | 28 => 1,
            other => return Err(WalletError::BadV(other)),
        };
        let compact = L1Signature { r, s, v: V_OFFSET + parity }.to_compact();
        let curve = CurveSignature::from_slice(&compact).map_err(|_| WalletError::Malformed)?;
        Ok(match curve.normalize_s() {
            None => L1Signature { r, s, v: V_OFFSET + parity },
            Some(low) => {
                s.copy_from_slice(&low.to_bytes()[32..]);
                L1Signature { r, s, v: V_OFFSET + (parity ^ 1) }
            }
        })
    }

    #[must_use]
    pub const fn r(&self) -> &[u8; 32] {
        &self.r
    }

    #[must_use]
    pub const fn s(&self) -> &[u8; 32] {
        &self.s
    }

    /// 27 or 28.
    #[must_use]
    pub const fn v(&self) -> u8 {
        self.v
    }

    #[must_use]
    pub fn to_compact(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&self.r);
        bytes[32..].copy_from_slice(&self.s);
        bytes
    }

    /// `0x` and 130 hex digits, `v` last: what `L1Sig` carries.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut bytes = [0u8; 65];
        bytes[..64].copy_from_slice(&self.to_compact());
        bytes[64] = self.v;
        hex::encode_prefixed(&bytes)
    }
}

/// The address that signed `digest`.
#[cfg(feature = "sign")]
pub fn recover_digest(digest: &[u8; 32], signature: &L1Signature) -> Result<Address, WalletError> {
    let recovery_id =
        signature.v.checked_sub(V_OFFSET).and_then(RecoveryId::from_byte).ok_or(WalletError::BadV(signature.v))?;
    let curve = CurveSignature::from_slice(&signature.to_compact()).map_err(|_| WalletError::Malformed)?;
    let key =
        VerifyingKey::recover_from_prehash(digest, &curve, recovery_id).map_err(|_| WalletError::Unrecoverable)?;
    Ok(address_of_public_key(&key))
}

/// The address that `personal_sign`ed `message`.
#[cfg(feature = "sign")]
pub fn recover(message: &str, signature: &L1Signature) -> Result<Address, WalletError> {
    recover_digest(&personal_message_hash(message.as_bytes()), signature)
}

/// Recovers `signature` over `message` and requires `expected`.
#[cfg(feature = "sign")]
pub fn verify(message: &str, signature: &L1Signature, expected: Address) -> Result<(), WalletError> {
    let recovered = recover(message, signature)?;
    match recovered == expected {
        true => Ok(()),
        false => Err(WalletError::WrongSigner { expected, recovered }),
    }
}

/// `keccak256(uncompressed public key without its 0x04 tag)`, last twenty bytes.
#[cfg(feature = "sign")]
#[must_use]
pub fn address_of_public_key(key: &VerifyingKey) -> Address {
    let point = key.to_encoded_point(false);
    let hash = keccak256(&point.as_bytes()[1..]);
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(&hash[12..]);
    Address::from_bytes(bytes)
}

/// The account's owner. In the browser that is the wallet, handed the text to `personal_sign`; in
/// tests and native tools, a [`LocalWallet`]. No `Send` bound: the browser is single-threaded.
pub trait Wallet {
    fn address(&self) -> Address;

    /// `personal_sign` over `message`, the text exactly as given.
    fn sign_message(&self, message: &str) -> impl Future<Output = Result<L1Signature, WalletError>>;
}

/// If `k256` ever stops wiping its key on drop, this stops compiling.
#[cfg(feature = "sign")]
const _: fn() = || {
    fn wiped_on_drop<T: ZeroizeOnDrop>() {}
    wiped_on_drop::<SigningKey>();
};

/// A private key held in memory, for tests and native tools. The browser never holds one: its
/// wallet signs.
#[cfg(feature = "sign")]
pub struct LocalWallet {
    key: SigningKey,
    address: Address,
}

#[cfg(feature = "sign")]
impl LocalWallet {
    pub fn from_bytes(secret: &[u8; 32]) -> Result<Self, WalletError> {
        let key = SigningKey::from_bytes(secret.into()).map_err(|_| WalletError::NotAKey)?;
        let address = address_of_public_key(key.verifying_key());
        Ok(LocalWallet { key, address })
    }

    /// Thirty-two bytes of hex, `0x` optional. A rejected key is never quoted back.
    pub fn from_hex(secret: &str) -> Result<Self, WalletError> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        hex::decode_into(secret, bytes.as_mut_slice()).map_err(|_| WalletError::NotAKey)?;
        LocalWallet::from_bytes(&bytes)
    }

    #[must_use]
    pub const fn address(&self) -> Address {
        self.address
    }

    /// RFC 6979, so the same digest always signs the same; low-s.
    pub fn sign_digest(&self, digest: &[u8; 32]) -> Result<L1Signature, WalletError> {
        let (signature, recovery_id): (CurveSignature, RecoveryId) =
            self.key.sign_prehash_recoverable(digest).map_err(|_| WalletError::Sign)?;
        let bytes = signature.to_bytes();
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Ok(L1Signature { r, s, v: recovery_id.to_byte() + V_OFFSET })
    }

    /// `personal_sign` over `message`.
    pub fn sign_text(&self, message: &str) -> Result<L1Signature, WalletError> {
        self.sign_digest(&personal_message_hash(message.as_bytes()))
    }
}

#[cfg(feature = "sign")]
impl Wallet for LocalWallet {
    fn address(&self) -> Address {
        self.address
    }

    fn sign_message(&self, message: &str) -> impl Future<Output = Result<L1Signature, WalletError>> {
        std::future::ready(self.sign_text(message))
    }
}

#[cfg(feature = "sign")]
impl fmt::Debug for LocalWallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalWallet").field("address", &self.address.to_hex()).finish_non_exhaustive()
    }
}

/// Holds because `SigningKey` wipes itself; the assertion above checks that.
#[cfg(feature = "sign")]
impl ZeroizeOnDrop for LocalWallet {}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/wallet.rs"]
mod tests;
