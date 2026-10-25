//! A market's grid: the decimals its sizes and prices are signed at. An order carries both as
//! integers, `base_amount = size * 10^size_decimals` and `price = price * 10^price_decimals`, so a
//! figure off the grid cannot be signed at all; it is put on the grid first, here.

use crate::utils::dec::{Dec, Round};
use crate::utils::ids::WIDTH_48;
use crate::utils::units::{BadUnits, exact_units};

/// The largest price an order signs: a `uint32`.
pub const MAX_PRICE_UNITS: u64 = u32::MAX as u64;

/// The largest size an order signs: 2^48 - 1 units.
pub const MAX_SIZE_UNITS: u64 = WIDTH_48;

/// One market's decimals, from `orderBookDetails` (perps and spot) or `events` (binary options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Grid {
    size_decimals: u32,
    price_decimals: u32,
}

impl Grid {
    #[must_use]
    pub const fn new(size_decimals: u32, price_decimals: u32) -> Grid {
        Grid { size_decimals, price_decimals }
    }

    #[must_use]
    pub const fn size_decimals(&self) -> u32 {
        self.size_decimals
    }

    #[must_use]
    pub const fn price_decimals(&self) -> u32 {
        self.price_decimals
    }

    /// The smallest size: one unit.
    #[must_use]
    pub fn size_step(&self) -> Dec {
        Dec::new(1, self.size_decimals).unwrap_or(Dec::ONE)
    }

    /// The smallest price move: one tick.
    #[must_use]
    pub fn tick(&self) -> Dec {
        Dec::new(1, self.price_decimals).unwrap_or(Dec::ONE)
    }

    /// `size` cut toward zero onto the grid; `None` when nothing is left.
    #[must_use]
    pub fn size(&self, size: Dec) -> Option<Dec> {
        let cut = size.round(self.size_decimals, Round::Down);
        cut.is_positive().then_some(cut)
    }

    /// `price` onto the grid, rounded as asked: a buy's limit rounds down and a sell's up, so the
    /// grid never costs the trader.
    #[must_use]
    pub fn price(&self, price: Dec, round: Round) -> Option<Dec> {
        let placed = price.round(self.price_decimals, round);
        placed.is_positive().then_some(placed)
    }

    /// The integer a size signs as: exact, from one unit to 2^48 - 1.
    pub fn base_amount(&self, size: Dec) -> Result<u64, BadUnits> {
        exact_units(size, self.size_decimals, MAX_SIZE_UNITS)
    }

    /// The integer a price signs as: exact, from one tick to 2^32 - 1.
    pub fn price_units(&self, price: Dec) -> Result<u32, BadUnits> {
        let units = exact_units(price, self.price_decimals, MAX_PRICE_UNITS)?;
        u32::try_from(units).map_err(|_| BadUnits::TooLarge {
            value: price,
            places: self.price_decimals,
            most: MAX_PRICE_UNITS,
        })
    }

    /// The size an integer amount stands for.
    #[must_use]
    pub fn size_of(&self, base_amount: u64) -> Dec {
        Dec::new(i128::from(base_amount), self.size_decimals).unwrap_or(Dec::ZERO)
    }

    /// The price an integer stands for.
    #[must_use]
    pub fn price_of(&self, units: u32) -> Dec {
        Dec::new(i128::from(units), self.price_decimals).unwrap_or(Dec::ZERO)
    }
}

#[cfg(test)]
#[path = "tests/grid.rs"]
mod tests;
