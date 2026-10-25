//! Amounts a transaction carries as whole integers: sizes and prices scaled by a market's
//! decimals, collateral in millionths, an asset in its own Lighter decimals (8 for ETH and LIT,
//! 6 for the collateral). The integer is what is signed, so it is made here, once.

use crate::utils::dec::{Dec, Round};

/// The collateral's decimals on both venues, USDC and USDG alike: millionths.
pub const COLLATERAL_DECIMALS: u32 = 6;

/// LIT counts in 1e8 on the venue, staking shares included.
pub const LIT_DECIMALS: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadUnits {
    #[error("{0} is not a positive amount")]
    NotPositive(Dec),
    #[error("{value} has more than {places} decimal places")]
    TooFine { value: Dec, places: u32 },
    #[error("{value} is more than {most} units at {places} places")]
    TooLarge { value: Dec, places: u32, most: u64 },
}

/// `value` as a count of `10^-places`, refused rather than rounded when it is finer than that:
/// what was typed is what is signed. At most `most` units.
pub fn exact_units(value: Dec, places: u32, most: u64) -> Result<u64, BadUnits> {
    if value.places() > places {
        return Err(BadUnits::TooFine { value, places });
    }
    cut_units(value, places, most)
}

/// `value` cut toward zero to a count of `10^-places`: for a computed figure (a whole balance, a
/// notional sized down), so it never asks for more than there is. At most `most` units.
pub fn cut_units(value: Dec, places: u32, most: u64) -> Result<u64, BadUnits> {
    if !value.is_positive() {
        return Err(BadUnits::NotPositive(value));
    }
    let too_large = || BadUnits::TooLarge { value, places, most };
    let units = value.units_at(places, Round::Down).ok_or_else(too_large)?;
    match u64::try_from(units) {
        Ok(0) => Err(BadUnits::NotPositive(value)),
        Ok(units) if units <= most => Ok(units),
        _ => Err(too_large()),
    }
}

/// `units / 10^places`, the decimal an integer amount stands for. `None` past what a [`Dec`]
/// holds.
#[must_use]
pub fn from_units(units: i128, places: u32) -> Option<Dec> {
    Dec::new(units, places).map(|value| value.round(places, Round::Down))
}

#[cfg(test)]
#[path = "tests/units.rs"]
mod tests;
