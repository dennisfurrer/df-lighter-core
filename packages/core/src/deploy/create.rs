//! `BinaryOptionsOperatorCreateMarket` (49): a binary options book on one of the operator's
//! slots, USDC on both sides, with its window, payout, fees and limits.
//!
//! Units are the book's own: prices in ticks of `10^-price_decimals` USDC (the payout cap, the
//! refund price), sizes in ticks of `10^-size_decimals` shares, quote amounts and limits in
//! millionths of USDC, fees in millionths of notional. The extension multipliers are derived as
//! lighter-go derives them: the quote's is `10^(12 - size_decimals - price_decimals)`, so the two
//! decimals add up to six at most, and the size's is the payout cap times it.
//!
//! The symbol and the two decimals ride in `tx_info` (the symbol as 32 bytes, zero-padded) but
//! are not hashed; everything else is.

use crate::deploy::slot::{MarketSlot, binary_options_slot};
use crate::utils::AssetId;
use crate::utils::MarketKind;
use crate::utils::ids::WIDTH_48;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Envelope, FEE_TICK, Tx, TxType, byte_list, element, number};

/// The most an extension multiplier may be: 2^56 - 1.
pub const MAX_EXTENSION_MULTIPLIER: u64 = (1 << 56) - 1;

/// The most open interest a book may cap at, in millionths of USDC: 2^56 - 1.
pub const MAX_OPEN_INTEREST: u64 = (1 << 56) - 1;

/// The most one order may be, in size ticks: 2^48 - 1.
pub const MAX_ORDER_BASE_AMOUNT: u64 = WIDTH_48;

/// The most one order may be worth, in millionths of USDC: 2^48 - 1.
pub const MAX_ORDER_QUOTE_AMOUNT: u64 = WIDTH_48;

/// A symbol is shorter than 32 bytes of UTF-8.
pub const SYMBOL_BYTES: usize = 32;

/// The places a quote amount is scaled to: USDC's six and the collateral multiplier's six.
const QUOTE_PLACES: u32 = 12;

/// How a book settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Settlement {
    /// At 0 or the payout cap: Up or Down. Lighter's Up/Down series settle so.
    #[default]
    Discrete,
    /// At any price from 0 to the payout cap.
    Continuous,
}

impl Settlement {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Settlement::Discrete => 0,
            Settlement::Continuous => 1,
        }
    }
}

/// `BinaryOptionsOperatorCreateMarket`, as lighter-go's request: the slot must be one Lighter
/// assigned to the envelope's account. The market id it gets is the venue's to give: read it
/// from `events` once the book exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMarket {
    pub slot: MarketSlot,
    /// Unix milliseconds: trading opens.
    pub start_ms: u64,
    /// Unix milliseconds: trading stops; settlement follows.
    pub end_ms: u64,
    /// A winning share's payout in price ticks: `100` is 1.00 USDC at two price decimals.
    pub settlement_cap: u32,
    pub settlement: Settlement,
    /// The price, in ticks, a refund settles at; fixed once the book exists.
    pub default_price: u32,
    /// Millionths of notional: 10,000 is 1%.
    pub taker_fee: u32,
    pub maker_fee: u32,
    /// The smallest order, in size ticks.
    pub min_base_amount: u64,
    /// The smallest order's worth, in millionths of USDC.
    pub min_quote_amount: u64,
    /// The largest order's worth, in millionths of USDC.
    pub order_quote_limit: u64,
    /// The book's open interest cap, in millionths of USDC.
    pub open_interest_limit: u64,
    /// Opens frozen: no order placed, modified or matched until an update thaws it.
    pub frozen: bool,
    /// Under 32 bytes: `ETH-UPDOWN-15M-1790007200`.
    pub symbol: String,
    pub size_decimals: u8,
    pub price_decimals: u8,
}

impl CreateMarket {
    /// `10^(12 - size_decimals - price_decimals)`: quote amounts are USDC's six places times the
    /// collateral multiplier's six. 0, which is refused, when the decimals pass twelve.
    #[must_use]
    pub fn quote_extension_multiplier(&self) -> u64 {
        let used = u32::from(self.size_decimals) + u32::from(self.price_decimals);
        QUOTE_PLACES.checked_sub(used).and_then(|exponent| 10u64.checked_pow(exponent)).unwrap_or(0)
    }

    /// The payout cap times the quote's multiplier; `None` past 64 bits, which is refused.
    #[must_use]
    pub fn size_extension_multiplier(&self) -> Option<u64> {
        u64::from(self.settlement_cap).checked_mul(self.quote_extension_multiplier())
    }

    /// The symbol as `tx_info` carries it: its bytes, zero-padded to 32.
    #[must_use]
    pub fn symbol_bytes(&self) -> [u8; SYMBOL_BYTES] {
        let mut bytes = [0u8; SYMBOL_BYTES];
        for (slot, byte) in bytes.iter_mut().zip(self.symbol.bytes()) {
            *slot = byte;
        }
        bytes
    }
}

fn collateral_id() -> u64 {
    u64::from(AssetId::COLLATERAL.get())
}

/// A window lighter-go takes: starts before it ends, ends by 2^48 - 1.
pub(crate) fn check_window(start_ms: u64, end_ms: u64) -> Result<(), BadTx> {
    if start_ms >= end_ms || end_ms > WIDTH_48 {
        return Err(BadTx::new("StartTimestamp", "a window starts before it ends, which is before 2^48 ms"));
    }
    Ok(())
}

pub(crate) fn check_fees(taker_fee: u32, maker_fee: u32) -> Result<(), BadTx> {
    if taker_fee > FEE_TICK || maker_fee > FEE_TICK {
        return Err(BadTx::new("TakerFee", "a market fee is at most 1,000,000, all of the notional"));
    }
    Ok(())
}

/// The order and interest limits, as create and update check them; spot carries no interest cap.
pub(crate) fn check_limits(
    min_base_amount: u64,
    min_quote_amount: u64,
    order_quote_limit: u64,
    open_interest_limit: u64,
    kind: MarketKind,
) -> Result<(), BadTx> {
    if min_base_amount == 0 || min_base_amount > MAX_ORDER_BASE_AMOUNT {
        return Err(BadTx::new("MinBaseAmount", "the smallest order is 1 to 2^48 - 1 size ticks"));
    }
    if min_quote_amount == 0 || min_quote_amount > MAX_ORDER_QUOTE_AMOUNT {
        return Err(BadTx::new("MinQuoteAmount", "the smallest order's worth is 1 to 2^48 - 1"));
    }
    if order_quote_limit > MAX_ORDER_QUOTE_AMOUNT || order_quote_limit < min_quote_amount {
        return Err(BadTx::new("OrderQuoteLimit", "the largest order's worth is the smallest's to 2^48 - 1"));
    }
    match kind {
        MarketKind::Spot if open_interest_limit != 0 => {
            Err(BadTx::new("OpenInterestLimit", "a spot book carries no open interest limit: 0"))
        }
        MarketKind::Spot => Ok(()),
        _ if open_interest_limit < order_quote_limit || open_interest_limit > MAX_OPEN_INTEREST => {
            Err(BadTx::new("OpenInterestLimit", "the open interest limit is the largest order's to 2^56 - 1"))
        }
        _ => Ok(()),
    }
}

fn check_multiplier(multiplier: Option<u64>, field: &'static str) -> Result<u64, BadTx> {
    match multiplier {
        Some(value) if value != 0 && value <= MAX_EXTENSION_MULTIPLIER && value % u64::from(FEE_TICK) == 0 => Ok(value),
        _ => Err(BadTx::new(field, "a multiplier is a multiple of 10^6 below 2^56: decimals add up to six at most")),
    }
}

impl Tx for CreateMarket {
    fn tx_type(&self) -> TxType {
        TxType::BinaryOptionsCreateMarket
    }

    /// Robinhood Chain lists no binary options.
    fn check_signing(&self, chain_id: u32, _envelope: &Envelope) -> Result<(), BadTx> {
        crate::deploy::check_chain(chain_id)
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![
            element(self.slot.get()),
            element(MarketKind::BinaryOptions.code()),
            element(collateral_id()),
            element(collateral_id()),
            element(self.start_ms),
            element(self.end_ms),
            element(self.size_extension_multiplier().unwrap_or(0)),
            element(self.quote_extension_multiplier()),
            element(self.settlement_cap),
            element(self.settlement.code()),
            element(self.default_price),
            element(self.taker_fee),
            element(self.maker_fee),
            element(self.min_base_amount),
            element(self.min_quote_amount),
            element(self.order_quote_limit),
            element(self.open_interest_limit),
            element(u8::from(self.frozen)),
        ]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("MarketIndex", number(self.slot.get())),
            ("MarketType", number(MarketKind::BinaryOptions.code())),
            ("BaseAssetId", number(collateral_id())),
            ("QuoteAssetId", number(collateral_id())),
            ("StartTimestamp", number(self.start_ms)),
            ("EndTimestamp", number(self.end_ms)),
            ("SizeExtensionMultiplier", number(self.size_extension_multiplier().unwrap_or(0))),
            ("QuoteExtensionMultiplier", number(self.quote_extension_multiplier())),
            ("SettlementCap", number(self.settlement_cap)),
            ("SettlementType", number(self.settlement.code())),
            ("DefaultPrice", number(self.default_price)),
            ("TakerFee", number(self.taker_fee)),
            ("MakerFee", number(self.maker_fee)),
            ("MinBaseAmount", number(self.min_base_amount)),
            ("MinQuoteAmount", number(self.min_quote_amount)),
            ("OrderQuoteLimit", number(self.order_quote_limit)),
            ("OpenInterestLimit", number(self.open_interest_limit)),
            ("IsFrozen", number(u8::from(self.frozen))),
            ("Symbol", byte_list(&self.symbol_bytes())),
            ("SizeDecimals", number(self.size_decimals)),
            ("PriceDecimals", number(self.price_decimals)),
        ]
    }

    /// lighter-go's `Validate`, and its request's symbol check.
    fn check(&self) -> Result<(), BadTx> {
        if self.symbol.len() >= SYMBOL_BYTES {
            return Err(BadTx::new("Symbol", "a symbol is under 32 bytes"));
        }
        binary_options_slot(self.slot)?;
        check_window(self.start_ms, self.end_ms)?;
        let size = check_multiplier(self.size_extension_multiplier(), "SizeExtensionMultiplier")?;
        let quote = check_multiplier(Some(self.quote_extension_multiplier()), "QuoteExtensionMultiplier")?;
        if self.settlement_cap == 0 || size % quote != 0 || size / quote != u64::from(self.settlement_cap) {
            return Err(BadTx::new("SettlementCap", "the payout cap is above 0, the size multiplier over the quote's"));
        }
        if self.default_price > self.settlement_cap {
            return Err(BadTx::new("DefaultPrice", "a refund settles at the payout cap at most"));
        }
        check_fees(self.taker_fee, self.maker_fee)?;
        check_limits(
            self.min_base_amount,
            self.min_quote_amount,
            self.order_quote_limit,
            self.open_interest_limit,
            MarketKind::BinaryOptions,
        )
    }
}

#[cfg(test)]
#[path = "tests/create.rs"]
mod tests;
