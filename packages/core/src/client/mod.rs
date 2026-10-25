//! Connecting to Lighter: the [`Client`] that reads, signs and sends over one network's
//! rate-limited REST API ([`http`]), the two networks, the L1 they settle on ([`evm`]), and the
//! native and browser wires.

pub mod evm;
pub mod http;
#[cfg(feature = "native")]
pub mod native;
pub mod network;
#[cfg(feature = "web")]
pub mod web;

#[cfg(feature = "sign")]
use std::future::Future;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::account::keys::{ApiKey, AuthToken, TOKEN_S, WalletError, api_key};
#[cfg(feature = "sign")]
use crate::account::keys::{ChangePubKey, L1Signature, Wallet, next_nonce, verify};
use crate::client::http::Api;
#[cfg(feature = "sign")]
use crate::client::http::{MOST_IN_BATCH, send_tx, send_tx_batch};
#[cfg(feature = "sign")]
use crate::utils::Address;
use crate::utils::NonceCounter;
use crate::utils::sign::SignTxError;
#[cfg(feature = "sign")]
use crate::utils::sign::{BadTx, Signed, Tx};
#[cfg(feature = "sign")]
use crate::utils::time::TX_LIFETIME_MS;
use crate::utils::time::{MINUTE_MS, ms_to_seconds};

pub use http::{
    ApiError, Call, Http, Kind, Method, OFFLINE, Request, Response, Sent, SentBatch, Spent, Tier, Transport,
    TransportError, read,
};
pub use network::{Collateral, Network, USDC, USDG, UnknownNetwork};

/// Unix milliseconds. Injected: std has no clock in the browser or Workers, and a test pins the
/// time.
pub type Clock = fn() -> u64;

/// An auth token is minted again when it has less than this left: half an hour.
const TOKEN_MARGIN_S: u64 = 30 * MINUTE_MS / 1_000;

/// Why a transaction was not taken. One whose answer never came ([`ApiError::may_have_landed`])
/// may have landed, so it is never sent again.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    #[error(transparent)]
    Sign(#[from] SignTxError),
    #[error(transparent)]
    Api(#[from] ApiError),
    /// The owner's wallet did not sign, or signed as someone else: nothing was sent.
    #[error(transparent)]
    Wallet(#[from] WalletError),
}

/// What a client holds to read an account's own data: nothing, or an [`ApiKey`], which mints the
/// auth tokens private reads and channels carry.
pub trait Credentials {
    /// A token good until `deadline_s`, when this can mint one.
    fn mint(&self, deadline_s: u64) -> Option<AuthToken>;
}

impl Credentials for () {
    fn mint(&self, _deadline_s: u64) -> Option<AuthToken> {
        None
    }
}

/// Mints with the operating system's randomness, so only with the `sign` feature.
impl Credentials for ApiKey {
    #[cfg(feature = "sign")]
    fn mint(&self, deadline_s: u64) -> Option<AuthToken> {
        self.auth_token(deadline_s).ok()
    }

    #[cfg(not(feature = "sign"))]
    fn mint(&self, _deadline_s: u64) -> Option<AuthToken> {
        None
    }
}

/// Reads for any module (`markets::order_books(&client)`) and, with an [`ApiKey`], transactions.
/// `K` is `()` for a client that only reads.
///
/// Each client counts its key's nonces, so give every key one client. A read that is the
/// account's own carries an auth token, minted from the key and kept until shortly before it
/// lapses, or the read-only token the client was given.
pub struct Client<H, K = ()> {
    api: Api<H>,
    key: K,
    nonces: Mutex<NonceCounter>,
    /// Held from signing to the venue's answer, so one client's transactions go out in turn.
    #[cfg(feature = "sign")]
    turn: futures_util::lock::Mutex<()>,
    token: Mutex<Option<AuthToken>>,
    read_token: Option<String>,
}

/// Its network and tier; never its key, its tokens or its transport.
impl<H: Http, K> std::fmt::Debug for Client<H, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("network", &self.network()).field("tier", &self.tier()).finish_non_exhaustive()
    }
}

impl<H: Http> Client<H> {
    /// A client that reads and never signs.
    pub fn reader(http: H, network: Network, clock: Clock) -> Self {
        Client::new(http, network, clock, ())
    }
}

impl<H: Http, K> Client<H, K> {
    /// One network, through `http`, signing with `key`.
    pub fn new(http: H, network: Network, clock: Clock, key: K) -> Self {
        Client {
            api: Api::new(http, network, clock),
            key,
            nonces: Mutex::new(NonceCounter::unknown()),
            #[cfg(feature = "sign")]
            turn: futures_util::lock::Mutex::new(()),
            token: Mutex::new(None),
            read_token: None,
        }
    }

    /// The account's tier, whose budget the client paces its reads against: Standard unless the
    /// account opted into another.
    #[must_use]
    pub fn with_tier(mut self, tier: Tier) -> Self {
        self.api = self.api.with_tier(tier);
        self
    }

    /// A read-only token (`ro:...`, from `tokens/create`), carried by the account's own reads
    /// when the client holds no key to mint one.
    #[must_use]
    pub fn with_read_token(mut self, token: impl Into<String>) -> Self {
        self.read_token = Some(token.into());
        self
    }

    #[must_use]
    pub const fn network(&self) -> Network {
        self.api.network()
    }

    #[must_use]
    pub const fn tier(&self) -> Tier {
        self.api.tier()
    }

    #[must_use]
    pub const fn http(&self) -> &H {
        self.api.http()
    }

    #[must_use]
    pub const fn key(&self) -> &K {
        &self.key
    }

    /// Now, by the client's clock: unix milliseconds.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        (self.api.clock())()
    }

    /// What went out over the last minute, and whether the gate is closed.
    pub fn spent(&self) -> Spent {
        self.api.spent()
    }

    /// Opens, or refreshes, the connection transactions travel on, with the venue's status (`/`),
    /// counted like any other read. Nothing is signed and the answer is dropped: a program warms
    /// before its first order so that order skips DNS and the TLS handshake.
    pub async fn warm(&self) {
        let _ = self.api.call(&Call::get("/"), None).await;
    }

    /// The key's nonce count is wrong or unknown: the next transaction asks `nextNonce`.
    pub fn forget_nonce(&self) {
        self.nonces().forget();
    }

    fn nonces(&self) -> MutexGuard<'_, NonceCounter> {
        self.nonces.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<H: Http, K: Credentials> Client<H, K> {
    /// The token the account's own reads and channels carry: one minted from the key for seven
    /// hours (minted again with half an hour left), else the read-only token; `None` when there
    /// is neither.
    pub fn auth_token(&self) -> Option<String> {
        let now_s = ms_to_seconds(self.now_ms());
        let mut cached = self.token.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(token) = cached.as_ref().filter(|token| token.deadline_s() > now_s + TOKEN_MARGIN_S) {
            return Some(token.as_str().to_owned());
        }
        match self.key.mint(now_s + TOKEN_S) {
            Some(token) => {
                let text = token.as_str().to_owned();
                *cached = Some(token);
                Some(text)
            }
            None => self.read_token.clone(),
        }
    }
}

impl<H: Http, K: Credentials> Transport for Client<H, K> {
    async fn call(&self, call: Call) -> Result<String, TransportError> {
        let token = match call.auth {
            true => Some(self.auth_token().ok_or_else(|| TransportError::NoToken(call.url_path()))?),
            false => None,
        };
        self.api.call(&call, token.as_deref()).await
    }
}

/// Whether the key is in place, which needs no signature.
impl<H: Http> Client<H, ApiKey> {
    /// Whether the venue lists this client's key at its slot: true once a registration has
    /// executed, a few seconds after it is taken.
    pub async fn is_registered(&self) -> Result<bool, ApiError> {
        let listed = api_key(self, self.key.account(), self.key.slot()).await?;
        Ok(listed.is_some_and(|slot| slot.holds(&self.key.public_key())))
    }
}

/// Sending needs the `sign` feature, for the randomness every Schnorr signature draws. Each
/// signature is verified before anything is posted, and each owner's signature recovered.
///
/// One client's transactions go out in turn, each signed at the key's next nonce: counted from
/// `nextNonce`, moved on by every one the venue takes, kept for the next after a refusal (it was
/// turned away before it counted), and asked again after an answer that never came or a refused
/// nonce. A refused nonce is signed and sent once more at the venue's count.
#[cfg(feature = "sign")]
impl<H: Http> Client<H, ApiKey> {
    /// The nonce the key's next transaction signs at: counted here, asked of the venue
    /// (`nextNonce`) when not known. Nothing is taken.
    pub async fn next_nonce(&self) -> Result<u64, ApiError> {
        if let Some(next) = self.nonces().next() {
            return Ok(next);
        }
        let asked = next_nonce(self, self.key.account(), self.key.slot()).await?;
        let mut nonces = self.nonces();
        Ok(match nonces.next() {
            Some(known) => known,
            None => {
                nonces.set(asked);
                asked
            }
        })
    }

    /// When a transaction signed now lapses: ten minutes less a second, as lighter-go signs.
    #[must_use]
    pub fn expiry(&self) -> u64 {
        self.now_ms().saturating_add(TX_LIFETIME_MS)
    }

    /// Signs `tx` at `nonce`, lapsing at `expired_at`. The nonce must be the key's next; nothing
    /// is counted.
    pub fn sign_at(&self, tx: &(impl Tx + ?Sized), nonce: u64, expired_at: u64) -> Result<Signed, SignTxError> {
        self.key.sign(tx, self.network().chain_id(), nonce, expired_at)
    }

    /// Signs `tx` at the key's next nonce, for a program that posts it itself (over the
    /// websocket, say). Its nonce is counted as used: if the venue never takes it, call
    /// [`Client::forget_nonce`].
    pub async fn sign(&self, tx: &(impl Tx + ?Sized)) -> Result<Signed, SendError> {
        let _turn = self.turn.lock().await;
        let nonce = self.next_nonce().await?;
        let signed = self.sign_at(tx, nonce, self.expiry())?;
        self.nonces().set(nonce.saturating_add(1));
        Ok(signed)
    }

    /// Signs `tx` at the key's next nonce and sends it.
    pub async fn send(&self, tx: &(impl Tx + ?Sized)) -> Result<Sent, SendError> {
        self.send_one(tx, None::<&Nobody>).await
    }

    /// Signs `tx`, has `owner` sign what the venue asks the owner to (a key registration, a
    /// transfer to another owner, an integrator approval with fees), and sends it. The owner's
    /// signature must recover to `owner`'s address before anything is posted, and the owner is
    /// never asked twice: a refused nonce comes back as it is.
    pub async fn send_with_owner(&self, tx: &(impl Tx + ?Sized), owner: &impl Wallet) -> Result<Sent, SendError> {
        self.send_one(tx, Some(owner)).await
    }

    async fn send_one<W: Wallet>(&self, tx: &(impl Tx + ?Sized), owner: Option<&W>) -> Result<Sent, SendError> {
        let _turn = self.turn.lock().await;
        let mut again = true;
        loop {
            let nonce = self.next_nonce().await?;
            let signed = self.sign_at(tx, nonce, self.expiry())?;
            let tx_info = match (signed.l1_message(), owner) {
                (Some(message), Some(owner)) => {
                    again = false;
                    owned(&signed, message, owner).await?
                }
                _ => signed.tx_info(),
            };
            match self.settle(nonce, 1, send_tx(self, signed.tx_type().code(), tx_info).await) {
                Err(SendError::Api(error)) if error.is_nonce() && std::mem::take(&mut again) => {}
                answer => return answer,
            }
        }
    }

    /// Sends a transaction signed earlier ([`Client::sign`]) once, with the owner's signature if
    /// it carries one. The nonce is the caller's to settle: forget it if the answer never came.
    pub async fn submit(&self, signed: &Signed, owner_signature: Option<&L1Signature>) -> Result<Sent, ApiError> {
        let tx_info = owner_signature
            .and_then(|signature| signed.tx_info_with_l1(&signature.to_hex()))
            .unwrap_or_else(|| signed.tx_info());
        send_tx(self, signed.tx_type().code(), tx_info).await
    }

    /// Signs up to fifty transactions at consecutive nonces and sends them as one batch. None
    /// may need the owner's signature.
    pub async fn send_batch(&self, txs: &[&dyn Tx]) -> Result<SentBatch, SendError> {
        if txs.is_empty() || txs.len() > MOST_IN_BATCH {
            return Err(SendError::Sign(SignTxError::Bad(BadTx::new("txs", "a batch carries 1 to 50 transactions"))));
        }
        let count = u64::try_from(txs.len()).unwrap_or(u64::MAX);
        let _turn = self.turn.lock().await;
        let mut again = true;
        loop {
            let first = self.next_nonce().await?;
            let expired_at = self.expiry();
            let signed = txs
                .iter()
                .zip(first..)
                .map(|(tx, nonce)| {
                    self.sign_at(*tx, nonce, expired_at).map(|one| (one.tx_type().code(), one.tx_info()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            match self.settle(first, count, send_tx_batch(self, &signed).await) {
                Err(SendError::Api(error)) if error.is_nonce() && std::mem::take(&mut again) => {}
                answer => return answer,
            }
        }
    }

    /// Registers this client's key at its slot, signed by the key and by `owner`, the account's
    /// L1 address. It replaces whatever key sat there: choose the slot with
    /// [`crate::account::keys::pick_slot`]. The slots Lighter's own apps hold (0 to 3, and 157 on
    /// Robinhood Chain) are refused: registering there logs the app out.
    pub async fn register(&self, owner: &impl Wallet) -> Result<Sent, SendError> {
        if self.network().reserves_api_key(self.key.slot()) {
            let why = format!("slot {} belongs to Lighter's own apps", self.key.slot());
            return Err(SendError::Sign(SignTxError::Bad(BadTx::new("ApiKeyIndex", why))));
        }
        self.send_with_owner(&ChangePubKey::new(self.key.public_key()), owner).await
    }

    /// After a send: taken, the count moves past it; an answer that may have landed, or a
    /// refused nonce, sends the next one to the venue for the count; any other refusal leaves
    /// the nonce for the next.
    fn settle<T>(&self, nonce: u64, count: u64, answer: Result<T, ApiError>) -> Result<T, SendError> {
        let mut nonces = self.nonces();
        match &answer {
            Ok(_) => nonces.set(nonce.saturating_add(count)),
            Err(error) if error.may_have_landed() || error.is_nonce() => nonces.forget(),
            Err(_) => {}
        }
        answer.map_err(SendError::Api)
    }
}

/// The owner [`Client::send`] passes: none, so it is never asked.
#[cfg(feature = "sign")]
struct Nobody;

#[cfg(feature = "sign")]
impl Wallet for Nobody {
    fn address(&self) -> Address {
        Address::ZERO
    }

    fn sign_message(&self, _message: &str) -> impl Future<Output = Result<L1Signature, WalletError>> {
        std::future::ready(Err(WalletError::Declined("no owner was given".to_owned())))
    }
}

/// `tx_info` with the owner's signature over `message`, which must recover to the owner.
#[cfg(feature = "sign")]
async fn owned(signed: &Signed, message: &str, owner: &impl Wallet) -> Result<String, SendError> {
    let signature = owner.sign_message(message).await?;
    verify(message, &signature, owner.address())?;
    Ok(signed.tx_info_with_l1(&signature.to_hex()).unwrap_or_else(|| signed.tx_info()))
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/client.rs"]
mod tests;
