//! An order as the account's reads and channels write it: `accountActiveOrders`,
//! `accountInactiveOrders`, `accountOrders` and the `account_orders` channels.

use serde::{Deserialize, Deserializer};

use crate::utils::dec::Dec;
use crate::utils::ids::whole;
use crate::utils::order::known;
use crate::utils::{AccountIndex, ClientOrderIndex, MarketId, OrderIndex, OrderKind, OrderRef, Side, TimeInForce};

/// Where an order stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrderStatus {
    /// Taken by the API, not yet by the sequencer.
    InProgress,
    /// A trigger order waiting for its price, or a child waiting for its parent.
    Pending,
    /// Resting on the book.
    Open,
    Filled,
    Canceled,
    CanceledPostOnly,
    CanceledReduceOnly,
    CanceledPositionNotAllowed,
    CanceledMarginNotAllowed,
    CanceledTooMuchSlippage,
    CanceledNotEnoughLiquidity,
    CanceledSelfTrade,
    CanceledExpired,
    CanceledOco,
    CanceledChild,
    CanceledLiquidation,
    CanceledInvalidBalance,
    #[serde(other)]
    Other,
}

impl OrderStatus {
    /// Whether it can still fill: in progress, pending or open.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, OrderStatus::InProgress | OrderStatus::Pending | OrderStatus::Open)
    }

    /// Whether it ended without filling in full, for whatever reason.
    #[must_use]
    pub const fn is_canceled(self) -> bool {
        !self.is_live() && !matches!(self, OrderStatus::Filled | OrderStatus::Other)
    }
}

/// What a trigger order waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerStatus {
    /// Not a trigger order.
    Na,
    Ready,
    MarkPrice,
    Twap,
    ParentOrder,
    #[serde(other)]
    Other,
}

/// One order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Order {
    /// The venue's index, from 2^48 up.
    #[serde(deserialize_with = "whole")]
    pub order_index: u64,
    /// The index it was placed with; 0 when none was given.
    #[serde(default, deserialize_with = "whole")]
    pub client_order_index: u64,
    pub market_index: MarketId,
    pub owner_account_index: AccountIndex,
    pub initial_base_amount: Dec,
    pub remaining_base_amount: Dec,
    #[serde(default)]
    pub filled_base_amount: Option<Dec>,
    #[serde(default)]
    pub filled_quote_amount: Option<Dec>,
    pub price: Dec,
    /// The size and price in the market's integer units, as they were signed.
    #[serde(default, deserialize_with = "whole")]
    pub base_size: u64,
    #[serde(default, deserialize_with = "whole")]
    pub base_price: u64,
    #[serde(default, deserialize_with = "whole")]
    pub nonce: u64,
    pub is_ask: bool,
    #[serde(rename = "type")]
    pub kind: OrderKind,
    /// `None` for a word this build does not know; the venue writes `Unknown` for some.
    #[serde(default, deserialize_with = "known")]
    pub time_in_force: Option<TimeInForce>,
    #[serde(default)]
    pub reduce_only: bool,
    /// `0` for an order with no trigger.
    #[serde(default, deserialize_with = "zero_if_empty")]
    pub trigger_price: Dec,
    /// Unix milliseconds; 0 for an order that does not lapse (an immediate one).
    #[serde(default, deserialize_with = "signed_ms")]
    pub order_expiry: u64,
    pub status: OrderStatus,
    #[serde(default = "no_trigger")]
    pub trigger_status: TriggerStatus,
    #[serde(default, deserialize_with = "signed_ms")]
    pub trigger_time: u64,
    /// The order that placed it, for a TWAP slice or a grouped child; 0 for none.
    #[serde(default, deserialize_with = "whole")]
    pub parent_order_index: u64,
    #[serde(default, deserialize_with = "whole")]
    pub block_height: u64,
    /// Unix seconds.
    #[serde(default, deserialize_with = "whole")]
    pub timestamp: u64,
    /// Unix milliseconds.
    #[serde(default, deserialize_with = "whole")]
    pub created_at: u64,
    #[serde(default, deserialize_with = "whole")]
    pub updated_at: u64,
    /// Unix microseconds.
    #[serde(default, deserialize_with = "whole")]
    pub transaction_time: u64,
    #[serde(default, deserialize_with = "whole")]
    pub order_version: u64,
    #[serde(default)]
    pub market_symbol: String,
}

const fn no_trigger() -> TriggerStatus {
    TriggerStatus::Na
}

/// A decimal, or zero for an empty string.
fn zero_if_empty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Dec, D::Error> {
    let text = String::deserialize(deserializer)?;
    match text.trim().is_empty() {
        true => Ok(Dec::ZERO),
        false => Dec::parse(&text).map_err(serde::de::Error::custom),
    }
}

/// A time the venue may write as `-1` for none: read as 0.
fn signed_ms<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = i64::deserialize(deserializer)?;
    Ok(u64::try_from(value).unwrap_or(0))
}

impl Order {
    #[must_use]
    pub const fn side(&self) -> Side {
        Side::from_is_ask(self.is_ask)
    }

    /// The venue's index, typed.
    #[must_use]
    pub fn index(&self) -> Option<OrderIndex> {
        OrderIndex::new(self.order_index).ok()
    }

    /// The client index it was placed with, if any.
    #[must_use]
    pub fn client_index(&self) -> Option<ClientOrderIndex> {
        ClientOrderIndex::new(self.client_order_index).ok()
    }

    /// How a cancel or a modify names it: the venue's index.
    #[must_use]
    pub fn reference(&self) -> Option<OrderRef> {
        self.index().map(OrderRef::Venue)
    }

    /// Filled so far, in the base asset.
    #[must_use]
    pub fn filled(&self) -> Dec {
        self.filled_base_amount
            .unwrap_or_else(|| self.initial_base_amount.checked_sub(self.remaining_base_amount).unwrap_or(Dec::ZERO))
    }
}

#[cfg(test)]
#[path = "tests/orders.rs"]
mod tests;
