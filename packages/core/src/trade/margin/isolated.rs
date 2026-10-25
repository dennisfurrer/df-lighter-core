//! `UpdateMargin` (29): collateral moved into or out of an isolated position, in the collateral's
//! millionths (USDC on Lighter, USDG on Robinhood Chain). The amount is hashed as two 32-bit
//! halves, so amounts past 4,294 USDC sign like any other.

use crate::trade::error::{TradeError, units};
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::{COLLATERAL_DECIMALS, Dec, MarketId, exact_units};

/// The most collateral one update moves, in millionths: 2^60 - 1.
pub const MAX_MARGIN_UNITS: u64 = (1 << 60) - 1;

/// Which way the collateral goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarginDirection {
    /// Out of the position, back to the account's cross collateral.
    Remove,
    /// Into the position.
    Add,
}

impl MarginDirection {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            MarginDirection::Remove => 0,
            MarginDirection::Add => 1,
        }
    }
}

/// `UpdateMargin` (29): `amount` millionths of collateral moved `direction` on `market`'s isolated
/// position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateMargin {
    pub market: MarketId,
    /// Millionths of the collateral, 1 to 2^60 - 1.
    pub amount: u64,
    pub direction: MarginDirection,
    pub attributes: Attributes,
}

impl UpdateMargin {
    /// Adds `amount` of collateral (`25` is 25 USDC), exactly: a figure past six places is refused.
    pub fn add(market: MarketId, amount: Dec) -> Result<UpdateMargin, TradeError> {
        UpdateMargin::moving(market, amount, MarginDirection::Add)
    }

    /// Removes `amount` of collateral, exactly.
    pub fn remove(market: MarketId, amount: Dec) -> Result<UpdateMargin, TradeError> {
        UpdateMargin::moving(market, amount, MarginDirection::Remove)
    }

    fn moving(market: MarketId, amount: Dec, direction: MarginDirection) -> Result<UpdateMargin, TradeError> {
        let amount = units("USDCAmount", exact_units(amount, COLLATERAL_DECIMALS, MAX_MARGIN_UNITS))?;
        Ok(UpdateMargin { market, amount, direction, attributes: Attributes::NONE })
    }

    /// The same update carrying `attributes` (a skipped nonce, say).
    #[must_use]
    pub const fn with(mut self, attributes: Attributes) -> UpdateMargin {
        self.attributes = attributes;
        self
    }
}

impl Tx for UpdateMargin {
    fn tx_type(&self) -> TxType {
        TxType::UpdateMargin
    }

    /// The amount as its low and high 32 bits.
    fn hash_fields(&self) -> Vec<Fp> {
        vec![
            element(self.market.get()),
            element(self.amount & 0xFFFF_FFFF),
            element(self.amount >> 32),
            element(self.direction.code()),
        ]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("MarketIndex", number(self.market.get())),
            ("USDCAmount", number(self.amount)),
            ("Direction", number(self.direction.code())),
        ]
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2UpdateMarginTxInfo.Validate`: the attributes, an amount of 1 to 2^60 - 1.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()?;
        if self.amount == 0 {
            return Err(BadTx::new("USDCAmount", "an amount is at least 1"));
        }
        if self.amount > MAX_MARGIN_UNITS {
            return Err(BadTx::new("USDCAmount", "an amount is at most 2^60 - 1"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/isolated.rs"]
mod tests;
