//! A [`U256`] amount and an exact [`Dec`]: wei to the coin and back at a named
//! number of decimals, refused rather than rounded.

use crate::client::evm::u256::U256;
use crate::utils::{Dec, Round};

/// ETH, the gas coin on Ethereum and on Robinhood Chain, has 18 decimals.
pub const NATIVE_DECIMALS: u32 = 18;

/// Why an amount does not convert between wei and a [`Dec`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadAmount {
    #[error("{0} is not a positive amount")]
    NotPositive(Dec),
    #[error("{value} has more than {decimals} decimal places")]
    TooFine { value: Dec, decimals: u32 },
    #[error("the amount is larger than this conversion can count")]
    TooLarge,
    /// A token counted in more than eighteen places, and an amount that needs
    /// them: a [`Dec`] keeps eighteen.
    #[error("{amount} at {decimals} decimals needs more places than a Dec keeps")]
    Unkept { amount: U256, decimals: u32 },
}

/// The places a [`Dec`] keeps.
const DEC_PLACES: u32 = crate::utils::dec::MAX_SCALE;

impl U256 {
    /// The amount as a decimal of `decimals` places: wei at 18 is the coin.
    /// Exact, or refused: past eighteen places the digits a [`Dec`] cannot
    /// keep must be zero.
    pub fn to_dec(self, decimals: u32) -> Result<Dec, BadAmount> {
        let (units, scale) = match decimals.checked_sub(DEC_PLACES) {
            None | Some(0) => (self, decimals),
            Some(extra) => {
                let unkept = BadAmount::Unkept { amount: self, decimals };
                let step = U256::ten_to(extra).ok_or_else(|| unkept.clone())?;
                let (quotient, rest) = self.div_rem(step).ok_or(BadAmount::TooLarge)?;
                if !rest.is_zero() {
                    return Err(unkept);
                }
                (quotient, DEC_PLACES)
            }
        };
        let units = units.to_u128().and_then(|units| i128::try_from(units).ok()).ok_or(BadAmount::TooLarge)?;
        Dec::new(units, scale).ok_or(BadAmount::TooLarge)
    }

    /// `value` counted in `10^-decimals`: 1.5 at 18 is 1.5e18 wei. A value
    /// finer than `decimals` is refused, never rounded.
    pub fn from_dec(value: Dec, decimals: u32) -> Result<U256, BadAmount> {
        if value.is_negative() {
            return Err(BadAmount::NotPositive(value));
        }
        let places = value.places();
        if places > decimals {
            return Err(BadAmount::TooFine { value, decimals });
        }
        let units = value
            .units_at(places, Round::Down)
            .and_then(|units| u128::try_from(units).ok())
            .ok_or(BadAmount::TooLarge)?;
        U256::ten_to(decimals - places)
            .and_then(|step| U256::from_u128(units).checked_mul(step))
            .ok_or(BadAmount::TooLarge)
    }
}

#[cfg(test)]
#[path = "tests/amount.rs"]
mod tests;
