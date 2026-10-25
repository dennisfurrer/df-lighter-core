//! `BinaryOptionsOperatorUpdateMarket` (52): a live book's mutable terms, set all at once on its
//! operator's behalf: the window, the fees, the limits, and whether it is frozen. Every other term
//! is fixed at creation.
//!
//! Despite its name it reaches any slot an operator holds. A binary options book takes a window
//! and a freeze; a perp book a liquidation fee (at most 1%); a spot book no open interest limit.
//! Whatever does not apply is zero. Units as at creation ([`crate::deploy::create`]).

use crate::deploy::create::{check_fees, check_limits, check_window};
use crate::deploy::slot::MarketSlot;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Envelope, FEE_TICK, Tx, TxType, element, number};
use crate::utils::{MarketId, MarketKind};

/// The most a perp book's liquidation fee may be set to: 1%, 10,000 millionths.
pub const MAX_LIQUIDATION_FEE: u32 = FEE_TICK / 100;

/// `BinaryOptionsOperatorUpdateMarket`, as lighter-go's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateMarket {
    pub slot: MarketSlot,
    /// The id traders know the book by.
    pub market: MarketId,
    /// Unix milliseconds; binary options only, 0 otherwise.
    pub start_ms: u64,
    pub end_ms: u64,
    /// Millionths of notional.
    pub taker_fee: u32,
    pub maker_fee: u32,
    /// Millionths of notional, at most [`MAX_LIQUIDATION_FEE`]; perps only, 0 otherwise.
    pub liquidation_fee: u32,
    /// In size ticks.
    pub min_base_amount: u64,
    /// In millionths of USDC, as the next two.
    pub min_quote_amount: u64,
    pub order_quote_limit: u64,
    /// 0 for spot.
    pub open_interest_limit: u64,
    /// Binary options only: no order placed, modified or matched while frozen; cancels still go.
    pub frozen: bool,
}

impl UpdateMarket {
    /// The same terms, frozen or thawed.
    #[must_use]
    pub const fn with_frozen(self, frozen: bool) -> UpdateMarket {
        UpdateMarket { frozen, ..self }
    }
}

impl Tx for UpdateMarket {
    fn tx_type(&self) -> TxType {
        TxType::BinaryOptionsUpdateMarket
    }

    /// Robinhood Chain lists no binary options.
    fn check_signing(&self, chain_id: u32, _envelope: &Envelope) -> Result<(), BadTx> {
        crate::deploy::check_chain(chain_id)
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![
            element(self.slot.get()),
            element(self.market.get()),
            element(self.start_ms),
            element(self.end_ms),
            element(self.taker_fee),
            element(self.maker_fee),
            element(self.liquidation_fee),
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
            ("PublicMarketIndex", number(self.market.get())),
            ("StartTimestamp", number(self.start_ms)),
            ("EndTimestamp", number(self.end_ms)),
            ("TakerFee", number(self.taker_fee)),
            ("MakerFee", number(self.maker_fee)),
            ("LiquidationFee", number(self.liquidation_fee)),
            ("MinBaseAmount", number(self.min_base_amount)),
            ("MinQuoteAmount", number(self.min_quote_amount)),
            ("OrderQuoteLimit", number(self.order_quote_limit)),
            ("OpenInterestLimit", number(self.open_interest_limit)),
            ("IsFrozen", number(u8::from(self.frozen))),
        ]
    }

    /// lighter-go's `Validate`, by what the slot holds.
    fn check(&self) -> Result<(), BadTx> {
        let kind = self.slot.kind();
        match kind {
            MarketKind::BinaryOptions => check_window(self.start_ms, self.end_ms)?,
            _ if self.start_ms != 0 || self.end_ms != 0 => {
                return Err(BadTx::new("StartTimestamp", "only a binary options book takes a window: 0 otherwise"));
            }
            _ => {}
        }
        check_fees(self.taker_fee, self.maker_fee)?;
        match kind {
            MarketKind::Perps if self.liquidation_fee > MAX_LIQUIDATION_FEE => {
                return Err(BadTx::new("LiquidationFee", "a liquidation fee is at most 10,000, 1%"));
            }
            MarketKind::Perps => {}
            _ if self.liquidation_fee != 0 => {
                return Err(BadTx::new("LiquidationFee", "only a perp book takes a liquidation fee: 0 otherwise"));
            }
            _ => {}
        }
        check_limits(
            self.min_base_amount,
            self.min_quote_amount,
            self.order_quote_limit,
            self.open_interest_limit,
            kind,
        )?;
        if self.frozen && kind != MarketKind::BinaryOptions {
            return Err(BadTx::new("IsFrozen", "only a binary options book freezes"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/update.rs"]
mod tests;
