//! `ModifyOrder` (17): an open order's new size, price and trigger, named by its client index or
//! the venue's index; its side, kind and expiry stay.
//!
//! The venue keeps a version for each order (`order_version` in its reads). A modify that names
//! one ([`ModifyOrder::version`]) is refused unless it is above the stored one, so of two racing
//! modifies the higher-numbered wins and a stale one never lands. A modify carries the same
//! integrator attributes as the order it changes.

use crate::trade::error::TradeError;
use crate::trade::orders::Fit;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::{Dec, Grid, MAX_SIZE_UNITS, MarketId, OrderRef, Side};

/// `ModifyOrder` (17): the order `order` on `market` with these amounts, integers on the market's
/// grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifyOrder {
    pub market: MarketId,
    pub order: OrderRef,
    /// The new size, `size * 10^size_decimals`; 0 for a reduce-only order that covers the whole
    /// position.
    pub base_amount: u64,
    /// The new limit, or a taker's worst price, `price * 10^price_decimals`.
    pub price: u32,
    /// The new trigger of a stop or take-profit; 0 for any other order.
    pub trigger_price: u32,
    pub attributes: Attributes,
}

impl ModifyOrder {
    /// The same modify, landing only above the order's version `version` (below 2^48).
    #[must_use]
    pub const fn version(mut self, version: u64) -> ModifyOrder {
        self.attributes = self.attributes.order_version(version);
        self
    }

    /// The same modify carrying `attributes`: the order's own integrator and fees, say. Replaces a
    /// version set before; set it after.
    #[must_use]
    pub const fn with(mut self, attributes: Attributes) -> ModifyOrder {
        self.attributes = attributes;
        self
    }
}

impl Tx for ModifyOrder {
    fn tx_type(&self) -> TxType {
        TxType::ModifyOrder
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![
            element(self.market.get()),
            element(self.order.index()),
            element(self.base_amount),
            element(self.price),
            element(self.trigger_price),
        ]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("MarketIndex", number(self.market.get())),
            ("Index", number(self.order.index())),
            ("BaseAmount", number(self.base_amount)),
            ("Price", number(self.price)),
            ("TriggerPrice", number(self.trigger_price)),
        ]
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2ModifyOrderTxInfo.Validate`: the attributes, a size of at most 2^48 - 1
    /// units, a price of at least one tick.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()?;
        if self.base_amount > MAX_SIZE_UNITS {
            return Err(BadTx::new("BaseAmount", "a size is at most 2^48 - 1 units"));
        }
        if self.price == 0 {
            return Err(BadTx::new("Price", "a price is at least 1"));
        }
        Ok(())
    }
}

/// A modify in decimals, before it is on the grid. The order's side says which way
/// [`Fit::Favour`] rounds its price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifyIntent {
    market: MarketId,
    order: OrderRef,
    side: Side,
    size: Dec,
    price: Dec,
    trigger: Option<Dec>,
    fit: Fit,
}

impl ModifyIntent {
    /// `order`, a `side` order on `market`, to `size` at `price`.
    pub fn new(market: MarketId, order: impl Into<OrderRef>, side: Side, size: Dec, price: Dec) -> ModifyIntent {
        ModifyIntent { market, order: order.into(), side, size, price, trigger: None, fit: Fit::Exact }
    }

    /// A stop's or take-profit's new trigger.
    #[must_use]
    pub const fn trigger(mut self, trigger: Dec) -> ModifyIntent {
        self.trigger = Some(trigger);
        self
    }

    /// How a figure finer than the grid is put on it: [`Fit::Exact`] (refused) unless set.
    #[must_use]
    pub const fn fit(mut self, fit: Fit) -> ModifyIntent {
        self.fit = fit;
        self
    }

    /// The modify on `grid`, checked as lighter-go checks it.
    pub fn build(&self, grid: Grid) -> Result<ModifyOrder, TradeError> {
        let trigger_price = match self.trigger {
            Some(trigger) => self.fit.trigger(grid, trigger)?,
            None => 0,
        };
        let modify = ModifyOrder {
            market: self.market,
            order: self.order,
            base_amount: self.fit.size(grid, self.size)?,
            price: self.fit.price(grid, self.price, self.side)?,
            trigger_price,
            attributes: Attributes::NONE,
        };
        modify.check()?;
        Ok(modify)
    }
}

#[cfg(test)]
#[path = "tests/modify.rs"]
mod tests;
