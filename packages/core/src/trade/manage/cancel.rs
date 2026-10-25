//! `CancelOrder` (15): one open order, named by the client index it was placed with or by the
//! index the venue gave it ([`OrderRef`]). An order placed from the venue's own app has no client
//! index; its venue index (`order_index` in `accountActiveOrders`) names it.

use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::{MarketId, OrderRef};

/// `CancelOrder` (15): the order `order` on `market`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelOrder {
    pub market: MarketId,
    pub order: OrderRef,
    pub attributes: Attributes,
}

impl CancelOrder {
    #[must_use]
    pub fn new(market: MarketId, order: impl Into<OrderRef>) -> CancelOrder {
        CancelOrder { market, order: order.into(), attributes: Attributes::NONE }
    }

    /// The same cancel carrying `attributes` (a skipped nonce, say).
    #[must_use]
    pub const fn with(mut self, attributes: Attributes) -> CancelOrder {
        self.attributes = attributes;
        self
    }
}

impl Tx for CancelOrder {
    fn tx_type(&self) -> TxType {
        TxType::CancelOrder
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.market.get()), element(self.order.index())]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("MarketIndex", number(self.market.get())), ("Index", number(self.order.index()))]
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2CancelOrderTxInfo.Validate`: the market and the index are in range by
    /// their types, so only the attributes are left.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()
    }
}

#[cfg(test)]
#[path = "tests/cancel.rs"]
mod tests;
