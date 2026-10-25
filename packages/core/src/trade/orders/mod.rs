//! Orders: one order as it is signed ([`Order`], lighter-go's `OrderInfo`), `CreateOrder` (14)
//! that places it, and what builds one.
//!
//! - [`intent`]: an order in decimals ([`OrderIntent`]) put on a market's grid: limit, market,
//!   stops and take-profits, TWAP; the venue's defaults for slippage and expiry.
//! - [`fit`]: how a decimal goes onto the grid ([`Fit`]), and a market order's worst price.
//! - [`client_index`]: client order indexes from the clock ([`ClientIndices`]).
//! - [`grouped`]: `CreateGroupedOrders` (28): an entry with its take profit and stop (OTO, OTOCO),
//!   or a take profit and a stop that cancel each other (OCO).
//!
//! An order's amounts are integers on its market's grid: `base_amount = size * 10^size_decimals`,
//! `price = price * 10^price_decimals`. A resting order's price is its limit; a taker's (a market
//! order, an immediate-or-cancel limit, a stop or take-profit once triggered) is the worst price it
//! fills at, the rest cancelled. Every order but an immediate-or-cancel one lapses at its expiry,
//! unix milliseconds, 5 minutes to 30 days ahead; the venue's own apps rest them 28 days.

pub mod client_index;
pub mod fit;
pub mod grouped;
pub mod intent;

pub use client_index::ClientIndices;
pub use fit::{Fit, worst_price};
pub use grouped::{CreateGroupedOrders, Grouping, MOST_IN_GROUP};
pub use intent::{
    DEFAULT_SLIPPAGE_BPS, Exit, MAX_SLIPPAGE_BPS, OrderIntent, Size, TRIGGER_SLIPPAGE_BPS, TWAP_LEAST_MS, TWAP_MOST_MS,
};

use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::{ClientOrderIndex, Dec, Grid, MAX_SIZE_UNITS, MarketId, OrderKind, Side, TimeInForce};

/// The latest expiry lighter-go takes: an `int64` of milliseconds.
const MOST_EXPIRY: u64 = i64::MAX.unsigned_abs();

/// One order as it is signed (lighter-go's `OrderInfo`): every amount an integer on its market's
/// grid. [`OrderIntent`] builds one from decimals; a literal is signed as written once
/// [`Order::check`] passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Order {
    pub market: MarketId,
    /// The index it is placed with, unique across the account's open orders on every market, to
    /// cancel or modify it by. `None` writes 0: the order is then named by the venue's index alone.
    pub client_index: Option<ClientOrderIndex>,
    /// `size * 10^size_decimals`, 1 to 2^48 - 1. Zero only on a reduce-only order, which then
    /// covers the whole position, and on a grouped order's child, which takes its entry's size.
    pub base_amount: u64,
    /// `price * 10^price_decimals`, at least 1: a resting order's limit, a taker's worst price.
    pub price: u32,
    pub side: Side,
    pub kind: OrderKind,
    pub time_in_force: TimeInForce,
    /// Only ever shrinks the position.
    pub reduce_only: bool,
    /// `trigger * 10^price_decimals` for a stop or take-profit, which fires when the mark crosses
    /// it; 0 for every other order.
    pub trigger_price: u32,
    /// Unix milliseconds the order lapses at; 0 (none) for an immediate-or-cancel limit and a
    /// market order.
    pub expiry: u64,
}

impl Order {
    /// The same order placed under `index`.
    #[must_use]
    pub const fn with_client_index(mut self, index: ClientOrderIndex) -> Order {
        self.client_index = Some(index);
        self
    }

    /// The size it is for, read back off `grid`.
    #[must_use]
    pub fn size(&self, grid: Grid) -> Dec {
        grid.size_of(self.base_amount)
    }

    /// Its limit, or a taker's worst price, read back off `grid`.
    #[must_use]
    pub fn limit(&self, grid: Grid) -> Dec {
        grid.price_of(self.price)
    }

    /// Its trigger, read back off `grid`; `None` for an order that waits for none.
    #[must_use]
    pub fn trigger(&self, grid: Grid) -> Option<Dec> {
        (self.trigger_price != 0).then(|| grid.price_of(self.trigger_price))
    }

    /// Whether it takes liquidity rather than rests: a market order, an immediate-or-cancel
    /// limit, a stop or take-profit that fires as a market order, a TWAP (its slices are market
    /// orders). The venue's least size and value bind the others only.
    #[must_use]
    pub const fn takes(&self) -> bool {
        match self.kind {
            OrderKind::Limit => matches!(self.time_in_force, TimeInForce::ImmediateOrCancel),
            OrderKind::StopLossLimit | OrderKind::TakeProfitLimit => false,
            _ => true,
        }
    }

    /// lighter-go's checks of a `CreateOrder`: [`Order::check_fields`], then what its kind asks of
    /// its time in force, trigger and expiry.
    pub fn check(&self) -> Result<(), BadTx> {
        self.check_fields()?;
        let ioc = matches!(self.time_in_force, TimeInForce::ImmediateOrCancel);
        let triggered = self.trigger_price != 0;
        let expires = self.expiry != 0;
        match self.kind {
            OrderKind::Market => {
                if !ioc {
                    return Err(BadTx::new("TimeInForce", "a market order is immediate-or-cancel"));
                }
                if expires {
                    return Err(BadTx::new("OrderExpiry", "a market order has no expiry"));
                }
                if triggered {
                    return Err(BadTx::new("TriggerPrice", "a market order has no trigger"));
                }
            }
            OrderKind::Limit => {
                if triggered {
                    return Err(BadTx::new("TriggerPrice", "a limit order has no trigger"));
                }
                if ioc && expires {
                    return Err(BadTx::new("OrderExpiry", "an immediate-or-cancel limit has no expiry"));
                }
                if !ioc && !expires {
                    return Err(BadTx::new("OrderExpiry", "a resting limit needs an expiry"));
                }
            }
            kind if kind.is_trigger() => self.check_trigger()?,
            OrderKind::Twap => {
                if !matches!(self.time_in_force, TimeInForce::GoodTillTime) {
                    return Err(BadTx::new("TimeInForce", "a TWAP is good-till-time"));
                }
                if triggered {
                    return Err(BadTx::new("TriggerPrice", "a TWAP has no trigger"));
                }
                if !expires {
                    return Err(BadTx::new("OrderExpiry", "a TWAP needs the time it ends"));
                }
            }
            _ => return Err(BadTx::new("Type", "the venue places TWAP slices and liquidations itself")),
        }
        Ok(())
    }

    /// The checks every order takes, alone or in a group: a size unless reduce-only, at most
    /// 2^48 - 1 units; a price of at least one tick; an expiry an `int64` holds.
    pub fn check_fields(&self) -> Result<(), BadTx> {
        if !self.reduce_only && self.base_amount == 0 {
            return Err(BadTx::new("BaseAmount", "an order that is not reduce-only needs a size of at least 1"));
        }
        if self.base_amount > MAX_SIZE_UNITS {
            return Err(BadTx::new("BaseAmount", "a size is at most 2^48 - 1 units"));
        }
        if self.price == 0 {
            return Err(BadTx::new("Price", "a price is at least 1"));
        }
        if self.expiry > MOST_EXPIRY {
            return Err(BadTx::new("OrderExpiry", "an expiry is unix milliseconds an int64 holds"));
        }
        Ok(())
    }

    /// A stop's or take-profit's own rules: a trigger and an expiry, and immediate-or-cancel when
    /// it fires as a market order.
    pub(crate) fn check_trigger(&self) -> Result<(), BadTx> {
        let market = matches!(self.kind, OrderKind::StopLoss | OrderKind::TakeProfit);
        if market && !matches!(self.time_in_force, TimeInForce::ImmediateOrCancel) {
            return Err(BadTx::new("TimeInForce", "a stop or take-profit that fires at market is immediate-or-cancel"));
        }
        if self.trigger_price == 0 {
            return Err(BadTx::new("TriggerPrice", "a stop or take-profit needs a trigger"));
        }
        if self.expiry == 0 {
            return Err(BadTx::new("OrderExpiry", "a stop or take-profit needs an expiry"));
        }
        Ok(())
    }

    /// The ten fields lighter-go hashes, in its order.
    pub(crate) fn hash_fields(&self) -> [Fp; 10] {
        [
            element(self.market.get()),
            element(self.client_index.map_or(0, ClientOrderIndex::get)),
            element(self.base_amount),
            element(self.price),
            element(u8::from(self.side.is_ask())),
            element(self.kind.code()),
            element(self.time_in_force.code()),
            element(u8::from(self.reduce_only)),
            element(self.trigger_price),
            element(self.expiry),
        ]
    }

    /// The same ten as `tx_info` writes them.
    pub(crate) fn info(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("MarketIndex", number(self.market.get())),
            ("ClientOrderIndex", number(self.client_index.map_or(0, ClientOrderIndex::get))),
            ("BaseAmount", number(self.base_amount)),
            ("Price", number(self.price)),
            ("IsAsk", number(u8::from(self.side.is_ask()))),
            ("Type", number(self.kind.code())),
            ("TimeInForce", number(self.time_in_force.code())),
            ("ReduceOnly", number(u8::from(self.reduce_only))),
            ("TriggerPrice", number(self.trigger_price)),
            ("OrderExpiry", number(self.expiry)),
        ]
    }
}

/// `CreateOrder` (14): one order, with what attributes it carries (an integrator and its fees,
/// self-trade handling, a skipped nonce).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateOrder {
    pub order: Order,
    pub attributes: Attributes,
}

impl CreateOrder {
    #[must_use]
    pub const fn new(order: Order) -> CreateOrder {
        CreateOrder { order, attributes: Attributes::NONE }
    }

    /// The same order carrying `attributes`.
    #[must_use]
    pub const fn with(mut self, attributes: Attributes) -> CreateOrder {
        self.attributes = attributes;
        self
    }
}

impl From<Order> for CreateOrder {
    fn from(order: Order) -> CreateOrder {
        CreateOrder::new(order)
    }
}

impl Tx for CreateOrder {
    fn tx_type(&self) -> TxType {
        TxType::CreateOrder
    }

    fn hash_fields(&self) -> Vec<Fp> {
        self.order.hash_fields().to_vec()
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        self.order.info()
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2CreateOrderTxInfo.Validate`: the attributes first, then the order.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()?;
        self.order.check()
    }
}

#[cfg(test)]
#[path = "tests/create.rs"]
mod tests;
