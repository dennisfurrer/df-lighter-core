//! An API key: the Schnorr key that signs every transaction for one account, from one of its 255
//! slots, and the auth tokens its private reads carry. The key is generated here, registered once
//! with the owner's Ethereum signature ([`crate::account::keys::ChangePubKey`]), and from then on
//! signs alone. It can trade, cancel, move money between the owner's own accounts and withdraw to
//! the owner's own address; it cannot send money to anyone else without the owner's signature.

use std::fmt;

#[cfg(feature = "sign")]
use crate::utils::sign::NoRandomness;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Envelope, SignTxError, Signed, Tx, sign_drawing};
use crate::utils::sign::{BadKey, PublicKey, SecretKey, hash_to_fp5};
use crate::utils::{AccountIndex, ApiKeyIndex};

/// The longest an auth token lives: eight hours.
pub const MAX_TOKEN_S: u64 = 8 * 3_600;

/// How long a token minted by a [`crate::client::Client`] lives: seven hours, an hour inside the cap
/// for clock skew.
pub const TOKEN_S: u64 = 7 * 3_600;

/// One key at one slot of one account.
#[derive(Clone)]
pub struct ApiKey {
    account: AccountIndex,
    slot: ApiKeyIndex,
    secret: SecretKey,
    public: PublicKey,
}

/// Its account, slot and public key; never its secret.
impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiKey")
            .field("account", &self.account)
            .field("slot", &self.slot)
            .field("public", &self.public.to_hex())
            .finish_non_exhaustive()
    }
}

impl ApiKey {
    #[must_use]
    pub fn new(account: AccountIndex, slot: ApiKeyIndex, secret: SecretKey) -> ApiKey {
        let public = secret.public_key();
        ApiKey { account, slot, secret, public }
    }

    /// From the private key as lighter-go writes it: `0x` and 80 hex digits.
    pub fn from_hex(account: AccountIndex, slot: ApiKeyIndex, private_key: &str) -> Result<ApiKey, BadKey> {
        Ok(ApiKey::new(account, slot, SecretKey::from_hex(private_key)?))
    }

    /// A fresh key from the operating system's randomness, for `slot` of `account`. It signs
    /// nothing the venue takes until it is registered.
    #[cfg(feature = "sign")]
    pub fn generate(account: AccountIndex, slot: ApiKeyIndex) -> Result<ApiKey, NoRandomness> {
        Ok(ApiKey::new(account, slot, SecretKey::generate()?))
    }

    #[must_use]
    pub const fn account(&self) -> AccountIndex {
        self.account
    }

    #[must_use]
    pub const fn slot(&self) -> ApiKeyIndex {
        self.slot
    }

    #[must_use]
    pub const fn public_key(&self) -> PublicKey {
        self.public
    }

    /// The secret, for storing the key: encrypt it at rest, never log it.
    #[must_use]
    pub const fn secret(&self) -> &SecretKey {
        &self.secret
    }

    /// The same key for another account or slot, as after registering it there.
    #[must_use]
    pub fn at(&self, account: AccountIndex, slot: ApiKeyIndex) -> ApiKey {
        ApiKey { account, slot, ..self.clone() }
    }

    /// The envelope a transaction from this key carries.
    #[must_use]
    pub const fn envelope(&self, nonce: u64, expired_at: u64) -> Envelope {
        Envelope { account: self.account, api_key: self.slot, nonce, expired_at }
    }

    /// Signs `tx` for `chain_id` at `nonce`, lapsing at `expired_at`, the nonce drawn from `draw`.
    pub fn sign_drawing<E: fmt::Display>(
        &self,
        tx: &(impl Tx + ?Sized),
        chain_id: u32,
        nonce: u64,
        expired_at: u64,
        draw: impl FnMut(&mut [u8; 40]) -> Result<(), E>,
    ) -> Result<Signed, SignTxError> {
        sign_drawing(tx, chain_id, &self.envelope(nonce, expired_at), &self.secret, draw)
    }

    /// Signs `tx` for `chain_id` at `nonce`, lapsing at `expired_at`.
    #[cfg(feature = "sign")]
    pub fn sign(
        &self,
        tx: &(impl Tx + ?Sized),
        chain_id: u32,
        nonce: u64,
        expired_at: u64,
    ) -> Result<Signed, SignTxError> {
        self.sign_drawing(tx, chain_id, nonce, expired_at, crate::utils::sign::schnorr::os_draw)
    }

    /// An auth token good until `deadline_s` (unix seconds, at most eight hours ahead):
    /// `{deadline}:{account}:{slot}:{signature}`, the signature over the first three as text.
    pub fn auth_token_drawing<E>(
        &self,
        deadline_s: u64,
        draw: impl FnMut(&mut [u8; 40]) -> Result<(), E>,
    ) -> Result<AuthToken, E> {
        let message = format!("{deadline_s}:{}:{}", self.account, self.slot);
        let elements = Fp::from_le_chunks(message.as_bytes()).unwrap_or_default();
        let signature = self.secret.sign_drawing(hash_to_fp5(&elements), draw)?;
        Ok(AuthToken { text: format!("{message}:{}", signature.to_hex()), deadline_s })
    }

    /// [`ApiKey::auth_token_drawing`] with the operating system's randomness.
    #[cfg(feature = "sign")]
    pub fn auth_token(&self, deadline_s: u64) -> Result<AuthToken, NoRandomness> {
        self.auth_token_drawing(deadline_s, crate::utils::sign::schnorr::os_draw)
    }
}

/// What a private REST read sends as `Authorization` and a private channel as `auth`.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken {
    text: String,
    deadline_s: u64,
}

impl AuthToken {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Unix seconds.
    #[must_use]
    pub const fn deadline_s(&self) -> u64 {
        self.deadline_s
    }
}

/// Its deadline; never the token, which reads the account until it lapses.
impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthToken").field("deadline_s", &self.deadline_s).finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "tests/api_key.rs"]
mod tests;
