//! What a position's row implies: its leverage, the margin behind it, its entry and liquidation
//! prices when there are any, and the leverage and margin mode an account has set on a market.
//!
//! The venue writes a position's `initial_margin_fraction` in percent (`"5.00"` is 20x, `"33.33"`
//! is 3x) and stores it as `floor(10000 / leverage)` basis points, so `10000 / bps`, floored,
//! gives back the leverage set. A cross position's margin is the initial margin its notional
//! asks; an isolated one's is what was allocated to it.

use crate::account::holdings::account::Account;
use crate::account::holdings::positions::Position;
use crate::utils::units::COLLATERAL_DECIMALS;
use crate::utils::{Dec, MarginMode, MarketId, Round};

/// Basis points in the whole: an initial margin fraction of 10,000 is 1x.
pub const BPS: u32 = 10_000;

/// The leverage and margin mode an account has set on a market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Setting {
    pub market: MarketId,
    /// Whole leverage: `floor(10000 / imf_bps)`.
    pub leverage: u16,
    pub margin_mode: MarginMode,
    /// The initial margin fraction in basis points: 500 is 20x.
    pub imf_bps: u32,
}

/// An initial margin fraction in percent (`"33.33"`) as basis points (3333), rounded to the
/// nearest; `None` for zero, a negative or more than a whole.
#[must_use]
pub fn imf_bps(percent: Dec) -> Option<u32> {
    let bps = percent.units_at(2, Round::Nearest)?;
    u32::try_from(bps).ok().filter(|bps| (1..=BPS).contains(bps))
}

/// The leverage an initial margin fraction in basis points stands for: `floor(10000 / bps)`.
#[must_use]
pub fn leverage_of(imf_bps: u32) -> Option<u16> {
    BPS.checked_div(imf_bps).and_then(|leverage| u16::try_from(leverage).ok()).filter(|leverage| *leverage > 0)
}

impl Position {
    /// The initial margin fraction in basis points: `"2.63"` is 263.
    #[must_use]
    pub fn imf_bps(&self) -> Option<u32> {
        imf_bps(self.initial_margin_fraction)
    }

    /// The leverage set on the market: `"2.63"` is 38x, `"33.33"` 3x.
    #[must_use]
    pub fn leverage(&self) -> Option<u16> {
        self.imf_bps().and_then(leverage_of)
    }

    /// The notional, positive: `position_value` without its sign.
    #[must_use]
    pub fn notional(&self) -> Dec {
        self.position_value.abs()
    }

    /// The margin behind it, in the collateral: an isolated position's allocated margin, a cross
    /// position's initial margin (its notional times the fraction, to six places). `None` for a
    /// cross position whose fraction does not read.
    #[must_use]
    pub fn margin(&self) -> Option<Dec> {
        match self.margin_mode {
            MarginMode::Isolated => Some(self.allocated_margin),
            MarginMode::Cross => {
                let fraction = Dec::new(i128::from(self.imf_bps()?), 4)?;
                Some(self.notional().checked_mul(fraction)?.round(COLLATERAL_DECIMALS, Round::Nearest))
            }
        }
    }

    /// The average entry price; `None` when flat (the venue writes 0).
    #[must_use]
    pub fn entry_price(&self) -> Option<Dec> {
        Some(self.avg_entry_price).filter(|price| price.is_positive())
    }

    /// The liquidation price; `None` when there is none (the venue writes 0).
    #[must_use]
    pub fn liquidation(&self) -> Option<Dec> {
        Some(self.liquidation_price).filter(|price| price.is_positive())
    }

    /// The leverage and margin mode this row carries.
    #[must_use]
    pub fn setting(&self) -> Option<Setting> {
        let imf_bps = self.imf_bps()?;
        let leverage = leverage_of(imf_bps)?;
        Some(Setting { market: self.market_id, leverage, margin_mode: self.margin_mode, imf_bps })
    }
}

impl Account {
    /// The leverage and margin mode set on `market`, open or flat. `None` for a market the account
    /// never set nor traded: the venue lists no row for it, and the market's default applies.
    #[must_use]
    pub fn setting(&self, market: MarketId) -> Option<Setting> {
        self.position(market)?.setting()
    }
}

#[cfg(test)]
#[path = "tests/margin.rs"]
mod tests;
