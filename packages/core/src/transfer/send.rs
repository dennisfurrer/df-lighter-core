//! `Transfer` (type 12): an asset moved from the signing account to another account, or between
//! one account's perps and spot routes, at once and on the venue.
//!
//! The amount is in the venue's decimals for the asset (the collateral's 6, `assetDetails` for the
//! rest), and the fee (`USDCFee`) is the collateral's, in millionths, charged on top: it must be
//! exactly what `transferFeeInfo` asks for the pair ([`crate::transfer::fees::transfer_fee`]).
//! The memo is 32 bytes the venue carries along and does not hash.
//!
//! Every transfer comes with the text an owner's wallet would sign, and the venue asks for that
//! signature when the receiving account has another owner: send those with
//! [`crate::client::Client::send_with_owner`] ([`transfer_to_owner`]). Between one owner's
//! accounts and routes the API key alone is enough ([`transfer`]).

#[cfg(feature = "sign")]
use crate::account::keys::{ApiKey, Wallet};
use crate::client::Network;
#[cfg(feature = "sign")]
use crate::client::{Client, Http, SendError, Sent};
use crate::utils::hex;
use crate::utils::json::Json;
#[cfg(feature = "sign")]
use crate::utils::sign::SignTxError;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Envelope, Tx, TxType, byte_list, element, hex16, number};
use crate::utils::units::{BadUnits, exact_units};
use crate::utils::{AccountIndex, Address, AssetId, Dec, Route};

/// The most a transfer or a withdrawal moves, and the largest fee: lighter-go's
/// `MaxExchangeUSDC`, 2^60 - 1 units.
pub const MAX_AMOUNT: u64 = (1 << 60) - 1;

/// Thirty-two bytes a transfer carries: zeros, a note, or the L1 address a fast withdrawal pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Memo([u8; 32]);

impl Memo {
    /// Thirty-two zeros: no memo.
    pub const NONE: Memo = Memo([0; 32]);

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Memo {
        Memo(bytes)
    }

    /// An L1 address in the first twenty bytes, then twelve zeros: where a fast withdrawal pays.
    #[must_use]
    pub fn address(address: Address) -> Memo {
        let mut bytes = [0u8; 32];
        bytes[..20].copy_from_slice(address.bytes());
        Memo(bytes)
    }

    /// Text of at most 32 bytes, zero padded; `None` for longer.
    #[must_use]
    pub fn text(text: &str) -> Option<Memo> {
        let raw = text.as_bytes();
        let mut bytes = [0u8; 32];
        bytes.get_mut(..raw.len())?.copy_from_slice(raw);
        Some(Memo(bytes))
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Sixty-four hex digits with no `0x`, as the owner's message writes it.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

/// `Transfer`: `amount` of `asset` from the signing account's `from_route` to `to`'s `to_route`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transfer {
    pub to: AccountIndex,
    pub asset: AssetId,
    pub from_route: Route,
    pub to_route: Route,
    /// Units of the asset at its venue decimals, 1 to [`MAX_AMOUNT`].
    pub amount: u64,
    /// `USDCFee`: the collateral's millionths, charged on top of `amount`.
    pub fee: u64,
    pub memo: Memo,
}

impl Transfer {
    /// `amount` of `asset`, counted in `decimals` (the asset's venue decimals from
    /// `assetDetails`), perps to perps, no fee and no memo. An amount finer than `decimals` is
    /// refused, never rounded.
    pub fn new(to: AccountIndex, asset: AssetId, decimals: u32, amount: Dec) -> Result<Transfer, BadUnits> {
        Ok(Transfer {
            to,
            asset,
            from_route: Route::Perps,
            to_route: Route::Perps,
            amount: exact_units(amount, decimals, MAX_AMOUNT)?,
            fee: 0,
            memo: Memo::NONE,
        })
    }

    /// `amount` of the network's collateral (USDC, or USDG), perps to perps.
    pub fn collateral(network: Network, to: AccountIndex, amount: Dec) -> Result<Transfer, BadUnits> {
        let collateral = network.collateral();
        Transfer::new(to, collateral.asset, collateral.decimals, amount)
    }

    /// The same transfer between these routes: perps to spot moves the collateral between an
    /// account's own two ledgers when `to` is the account itself.
    #[must_use]
    pub const fn routes(mut self, from: Route, to: Route) -> Transfer {
        self.from_route = from;
        self.to_route = to;
        self
    }

    /// The same transfer with the fee `transferFeeInfo` asks, in the collateral's millionths.
    #[must_use]
    pub const fn with_fee(mut self, fee: u64) -> Transfer {
        self.fee = fee;
        self
    }

    #[must_use]
    pub const fn with_memo(mut self, memo: Memo) -> Transfer {
        self.memo = memo;
        self
    }
}

/// lighter-go's split of a 60-bit amount: the low 32 bits, then the rest.
pub(crate) fn halves(amount: u64) -> [Fp; 2] {
    [element(amount & 0xFFFF_FFFF), element(amount >> 32)]
}

impl Tx for Transfer {
    fn tx_type(&self) -> TxType {
        TxType::Transfer
    }

    fn hash_fields(&self) -> Vec<Fp> {
        let mut fields = vec![
            element(self.to.get()),
            element(self.asset.get()),
            element(self.from_route.code()),
            element(self.to_route.code()),
        ];
        fields.extend(halves(self.amount));
        fields.extend(halves(self.fee));
        fields
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("ToAccountIndex", number(self.to.get())),
            ("AssetIndex", number(self.asset.get())),
            ("FromRouteType", number(self.from_route.code())),
            ("ToRouteType", number(self.to_route.code())),
            ("Amount", number(self.amount)),
            ("USDCFee", number(self.fee)),
            ("Memo", byte_list(self.memo.bytes())),
        ]
    }

    fn account_key(&self) -> &'static str {
        "FromAccountIndex"
    }

    fn trailing_l1_sig(&self) -> bool {
        true
    }

    /// Always written, as lighter-go writes it; the venue asks for it only when the receiving
    /// account has another owner.
    fn l1_message(&self, envelope: &Envelope, chain_id: u32) -> Option<String> {
        Some(format!(
            "Transfer\n\nnonce: {}\nfrom: {} (route {})\napi key: {}\nto: {} (route {})\nasset: {}\namount: {}\nfee: {}\nchainId: {}\nmemo: {}\nOnly sign this message for a trusted client!",
            hex16(envelope.nonce),
            hex16(envelope.account.get()),
            hex16(u64::from(self.from_route.code())),
            hex16(u64::from(envelope.api_key.get())),
            hex16(self.to.get()),
            hex16(u64::from(self.to_route.code())),
            hex16(u64::from(self.asset.get())),
            hex16(self.amount),
            hex16(self.fee),
            hex16(u64::from(chain_id)),
            self.memo.to_hex(),
        ))
    }

    /// lighter-go's `L2TransferTxInfo.Validate`; the ids and routes are checked by their types.
    fn check(&self) -> Result<(), BadTx> {
        if self.amount == 0 {
            return Err(BadTx::new("Amount", "a transfer moves at least 1 unit (TransferAmountTooLow)"));
        }
        if self.amount > MAX_AMOUNT {
            return Err(BadTx::new("Amount", "a transfer moves at most 2^60 - 1 units"));
        }
        if self.fee > MAX_AMOUNT {
            return Err(BadTx::new("USDCFee", "a fee is at most 2^60 - 1 units"));
        }
        Ok(())
    }
}

/// Sends `transfer` from the client's account to another account of the same owner (a
/// sub-account, or the account itself between routes), with the fee `transferFeeInfo` asks for
/// the pair. No owner's signature: the API key is enough.
#[cfg(feature = "sign")]
pub async fn transfer<H: Http>(client: &Client<H, ApiKey>, transfer: Transfer) -> Result<Sent, SendError> {
    let transfer = priced(client, transfer).await?;
    client.send(&transfer).await
}

/// Sends `transfer` to an account of another owner, with the fee `transferFeeInfo` asks and
/// `owner`'s signature over the transfer's text, checked before anything is posted.
#[cfg(feature = "sign")]
pub async fn transfer_to_owner<H: Http>(
    client: &Client<H, ApiKey>,
    transfer: Transfer,
    owner: &impl Wallet,
) -> Result<Sent, SendError> {
    let transfer = priced(client, transfer).await?;
    client.send_with_owner(&transfer, owner).await
}

/// `transfer` with the fee the venue asks for its pair of accounts.
#[cfg(feature = "sign")]
async fn priced<H: Http>(client: &Client<H, ApiKey>, transfer: Transfer) -> Result<Transfer, SendError> {
    let fee = crate::transfer::fees::transfer_fee(client, client.key().account(), Some(transfer.to)).await?;
    Ok(transfer.with_fee(fee))
}

/// A refused amount as the signing refusal it is: nothing was signed or sent.
#[cfg(feature = "sign")]
pub(crate) fn refused_units(field: &'static str, error: &BadUnits) -> SendError {
    SendError::Sign(SignTxError::Bad(BadTx::new(field, error.to_string())))
}

#[cfg(test)]
#[path = "tests/send.rs"]
mod tests;
