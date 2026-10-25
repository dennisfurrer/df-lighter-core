//! The account's orders: those still working (`accountActiveOrders`), those that ended
//! (`accountInactiveOrders`, a page at a time), and any by the client indexes they were placed with
//! (`accountOrders`, twenty at a time). All three are the account's own and carry the auth token;
//! each weighs 100.
//!
//! Also what an order implies for an app: the price it rests at (none for a market trigger), its
//! trigger, and when it was placed.

use serde::Deserialize;

use crate::account::history::orders::Order;
use crate::client::{ApiError, Call, Transport, read};
use crate::utils::lenient::list;
use crate::utils::time::seconds_to_ms;
use crate::utils::{AccountIndex, ClientOrderIndex, Dec, MarketId, MarketKind, OrderKind, Side};

/// The most client indexes one `accountOrders` takes.
pub const MOST_CLIENT_INDEXES: usize = 20;

/// The most orders one inactive page holds.
pub const MOST_INACTIVE: u8 = 100;

/// A page of orders, and the cursor to the next.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrderPage {
    #[serde(default, deserialize_with = "list")]
    pub orders: Vec<Order>,
    /// Empty or absent on the last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl OrderPage {
    /// The cursor to the next page, when there is one.
    #[must_use]
    pub fn next(&self) -> Option<&str> {
        self.next_cursor.as_deref().filter(|cursor| !cursor.is_empty())
    }
}

/// `market_type` as a filter writes it: the kinds joined by commas; `None` for every kind.
#[must_use]
pub fn kinds_filter(kinds: &[MarketKind]) -> Option<String> {
    (!kinds.is_empty()).then(|| kinds.iter().map(|kind| kind.filter()).collect::<Vec<_>>().join(","))
}

/// What `accountActiveOrders` asks: every market when `market` is `None`, every kind of market
/// when `kinds` is empty.
#[must_use]
pub fn active_orders_call(account: AccountIndex, market: Option<MarketId>, kinds: &[MarketKind]) -> Call {
    Call::get("accountActiveOrders")
        .field("account_index", account)
        .maybe("market_id", market)
        .maybe("market_type", kinds_filter(kinds))
        .authed()
}

/// The orders still working: resting, waiting on a trigger, or not yet sequenced.
pub async fn active_orders(
    transport: &impl Transport,
    account: AccountIndex,
    market: Option<MarketId>,
    kinds: &[MarketKind],
) -> Result<Vec<Order>, ApiError> {
    Ok(read::<OrderPage>(transport, active_orders_call(account, market, kinds)).await?.orders)
}

/// What `accountInactiveOrders` asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InactiveQuery {
    pub account: AccountIndex,
    pub market: Option<MarketId>,
    /// Empty for every kind.
    pub kinds: Vec<MarketKind>,
    /// One side only (`ask_filter`); `None` for both.
    pub side: Option<Side>,
    /// The venue's own `between_timestamps` text, passed as given.
    pub between_timestamps: Option<String>,
    pub cursor: Option<String>,
    /// 1 to 100.
    pub limit: u8,
}

impl InactiveQuery {
    /// The newest hundred of every market.
    #[must_use]
    pub const fn new(account: AccountIndex) -> InactiveQuery {
        InactiveQuery {
            account,
            market: None,
            kinds: Vec::new(),
            side: None,
            between_timestamps: None,
            cursor: None,
            limit: MOST_INACTIVE,
        }
    }

    /// The next page after `page`, or `None` after the last.
    #[must_use]
    pub fn after(&self, page: &OrderPage) -> Option<InactiveQuery> {
        let cursor = page.next()?;
        Some(InactiveQuery { cursor: Some(cursor.to_owned()), ..self.clone() })
    }
}

/// What `accountInactiveOrders` asks for `query`.
#[must_use]
pub fn inactive_orders_call(query: &InactiveQuery) -> Call {
    Call::get("accountInactiveOrders")
        .field("account_index", query.account)
        .maybe("market_id", query.market)
        .maybe("ask_filter", query.side.map(|side| u8::from(side.is_ask())))
        .maybe("between_timestamps", query.between_timestamps.as_deref())
        .maybe("cursor", query.cursor.as_deref())
        .field("limit", query.limit.clamp(1, MOST_INACTIVE))
        .maybe("market_type", kinds_filter(&query.kinds))
        .authed()
}

/// A page of the orders that ended: filled or cancelled, newest first.
pub async fn inactive_orders(transport: &impl Transport, query: &InactiveQuery) -> Result<OrderPage, ApiError> {
    read(transport, inactive_orders_call(query)).await
}

/// What `accountOrders` asks: up to twenty client indexes. `account` `None` reads the token's own.
#[must_use]
pub fn orders_by_client_index_call(account: Option<AccountIndex>, indexes: &[ClientOrderIndex]) -> Call {
    let listed: Vec<String> = indexes.iter().map(ToString::to_string).collect();
    Call::get("accountOrders").maybe("account_index", account).field("client_order_indexes", listed.join(",")).authed()
}

/// The orders placed with `indexes`, twenty to a request. The venue keeps the last 10,000 active
/// orders and the last 1,000 inactive ones of the past day; an index it no longer holds is missing
/// from the answer.
pub async fn orders_by_client_index(
    transport: &impl Transport,
    account: Option<AccountIndex>,
    indexes: &[ClientOrderIndex],
) -> Result<Vec<Order>, ApiError> {
    let mut orders = Vec::new();
    for chunk in indexes.chunks(MOST_CLIENT_INDEXES) {
        orders.extend(read::<OrderPage>(transport, orders_by_client_index_call(account, chunk)).await?.orders);
    }
    Ok(orders)
}

impl Order {
    /// Whether it waits for a trigger price: a stop or a take-profit.
    #[must_use]
    pub const fn is_trigger(&self) -> bool {
        self.kind.is_trigger()
    }

    /// The price it rests at, or the worst it fills at: `None` for a market order and a market
    /// trigger (stop-loss, take-profit), whose `price` is only an execution bound.
    #[must_use]
    pub fn limit_price(&self) -> Option<Dec> {
        match self.kind {
            OrderKind::Market | OrderKind::StopLoss | OrderKind::TakeProfit => None,
            _ => Some(self.price).filter(|price| price.is_positive()),
        }
    }

    /// The trigger price of a stop or a take-profit; `None` for any other order.
    #[must_use]
    pub fn trigger(&self) -> Option<Dec> {
        Some(self.trigger_price).filter(|price| self.is_trigger() && price.is_positive())
    }

    /// When it was placed, unix milliseconds: `created_at`, else the second it was sequenced.
    #[must_use]
    pub const fn placed_ms(&self) -> u64 {
        match self.created_at {
            0 => seconds_to_ms(self.timestamp),
            created => created,
        }
    }

    /// Size left to fill, in the base asset.
    #[must_use]
    pub const fn remaining(&self) -> Dec {
        self.remaining_base_amount
    }
}

#[cfg(test)]
#[path = "tests/order_reads.rs"]
mod tests;
