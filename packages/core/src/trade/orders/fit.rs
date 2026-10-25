//! A decimal onto a market's grid, as the integer it signs as: refused when it is finer than the
//! grid unless the caller says how to round. And a taker's worst price: a reference moved by a
//! slippage, rounded back toward the reference so the bound is never looser than asked.

use crate::trade::error::{TradeError, units};
use crate::trade::orders::intent::MAX_SLIPPAGE_BPS;
use crate::utils::{Dec, Grid, Round, Side};

/// Basis points in the whole: 10,000.
const BPS_DECIMALS: u32 = 4;

/// How a size or price finer than the grid is put on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Refused: what was asked is what is signed, or nothing is.
    #[default]
    Exact,
    /// In the trader's favour: a size cut down, a buy's price down and a sell's up, a trigger to
    /// the nearest tick (where it arms favours neither side).
    Favour,
    /// Every figure rounded as asked, sizes and triggers included.
    Round(Round),
}

impl Fit {
    /// `size` as `base_amount`: 1 to 2^48 - 1 units.
    pub fn size(self, grid: Grid, size: Dec) -> Result<u64, TradeError> {
        let placed = match self {
            Fit::Exact => size,
            Fit::Favour => size.round(grid.size_decimals(), Round::Down),
            Fit::Round(round) => size.round(grid.size_decimals(), round),
        };
        units("BaseAmount", grid.base_amount(placed))
    }

    /// A limit (or a worst price the trader typed) for `side` as `price`: 1 to 2^32 - 1 ticks.
    pub fn price(self, grid: Grid, price: Dec, side: Side) -> Result<u32, TradeError> {
        let placed = match self {
            Fit::Exact => price,
            Fit::Favour => price.round(grid.price_decimals(), toward_less_cost(side)),
            Fit::Round(round) => price.round(grid.price_decimals(), round),
        };
        units("Price", grid.price_units(placed))
    }

    /// A trigger as `trigger_price`: 1 to 2^32 - 1 ticks.
    pub fn trigger(self, grid: Grid, trigger: Dec) -> Result<u32, TradeError> {
        let placed = match self {
            Fit::Exact => trigger,
            Fit::Favour => trigger.round(grid.price_decimals(), Round::Nearest),
            Fit::Round(round) => trigger.round(grid.price_decimals(), round),
        };
        units("TriggerPrice", grid.price_units(placed))
    }

    /// A size worth `quote` at `price`, as `base_amount`: cut down (or rounded as
    /// [`Fit::Round`] asks), since the division is ours, not the trader's.
    pub fn size_for(self, grid: Grid, quote: Dec, price: Dec) -> Result<u64, TradeError> {
        if !price.is_positive() {
            return Err(TradeError::NoPrice("a quote-sized order"));
        }
        let round = match self {
            Fit::Round(round) => round,
            Fit::Exact | Fit::Favour => Round::Down,
        };
        let size =
            quote.checked_div(price, grid.size_decimals(), round).ok_or(TradeError::NoPrice("a quote-sized order"))?;
        units("BaseAmount", grid.base_amount(size))
    }
}

/// Which way a price rounds so it costs `side` no more: a bid down, an ask up.
const fn toward_less_cost(side: Side) -> Round {
    match side {
        Side::Bid => Round::Down,
        Side::Ask => Round::Up,
    }
}

/// The worst price a taker on `side` accepts: `reference` moved `slippage_bps` against it (up for a
/// buy, down for a sell), then onto `grid` back toward `reference`, so the bound is never looser
/// than asked. 65,000 at 200 is 66,300 for a buy; 65,000.05 is 63,700.1 for a sell (63,700.049
/// rounded up). At most [`MAX_SLIPPAGE_BPS`].
pub fn worst_price(grid: Grid, side: Side, reference: Dec, slippage_bps: u32) -> Result<Dec, TradeError> {
    if !reference.is_positive() {
        return Err(TradeError::NoPrice("a reference"));
    }
    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(TradeError::Slippage(slippage_bps));
    }
    let fraction = Dec::new(i128::from(slippage_bps), BPS_DECIMALS).ok_or(TradeError::Slippage(slippage_bps))?;
    let moved = reference.checked_mul(fraction).and_then(|off| match side {
        Side::Bid => reference.checked_add(off),
        Side::Ask => reference.checked_sub(off),
    });
    moved.and_then(|moved| grid.price(moved, toward_less_cost(side))).ok_or(TradeError::NoPrice("a reference"))
}

#[cfg(test)]
#[path = "tests/fit.rs"]
mod tests;
