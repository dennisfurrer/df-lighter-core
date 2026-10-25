//! Schnorr signatures over ECgFp5, as Lighter's API keys make them (poseidon_crypto
//! `signature/schnorr`): the challenge is e = Poseidon2(encode(kG) || hash), the response
//! s = k - e*sk, and a signature is 80 bytes, s then e, each 40 bytes little-endian.
//!
//! The nonce k is random: the same transaction signed twice signs differently, and both are valid.
//! It is drawn as lighter-go draws it (`crypto/rand.Int(rand.Reader, n)`), so a recorded stream
//! of draws reproduces lighter-go's signatures byte for byte.

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::utils::hex;
use crate::utils::sign::curve::Point;
use crate::utils::sign::fp5::Fp5;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::poseidon2::hash_to_fp5;
use crate::utils::sign::scalar::Scalar;

/// A key's secret: a scalar, 40 bytes little-endian. Never printed, and wiped when dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey(Scalar);

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for SecretKey {}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(..)")
    }
}

/// A public key: one GF(p^5) element, the encoded point sk*G, 40 bytes little-endian. The venue
/// lists it as 80 hex digits without `0x`; lighter-go writes it with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; 40]);

/// `s || e`, 80 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; 80]);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadKey {
    #[error("a key is 40 bytes, 80 hex digits")]
    Length,
    #[error("a public key's limbs must each be below the field's modulus")]
    NotCanonical,
}

/// The random source gave nothing: nothing was signed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no randomness to sign with: {0}")]
pub struct NoRandomness(pub String);

impl SecretKey {
    /// Forty bytes, reduced mod n, as lighter-go reads a private key.
    #[must_use]
    pub fn from_le_bytes(bytes: &[u8; 40]) -> SecretKey {
        SecretKey(Scalar::from_le_bytes(bytes))
    }

    /// `0x` and 80 hex digits, as lighter-go writes a private key; the prefix is optional. A
    /// refused key is never quoted back.
    pub fn from_hex(text: &str) -> Result<SecretKey, BadKey> {
        let mut bytes = zeroize::Zeroizing::new([0u8; 40]);
        hex::decode_into(text, bytes.as_mut_slice()).map_err(|_| BadKey::Length)?;
        Ok(SecretKey::from_le_bytes(&bytes))
    }

    #[must_use]
    pub const fn from_scalar(scalar: Scalar) -> SecretKey {
        SecretKey(scalar)
    }

    /// The forty bytes, for storing the key. Wiped when dropped.
    #[must_use]
    pub fn to_le_bytes(&self) -> zeroize::Zeroizing<[u8; 40]> {
        zeroize::Zeroizing::new(self.0.to_le_bytes())
    }

    /// `0x` and 80 hex digits, as lighter-go's `GenerateAPIKey` writes it. Wiped when dropped.
    #[must_use]
    pub fn to_hex(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(hex::encode_prefixed(self.to_le_bytes().as_slice()))
    }

    /// encode(sk*G).
    #[must_use]
    pub fn public_key(&self) -> PublicKey {
        PublicKey(Point::GENERATOR.mul(self.0).encode().to_le_bytes())
    }

    /// The signature of a message already hashed to GF(p^5), with the nonce `k`. A `k` used twice
    /// gives the key away: draw a fresh one each time ([`SecretKey::sign_drawing`]).
    #[must_use]
    pub fn sign_with(&self, hashed: Fp5, k: Scalar) -> Signature {
        let r = Point::GENERATOR.mul(k).encode();
        let e = challenge(r, hashed);
        let s = k.sub(e.mul(self.0));
        let mut bytes = [0u8; 80];
        bytes[..40].copy_from_slice(&s.to_le_bytes());
        bytes[40..].copy_from_slice(&e.to_le_bytes());
        Signature(bytes)
    }

    /// Signs with a nonce drawn from `draw`, forty bytes at a time until one is below n, the way
    /// lighter-go draws it.
    pub fn sign_drawing<E>(
        &self,
        hashed: Fp5,
        draw: impl FnMut(&mut [u8; 40]) -> Result<(), E>,
    ) -> Result<Signature, E> {
        let mut k = sample_scalar(draw)?;
        let signature = self.sign_with(hashed, k);
        k.zeroize();
        Ok(signature)
    }

    /// Signs with a nonce from the operating system (or `crypto.getRandomValues` in the browser).
    #[cfg(feature = "sign")]
    pub fn sign(&self, hashed: Fp5) -> Result<Signature, NoRandomness> {
        self.sign_drawing(hashed, os_draw)
    }

    /// A fresh key from the operating system's randomness, drawn as lighter-go's `GenerateAPIKey`
    /// draws one.
    #[cfg(feature = "sign")]
    pub fn generate() -> Result<SecretKey, NoRandomness> {
        SecretKey::generate_drawing(os_draw)
    }

    /// A fresh key from `draw`.
    pub fn generate_drawing<E>(draw: impl FnMut(&mut [u8; 40]) -> Result<(), E>) -> Result<SecretKey, E> {
        sample_scalar(draw).map(SecretKey)
    }

    /// Whether `signature` is this key's over `hashed`: s*G + e*(sk*G) encodes to the r whose
    /// challenge is e. Every signature is checked so before it is sent.
    #[must_use]
    pub fn verifies(&self, hashed: Fp5, signature: &Signature) -> bool {
        let Some((s, e)) = signature.halves() else { return false };
        let public = Point::GENERATOR.mul(self.0);
        let r = Point::GENERATOR.mul(s).add(public.mul(e)).encode();
        challenge(r, hashed) == e
    }
}

/// e = Poseidon2(r || hashed), read as a scalar.
fn challenge(r: Fp5, hashed: Fp5) -> Scalar {
    let mut preimage = [Fp::ZERO; 10];
    preimage[..5].copy_from_slice(&r.0);
    preimage[5..].copy_from_slice(&hashed.0);
    Scalar::from_fp5(hash_to_fp5(&preimage))
}

/// A scalar drawn as Go's `crypto/rand.Int(rand.Reader, n)` draws one: 40 bytes at a time until
/// one, its top bit cleared, is below n.
pub fn sample_scalar<E>(mut draw: impl FnMut(&mut [u8; 40]) -> Result<(), E>) -> Result<Scalar, E> {
    loop {
        let mut bytes = zeroize::Zeroizing::new([0u8; 40]);
        draw(&mut bytes)?;
        if let Some(scalar) = Scalar::from_rand_draw(&bytes) {
            return Ok(scalar);
        }
    }
}

/// Forty bytes from the operating system, or `crypto.getRandomValues` in the browser and Workers.
#[cfg(feature = "sign")]
pub fn os_draw(bytes: &mut [u8; 40]) -> Result<(), NoRandomness> {
    getrandom02::getrandom(bytes).map_err(|error| NoRandomness(error.to_string()))
}

impl PublicKey {
    #[must_use]
    pub const fn bytes(&self) -> &[u8; 40] {
        &self.0
    }

    /// Forty bytes whose five limbs are each below p.
    pub fn from_bytes(bytes: [u8; 40]) -> Result<PublicKey, BadKey> {
        match Fp5::from_le_bytes(&bytes) {
            Some(_) => Ok(PublicKey(bytes)),
            None => Err(BadKey::NotCanonical),
        }
    }

    /// 80 hex digits, `0x` optional: the venue lists keys without it, lighter-go writes them with.
    pub fn from_hex(text: &str) -> Result<PublicKey, BadKey> {
        let bytes = hex::decode_array::<40>(text).map_err(|_| BadKey::Length)?;
        PublicKey::from_bytes(bytes)
    }

    /// The five limbs, as a transaction that registers the key hashes them.
    #[must_use]
    pub fn limbs(&self) -> Fp5 {
        Fp5::from_le_bytes(&self.0).unwrap_or(Fp5::ZERO)
    }

    /// `0x` and 80 lowercase hex digits.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode_prefixed(&self.0)
    }

    /// Whether `signature` is this key's over `hashed`, as the venue checks it
    /// (poseidon_crypto's `IsSchnorrSignatureValid`): s and e canonical, the key a point, and
    /// Poseidon2(encode(s*G + e*pk) || hashed) equal to e.
    #[must_use]
    pub fn verifies(&self, hashed: Fp5, signature: &Signature) -> bool {
        let Some((s, e)) = signature.halves() else { return false };
        let Some(public) = Point::decode(self.limbs()) else { return false };
        let r = Point::GENERATOR.mul(s).add(public.mul(e)).encode();
        challenge(r, hashed) == e
    }

    /// Whether the venue's listing (no prefix, any case) is this key.
    #[must_use]
    pub fn is(&self, listed: &str) -> bool {
        PublicKey::from_hex(listed).is_ok_and(|key| key == *self)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Signature {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 80]) -> Signature {
        Signature(bytes)
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8; 80] {
        &self.0
    }

    /// `s` and `e`, each below n; `None` for a signature that is not canonical.
    #[must_use]
    pub fn halves(&self) -> Option<(Scalar, Scalar)> {
        let mut s = [0u8; 40];
        let mut e = [0u8; 40];
        s.copy_from_slice(&self.0[..40]);
        e.copy_from_slice(&self.0[40..]);
        Some((Scalar::from_canonical_le_bytes(&s)?, Scalar::from_canonical_le_bytes(&e)?))
    }

    /// Lowercase hex, no prefix: how an auth token carries it.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

#[cfg(test)]
#[path = "tests/schnorr.rs"]
mod tests;
