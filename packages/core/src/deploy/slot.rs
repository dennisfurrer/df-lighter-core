//! Market slots: the internal position an order book is created on (lighter-go's `MarketSlot`,
//! an `int16`), distinct from the market id traders use. An operator transaction names both:
//! the slot as `MarketIndex`, the market id as `PublicMarketIndex`. Lighter assigns an operator
//! its slots; the ranges say what each holds.

use std::fmt;

use crate::utils::MarketKind;
use crate::utils::sign::tx::BadTx;

/// Perp books sit on slots 0 to 254.
pub const PERPS_SLOTS: std::ops::RangeInclusive<u16> = 0..=254;

/// Binary options books sit on slots 1000 to 2000.
pub const BINARY_OPTIONS_SLOTS: std::ops::RangeInclusive<u16> = 1000..=2000;

/// Spot books sit on slots 2048 to 4094.
pub const SPOT_SLOTS: std::ops::RangeInclusive<u16> = 2048..=4094;

/// A slot in one of the three ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarketSlot(u16);

impl MarketSlot {
    /// The slot, when it is in a range lighter-go takes.
    pub fn new(slot: u16) -> Result<MarketSlot, BadTx> {
        match kind_of(slot) {
            Some(_) => Ok(MarketSlot(slot)),
            None => Err(BadTx::new("MarketIndex", format!("{slot} is no market slot: 0-254, 1000-2000, 2048-4094"))),
        }
    }

    /// For a constant; a slot outside the ranges does not compile.
    #[must_use]
    pub const fn of<const SLOT: u16>() -> MarketSlot {
        const { assert!(kind_of(SLOT).is_some(), "not a market slot") };
        MarketSlot(SLOT)
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// What a book on this slot trades.
    #[must_use]
    pub const fn kind(self) -> MarketKind {
        match kind_of(self.0) {
            Some(kind) => kind,
            None => MarketKind::Perps,
        }
    }
}

const fn kind_of(slot: u16) -> Option<MarketKind> {
    match slot {
        0..=254 => Some(MarketKind::Perps),
        1000..=2000 => Some(MarketKind::BinaryOptions),
        2048..=4094 => Some(MarketKind::Spot),
        _ => None,
    }
}

impl fmt::Display for MarketSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `slot` must hold binary options: create and settle take no other.
pub(crate) fn binary_options_slot(slot: MarketSlot) -> Result<(), BadTx> {
    match slot.kind() {
        MarketKind::BinaryOptions => Ok(()),
        _ => Err(BadTx::new("MarketIndex", format!("slot {slot} holds no binary options: 1000-2000"))),
    }
}

#[cfg(test)]
#[path = "tests/slot.rs"]
mod tests;
