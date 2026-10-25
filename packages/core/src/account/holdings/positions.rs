//! What an account holds, as `account` and the account channels write it: a position per perp
//! market, a balance per asset, and its shares in public pools.

use serde::Deserialize;

use crate::utils::dec::Dec;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, AssetId, MarginMode, MarketId, Side};

/// One perp market's position. The venue lists every market the account ever touched, flat ones
/// included; [`Position::is_open`] tells them apart.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Position {
    pub market_id: MarketId,
    #[serde(default)]
    pub symbol: String,
    /// The margin the position's leverage asks, in percent: `2.63` is 38x.
    pub initial_margin_fraction: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub open_order_count: u64,
    #[serde(default, deserialize_with = "whole")]
    pub pending_order_count: u64,
    #[serde(default, deserialize_with = "whole")]
    pub position_tied_order_count: u64,
    /// `1` long, `-1` short; a flat position reads either.
    pub sign: i8,
    /// The size, always positive: [`Position::signed_size`] carries the sign.
    pub position: Dec,
    pub avg_entry_price: Dec,
    pub position_value: Dec,
    pub unrealized_pnl: Dec,
    pub realized_pnl: Dec,
    /// `0` when there is none.
    pub liquidation_price: Dec,
    #[serde(default)]
    pub total_funding_paid_out: Option<Dec>,
    pub margin_mode: MarginMode,
    /// The isolated margin; zero for a cross position.
    pub allocated_margin: Dec,
    #[serde(default)]
    pub total_discount: Option<Dec>,
}

impl Position {
    #[must_use]
    pub fn is_open(&self) -> bool {
        !self.position.is_zero()
    }

    /// Long positive, short negative.
    #[must_use]
    pub fn signed_size(&self) -> Dec {
        match self.sign < 0 {
            true => self.position.negate(),
            false => self.position,
        }
    }

    /// Bid for a long, ask for a short; `None` when flat.
    #[must_use]
    pub fn side(&self) -> Option<Side> {
        match (self.is_open(), self.sign < 0) {
            (false, _) => None,
            (true, true) => Some(Side::Ask),
            (true, false) => Some(Side::Bid),
        }
    }
}

/// One asset's balance.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub symbol: String,
    pub asset_id: AssetId,
    /// In spot: what the account holds of it.
    pub balance: Dec,
    /// Held by open spot orders.
    pub locked_balance: Dec,
    /// What it counts for as perps margin.
    #[serde(default)]
    pub margin_balance: Dec,
    /// `enabled` when the asset backs perps, `disabled` otherwise.
    #[serde(default)]
    pub margin_mode: String,
    /// How much of its value counts as margin.
    #[serde(default)]
    pub multiplier: Dec,
}

impl Asset {
    #[must_use]
    pub fn backs_perps(&self) -> bool {
        self.margin_mode == "enabled"
    }

    /// What is free to trade or move: the balance less what open orders hold.
    #[must_use]
    pub fn free(&self) -> Dec {
        self.balance.checked_sub(self.locked_balance).unwrap_or(self.balance)
    }
}

/// Shares the account holds in one public pool (the LLP among them).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PoolShare {
    #[serde(rename = "public_pool_index")]
    pub pool: AccountIndex,
    #[serde(rename = "shares_amount", deserialize_with = "whole")]
    pub shares: u64,
    /// The collateral paid in for them.
    pub entry_usdc: Dec,
    /// What they are worth now in the pool's own count.
    pub principal_amount: Dec,
    /// Unix seconds; 0 when the venue does not know.
    #[serde(default, deserialize_with = "whole")]
    pub entry_timestamp: u64,
}

#[cfg(test)]
#[path = "tests/positions.rs"]
mod tests;
