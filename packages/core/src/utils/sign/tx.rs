//! What every Lighter transaction shares, as lighter-go builds it: its type, the envelope (account,
//! API key slot, nonce, expiry), the attributes it may carry, its hash and the `tx_info` the venue
//! reads.
//!
//! The hash is Poseidon2 over the chain id, the type, the nonce, the expiry, the account, the API
//! key and then the transaction's own fields, in struct order. With attributes, that hash and the
//! attributes' hash are hashed once more. The hash is what the API key signs and what the venue
//! answers as `tx_hash`. `tx_info` is the struct as Go's `encoding/json` writes it: keys in field
//! order, byte slices in base64, byte arrays as lists of numbers.

use crate::utils::base64;
use crate::utils::hex;
use crate::utils::ids::{AccountIndex, ApiKeyIndex, MarketId, WIDTH_48};
use crate::utils::json::Json;
use crate::utils::sign::fp5::Fp5;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::poseidon2::hash_to_fp5;
use crate::utils::sign::schnorr::{SecretKey, Signature};

/// The transaction types this crate signs, by the number `sendTx` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TxType {
    ChangePubKey,
    CreateSubAccount,
    CreatePublicPool,
    UpdatePublicPool,
    Transfer,
    Withdraw,
    CreateOrder,
    CancelOrder,
    CancelAllOrders,
    ModifyOrder,
    MintShares,
    BurnShares,
    UpdateLeverage,
    CreateGroupedOrders,
    UpdateMargin,
    StakeAssets,
    UnstakeAssets,
    UpdateAccountConfig,
    UpdateAccountAssetConfig,
    ApproveIntegrator,
    BinaryOptionsCreateMarket,
    BinaryOptionsSettleMarket,
    BinaryOptionsUpdateMarket,
}

impl TxType {
    pub const ALL: [TxType; 23] = [
        TxType::ChangePubKey,
        TxType::CreateSubAccount,
        TxType::CreatePublicPool,
        TxType::UpdatePublicPool,
        TxType::Transfer,
        TxType::Withdraw,
        TxType::CreateOrder,
        TxType::CancelOrder,
        TxType::CancelAllOrders,
        TxType::ModifyOrder,
        TxType::MintShares,
        TxType::BurnShares,
        TxType::UpdateLeverage,
        TxType::CreateGroupedOrders,
        TxType::UpdateMargin,
        TxType::StakeAssets,
        TxType::UnstakeAssets,
        TxType::UpdateAccountConfig,
        TxType::UpdateAccountAssetConfig,
        TxType::ApproveIntegrator,
        TxType::BinaryOptionsCreateMarket,
        TxType::BinaryOptionsSettleMarket,
        TxType::BinaryOptionsUpdateMarket,
    ];

    /// lighter-go's `TxTypeL2*` number.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            TxType::ChangePubKey => 8,
            TxType::CreateSubAccount => 9,
            TxType::CreatePublicPool => 10,
            TxType::UpdatePublicPool => 11,
            TxType::Transfer => 12,
            TxType::Withdraw => 13,
            TxType::CreateOrder => 14,
            TxType::CancelOrder => 15,
            TxType::CancelAllOrders => 16,
            TxType::ModifyOrder => 17,
            TxType::MintShares => 18,
            TxType::BurnShares => 19,
            TxType::UpdateLeverage => 20,
            TxType::CreateGroupedOrders => 28,
            TxType::UpdateMargin => 29,
            TxType::StakeAssets => 35,
            TxType::UnstakeAssets => 36,
            TxType::UpdateAccountConfig => 41,
            TxType::UpdateAccountAssetConfig => 42,
            TxType::ApproveIntegrator => 45,
            TxType::BinaryOptionsCreateMarket => 49,
            TxType::BinaryOptionsSettleMarket => 50,
            TxType::BinaryOptionsUpdateMarket => 52,
        }
    }

    /// The type a number names; `None` for one this crate does not sign.
    #[must_use]
    pub fn from_code(code: u8) -> Option<TxType> {
        TxType::ALL.into_iter().find(|tx_type| tx_type.code() == code)
    }

    /// Whether it spends the volume quota a Plus or Premium account's `sendTx` budget counts:
    /// orders, modifies and cancel-all do; a single cancel does not.
    #[must_use]
    pub const fn spends_quota(self) -> bool {
        matches!(
            self,
            TxType::CreateOrder | TxType::CancelAllOrders | TxType::ModifyOrder | TxType::CreateGroupedOrders
        )
    }
}

/// Who signs and when it lapses: the part of every transaction that is not its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    pub account: AccountIndex,
    pub api_key: ApiKeyIndex,
    /// The key's next nonce, from `nextNonce` or counted on from it.
    pub nonce: u64,
    /// Unix milliseconds after which the sequencer drops it; ten minutes after signing by
    /// default ([`crate::utils::time::TX_LIFETIME_MS`]).
    pub expired_at: u64,
}

/// What lighter-go refuses before it signs, field by field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {why}")]
pub struct BadTx {
    pub field: &'static str,
    pub why: String,
}

impl BadTx {
    #[must_use]
    pub fn new(field: &'static str, why: impl Into<String>) -> BadTx {
        BadTx { field, why: why.into() }
    }
}

/// A fee rate in millionths of notional: 100 is one basis point, 1,000,000 all of it.
pub const FEE_TICK: u32 = 1_000_000;

/// Attribute types, as lighter-go numbers them.
const INTEGRATOR_ACCOUNT: u8 = 1;
const INTEGRATOR_TAKER_FEE: u8 = 2;
const INTEGRATOR_MAKER_FEE: u8 = 3;
const SKIP_NONCE: u8 = 4;
const CANCEL_ALL_MARKET: u8 = 5;
const SELF_TRADE_BEHAVIOR: u8 = 6;
const SELF_TRADE_EQUALITY: u8 = 7;
const ORDER_VERSION: u8 = 8;

/// At most four attributes ride on one transaction.
pub const MOST_ATTRIBUTES: usize = 4;

/// What an order does when it would trade against the same account (or the same owner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SelfTrade {
    /// The resting order is cancelled: the venue's default.
    #[default]
    ExpireMaker,
    ExpireTaker,
    ExpireBoth,
    /// The larger order is reduced by the smaller.
    Reduce,
}

/// Which orders count as the same trader's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SelfTradeScope {
    /// The same account index: the default.
    #[default]
    Account,
    /// Any account of the same master.
    Owner,
}

impl SelfTrade {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            SelfTrade::ExpireMaker => 0,
            SelfTrade::ExpireTaker => 1,
            SelfTrade::ExpireBoth => 2,
            SelfTrade::Reduce => 3,
        }
    }
}

impl SelfTradeScope {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            SelfTradeScope::Account => 0,
            SelfTradeScope::Owner => 1,
        }
    }
}

/// The optional attributes (lighter-go's `L2TxAttributes`), each kept only when it is not its
/// default: an integrator and its fees, a skipped nonce, the market a cancel-all is limited to,
/// self-trade handling, an order's version. None at all is `null` in `tx_info` and leaves the
/// hash alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attributes {
    integrator: Option<AccountIndex>,
    taker_fee: u32,
    maker_fee: u32,
    skip_nonce: bool,
    cancel_market: Option<MarketId>,
    self_trade: SelfTrade,
    scope: SelfTradeScope,
    order_version: u64,
}

impl Attributes {
    pub const NONE: Attributes = Attributes {
        integrator: None,
        taker_fee: 0,
        maker_fee: 0,
        skip_nonce: false,
        cancel_market: None,
        self_trade: SelfTrade::ExpireMaker,
        scope: SelfTradeScope::Account,
        order_version: 0,
    };

    /// Credits `account` with the fee rates (millionths; 100 is a basis point) on the taker and
    /// maker side. Zero fees attribute the trade without charging. The client must have approved
    /// the integrator for at least these rates.
    #[must_use]
    pub const fn integrator(mut self, account: AccountIndex, taker_fee: u32, maker_fee: u32) -> Attributes {
        self.integrator = Some(account);
        self.taker_fee = taker_fee;
        self.maker_fee = maker_fee;
        self
    }

    /// Lets the nonce jump ahead rather than be the last plus one.
    #[must_use]
    pub const fn skip_nonce(mut self) -> Attributes {
        self.skip_nonce = true;
        self
    }

    /// Limits a cancel-all to one market; only an immediate one may be.
    #[must_use]
    pub const fn cancel_market(mut self, market: MarketId) -> Attributes {
        self.cancel_market = Some(market);
        self
    }

    /// What happens when the order meets one of the trader's own. Not with integrator fees.
    #[must_use]
    pub const fn self_trade(mut self, behavior: SelfTrade, scope: SelfTradeScope) -> Attributes {
        self.self_trade = behavior;
        self.scope = scope;
        self
    }

    /// A modify only lands when this is above the order's current version.
    #[must_use]
    pub const fn order_version(mut self, version: u64) -> Attributes {
        self.order_version = version;
        self
    }

    #[must_use]
    pub const fn integrator_account(&self) -> Option<AccountIndex> {
        self.integrator
    }

    #[must_use]
    pub const fn fees(&self) -> (u32, u32) {
        (self.taker_fee, self.maker_fee)
    }

    #[must_use]
    pub fn is_none(&self) -> bool {
        self.entries().is_empty()
    }

    /// Each attribute that is not its default, by type, ascending.
    #[must_use]
    pub fn entries(&self) -> Vec<(u8, u64)> {
        let integrator = self.integrator.map(AccountIndex::get).filter(|index| *index != 0);
        [
            integrator.map(|index| (INTEGRATOR_ACCOUNT, index)),
            (self.taker_fee != 0).then_some((INTEGRATOR_TAKER_FEE, u64::from(self.taker_fee))),
            (self.maker_fee != 0).then_some((INTEGRATOR_MAKER_FEE, u64::from(self.maker_fee))),
            self.skip_nonce.then_some((SKIP_NONCE, 1)),
            self.cancel_market.map(|market| (CANCEL_ALL_MARKET, market.get())),
            (self.self_trade != SelfTrade::ExpireMaker)
                .then_some((SELF_TRADE_BEHAVIOR, u64::from(self.self_trade.code()))),
            (self.scope != SelfTradeScope::Account).then_some((SELF_TRADE_EQUALITY, u64::from(self.scope.code()))),
            (self.order_version != 0).then_some((ORDER_VERSION, self.order_version)),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// lighter-go's `L2TxAttributes.Validate`.
    pub fn check(&self) -> Result<(), BadTx> {
        let entries = self.entries();
        if entries.len() > MOST_ATTRIBUTES {
            return Err(BadTx::new(
                "L2TxAttributes",
                format!("at most {MOST_ATTRIBUTES} attributes ride on one transaction"),
            ));
        }
        if self.taker_fee > FEE_TICK || self.maker_fee > FEE_TICK {
            return Err(BadTx::new("L2TxAttributes", "an integrator fee is at most 1,000,000, all of the notional"));
        }
        if self.order_version > WIDTH_48 {
            return Err(BadTx::new("L2TxAttributes", "an order version is below 2^48"));
        }
        let fees = self.taker_fee != 0 || self.maker_fee != 0;
        let integrator = self.integrator.is_some_and(|index| index.get() != 0);
        if fees && !integrator {
            return Err(BadTx::new("L2TxAttributes", "integrator fees need the integrator's account"));
        }
        let self_trade = self.self_trade != SelfTrade::ExpireMaker || self.scope != SelfTradeScope::Account;
        if fees && self_trade {
            return Err(BadTx::new("L2TxAttributes", "self-trade handling does not combine with integrator fees"));
        }
        if self.self_trade == SelfTrade::Reduce && self.scope == SelfTradeScope::Owner {
            return Err(BadTx::new("L2TxAttributes", "reduce is not allowed across an owner's accounts"));
        }
        Ok(())
    }

    /// The four slots, each a type and its value, the empty ones zero.
    fn hash(&self) -> Fp5 {
        let mut elements = [Fp::ZERO; 2 * MOST_ATTRIBUTES];
        for (slot, (kind, value)) in elements.chunks_exact_mut(2).zip(self.entries()) {
            slot[0] = Fp::new(u64::from(kind));
            slot[1] = Fp::new(value);
        }
        hash_to_fp5(&elements)
    }

    /// The transaction's own hash, folded with these attributes when there are any.
    fn aggregate(&self, tx_hash: Fp5) -> Fp5 {
        if self.is_none() {
            return tx_hash;
        }
        let mut both = [Fp::ZERO; 10];
        both[..5].copy_from_slice(&tx_hash.0);
        both[5..].copy_from_slice(&self.hash().0);
        hash_to_fp5(&both)
    }

    /// `{"1":713888,"2":500}`, or `null` for none: a Go map, keys in their string order.
    #[must_use]
    pub fn json(&self) -> Json {
        let entries = self.entries();
        if entries.is_empty() {
            return Json::Null;
        }
        Json::Object(entries.into_iter().map(|(kind, value)| (kind.to_string(), number(value))).collect())
    }
}

/// One transaction of any type: its own fields, for the hash and for `tx_info`. The envelope and
/// the signature are added around them ([`sign`]).
pub trait Tx {
    fn tx_type(&self) -> TxType;

    /// The fields hashed after the account and the API key, in lighter-go's order.
    fn hash_fields(&self) -> Vec<Fp>;

    /// The fields `tx_info` writes between `ApiKeyIndex` and `ExpiredAt`, in Go's field order.
    fn info_fields(&self) -> Vec<(&'static str, Json)>;

    /// The key `tx_info` names the account by: `FromAccountIndex` on a transfer or withdrawal.
    fn account_key(&self) -> &'static str {
        "AccountIndex"
    }

    /// Whether `tx_info` carries `"L1Sig"` after `"Sig"`: a transfer and an integrator approval do.
    fn trailing_l1_sig(&self) -> bool {
        false
    }

    fn attributes(&self) -> Attributes {
        Attributes::NONE
    }

    /// What the owner's wallet signs (EIP-191) for the venue to take it, when it may ask: a key
    /// registration always, a transfer to another owner, an integrator approval with fees.
    fn l1_message(&self, _envelope: &Envelope, _chain_id: u32) -> Option<String> {
        None
    }

    /// lighter-go's checks of the transaction's own fields.
    fn check(&self) -> Result<(), BadTx> {
        Ok(())
    }

    /// The checks that need the chain or the signer: a transaction one venue does not take, or
    /// one only a master account may sign.
    fn check_signing(&self, _chain_id: u32, _envelope: &Envelope) -> Result<(), BadTx> {
        Ok(())
    }
}

/// A transaction's hash: 40 bytes, written as 80 lowercase hex digits without `0x`, exactly the
/// `tx_hash` the venue answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxHash(Fp5);

impl TxHash {
    #[must_use]
    pub const fn element(&self) -> Fp5 {
        self.0
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0.to_le_bytes())
    }
}

impl std::fmt::Display for TxHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The hash the API key signs. Nothing is checked here; [`sign`] checks first.
#[must_use]
pub fn tx_hash(tx: &(impl Tx + ?Sized), chain_id: u32, envelope: &Envelope) -> TxHash {
    let mut elements = vec![
        Fp::new(u64::from(chain_id)),
        Fp::new(u64::from(tx.tx_type().code())),
        Fp::new(envelope.nonce),
        Fp::new(envelope.expired_at),
        Fp::new(envelope.account.get()),
        Fp::new(u64::from(envelope.api_key.get())),
    ];
    elements.extend(tx.hash_fields());
    TxHash(tx.attributes().aggregate(hash_to_fp5(&elements)))
}

/// The envelope's own checks: lighter-go caps the nonce and the expiry at 2^48 - 1.
fn check_envelope(envelope: &Envelope) -> Result<(), BadTx> {
    if envelope.expired_at > WIDTH_48 {
        return Err(BadTx::new("ExpiredAt", "an expiry is unix milliseconds below 2^48"));
    }
    if envelope.nonce > WIDTH_48 {
        return Err(BadTx::new("Nonce", "a nonce is below 2^48"));
    }
    Ok(())
}

/// A transaction signed by an API key, ready for `sendTx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    tx_type: TxType,
    hash: TxHash,
    nonce: u64,
    /// `tx_info` with `L1Sig` empty where the transaction has one.
    info: Vec<(String, Json)>,
    message: Option<String>,
}

impl Signed {
    #[must_use]
    pub const fn tx_type(&self) -> TxType {
        self.tx_type
    }

    #[must_use]
    pub const fn hash(&self) -> TxHash {
        self.hash
    }

    /// The nonce it was signed at.
    #[must_use]
    pub const fn nonce(&self) -> u64 {
        self.nonce
    }

    /// `tx_info` as signed, with no owner's signature in it.
    #[must_use]
    pub fn tx_info(&self) -> String {
        Json::Object(self.info.clone()).compact()
    }

    /// The text the owner's wallet signs (`personal_sign`) for this transaction, when the venue may
    /// ask for one; see [`Tx::l1_message`].
    #[must_use]
    pub fn l1_message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// `tx_info` with the owner's signature (`0x` and 130 hex digits) as `L1Sig`. `None` for a
    /// transaction that carries none.
    #[must_use]
    pub fn tx_info_with_l1(&self, l1_signature: &str) -> Option<String> {
        let mut info = self.info.clone();
        let (_, value) = info.iter_mut().find(|(key, _)| key == "L1Sig")?;
        *value = Json::String(l1_signature.to_owned());
        Some(Json::Object(info).compact())
    }

    /// The signature `tx_info` carries.
    #[must_use]
    pub fn signature(&self) -> Option<Signature> {
        let text = self.info.iter().find(|(key, _)| key == "Sig").and_then(|(_, value)| value.as_str())?;
        let bytes: [u8; 80] = base64::decode(text).ok()?.try_into().ok()?;
        Some(Signature::from_bytes(bytes))
    }
}

/// Why nothing was signed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignTxError {
    #[error(transparent)]
    Bad(#[from] BadTx),
    #[error("{0}")]
    NoRandomness(String),
    /// The signature did not verify against the key that made it: nothing is sent.
    #[error("the signature does not verify; nothing was sent")]
    Unverified,
}

/// Checks `tx` as lighter-go would, hashes it for `chain_id`, signs the hash with `key` (a nonce
/// drawn from `draw`), verifies the signature and writes `tx_info`.
pub fn sign_drawing<E: std::fmt::Display>(
    tx: &(impl Tx + ?Sized),
    chain_id: u32,
    envelope: &Envelope,
    key: &SecretKey,
    draw: impl FnMut(&mut [u8; 40]) -> Result<(), E>,
) -> Result<Signed, SignTxError> {
    tx.check()?;
    tx.check_signing(chain_id, envelope)?;
    tx.attributes().check()?;
    check_envelope(envelope)?;
    let hash = tx_hash(tx, chain_id, envelope);
    let signature =
        key.sign_drawing(hash.element(), draw).map_err(|error| SignTxError::NoRandomness(error.to_string()))?;
    if !key.verifies(hash.element(), &signature) {
        return Err(SignTxError::Unverified);
    }
    Ok(Signed {
        tx_type: tx.tx_type(),
        hash,
        nonce: envelope.nonce,
        info: info(tx, envelope, &signature),
        message: tx.l1_message(envelope, chain_id),
    })
}

/// [`sign_drawing`] with the operating system's randomness.
#[cfg(feature = "sign")]
pub fn sign(
    tx: &(impl Tx + ?Sized),
    chain_id: u32,
    envelope: &Envelope,
    key: &SecretKey,
) -> Result<Signed, SignTxError> {
    sign_drawing(tx, chain_id, envelope, key, crate::utils::sign::schnorr::os_draw)
}

/// `tx_info`, in Go's field order.
fn info(tx: &(impl Tx + ?Sized), envelope: &Envelope, signature: &Signature) -> Vec<(String, Json)> {
    let mut fields: Vec<(String, Json)> = vec![
        (tx.account_key().to_owned(), number(envelope.account.get())),
        ("ApiKeyIndex".to_owned(), number(envelope.api_key.get())),
    ];
    fields.extend(tx.info_fields().into_iter().map(|(key, value)| (key.to_owned(), value)));
    fields.push(("ExpiredAt".to_owned(), number(envelope.expired_at)));
    fields.push(("Nonce".to_owned(), number(envelope.nonce)));
    fields.push(("Sig".to_owned(), Json::String(base64::encode(signature.bytes()))));
    if tx.trailing_l1_sig() {
        fields.push(("L1Sig".to_owned(), Json::String(String::new())));
    }
    fields.push(("L2TxAttributes".to_owned(), tx.attributes().json()));
    fields
}

/// An integer field as `tx_info` writes it.
pub fn number(value: impl Into<i128>) -> Json {
    Json::Number(value.into().to_string())
}

/// A byte array as Go writes a `[N]byte`: a list of numbers.
#[must_use]
pub fn byte_list(bytes: &[u8]) -> Json {
    Json::Array(bytes.iter().map(|byte| number(*byte)).collect())
}

/// A byte slice as Go writes a `[]byte`: base64.
#[must_use]
pub fn byte_string(bytes: &[u8]) -> Json {
    Json::String(base64::encode(bytes))
}

/// An integer as lighter-go's L1 messages write one: `0x` and sixteen hex digits.
#[must_use]
pub fn hex16(value: u64) -> String {
    format!("0x{value:016x}")
}

/// A field element from an integer field; every field this crate signs is below 2^60, inside p.
#[must_use]
pub fn element(value: impl Into<u64>) -> Fp {
    Fp::new(value.into())
}

#[cfg(test)]
#[path = "tests/tx.rs"]
mod tests;
