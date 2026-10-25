//! `UpdateLeverage` (20): a perp position's leverage and margin mode. The venue takes the initial
//! margin fraction, in basis points of the position's value: 10,000 over the leverage, rounded
//! down, so 10x is 1,000 and 3x is 3,333. A market's `min_initial_margin_fraction` caps the
//! leverage (`10,000 / min_initial_margin_fraction`), and a market whose `market_margin_mode` is
//! isolated takes no cross margin: asking for it is refused, never switched to isolated.

use crate::trade::error::TradeError;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::{MarginMode, MarketId};

/// The whole position's value, in the basis points a margin fraction counts in.
pub const MARGIN_FRACTION_TICK: u16 = 10_000;

/// Leverage as the initial margin fraction: `10,000 / leverage` rounded down; `None` for 0x or
/// past 10,000x.
#[must_use]
pub const fn leverage_fraction(leverage: u16) -> Option<u16> {
    match leverage {
        0 => None,
        leverage => match MARGIN_FRACTION_TICK / leverage {
            0 => None,
            fraction => Some(fraction),
        },
    }
}

/// What a market allows a position, from `orderBookDetails`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarginLimits {
    /// `min_initial_margin_fraction`: the thinnest margin, so the most leverage.
    pub min_initial_margin_fraction: u16,
    /// `market_config.market_margin_mode` is 1: isolated margin only.
    pub isolated_only: bool,
}

impl MarginLimits {
    /// The most leverage: `10,000 / min_initial_margin_fraction` rounded down, at least 1.
    #[must_use]
    pub const fn max_leverage(&self) -> u16 {
        match self.min_initial_margin_fraction {
            0 => MARGIN_FRACTION_TICK,
            least => match MARGIN_FRACTION_TICK / least {
                0 => 1,
                most => most,
            },
        }
    }
}

/// `UpdateLeverage` (20): `market`'s position at `initial_margin_fraction`, margined `mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateLeverage {
    pub market: MarketId,
    /// Basis points of the position's value, 1 to 10,000.
    pub initial_margin_fraction: u16,
    pub mode: MarginMode,
    pub attributes: Attributes,
}

impl UpdateLeverage {
    /// `leverage`x (1 to 10,000), margined `mode`.
    pub fn new(market: MarketId, leverage: u16, mode: MarginMode) -> Result<UpdateLeverage, TradeError> {
        let fraction = leverage_fraction(leverage).ok_or(TradeError::NoLeverage)?;
        UpdateLeverage::fraction(market, fraction, mode)
    }

    /// An initial margin fraction of `fraction` basis points (1 to 10,000), margined `mode`.
    pub fn fraction(market: MarketId, fraction: u16, mode: MarginMode) -> Result<UpdateLeverage, TradeError> {
        let update = UpdateLeverage { market, initial_margin_fraction: fraction, mode, attributes: Attributes::NONE };
        update.check()?;
        Ok(update)
    }

    /// The leverage it sets: `10,000 / initial_margin_fraction`, rounded down.
    #[must_use]
    pub const fn leverage(&self) -> u16 {
        match self.initial_margin_fraction {
            0 => 0,
            fraction => MARGIN_FRACTION_TICK / fraction,
        }
    }

    /// The same update, refused past the market's most leverage or as cross margin on a market
    /// that trades isolated only.
    pub fn within(self, limits: MarginLimits) -> Result<UpdateLeverage, TradeError> {
        if self.initial_margin_fraction < limits.min_initial_margin_fraction {
            return Err(TradeError::Leverage { asked: self.leverage(), most: limits.max_leverage() });
        }
        if limits.isolated_only && self.mode == MarginMode::Cross {
            return Err(TradeError::IsolatedOnly);
        }
        Ok(self)
    }

    /// The same update carrying `attributes` (a skipped nonce, say).
    #[must_use]
    pub const fn with(mut self, attributes: Attributes) -> UpdateLeverage {
        self.attributes = attributes;
        self
    }
}

impl Tx for UpdateLeverage {
    fn tx_type(&self) -> TxType {
        TxType::UpdateLeverage
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.market.get()), element(self.initial_margin_fraction), element(self.mode.code())]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("MarketIndex", number(self.market.get())),
            ("InitialMarginFraction", number(self.initial_margin_fraction)),
            ("MarginMode", number(self.mode.code())),
        ]
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2UpdateLeverageTxInfo.Validate`: the attributes, a fraction of 1 to 10,000.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()?;
        if self.initial_margin_fraction == 0 {
            return Err(BadTx::new("InitialMarginFraction", "a margin fraction is at least 1 basis point"));
        }
        if self.initial_margin_fraction > MARGIN_FRACTION_TICK {
            return Err(BadTx::new("InitialMarginFraction", "a margin fraction is at most 10,000, the whole value"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/leverage.rs"]
mod tests;
