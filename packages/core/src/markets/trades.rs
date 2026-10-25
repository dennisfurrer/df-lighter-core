//! A trade as every read and channel writes it: `recentTrades`, `trades`, the `trade` channel and
//! an account's own trades. Each names its maker by `is_maker_ask`, so the taker's side is the
//! other one.
//!
//! The reads: a market's tape (`recentTrades`, the newest [`MOST_TRADES`]) and the paged history
//! (`trades`, newest first, a cursor to the next page). A market's history and a public pool's are
//! public; a master account's or a sub-account's carries its auth token ([`TradeQuery::account`]).

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::utils::dec::Dec;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, MarketId, MarketKind, OrderIndex, Side};

/// What made a trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TradeKind {
    /// Two orders crossed.
    Trade,
    /// A liquidation order filled against the book.
    Liquidation,
    /// The venue closed a position against a counterparty's (auto-deleveraging).
    Deleverage,
    /// A binary option settled.
    MarketSettlement,
    #[serde(other)]
    Other,
}

/// One trade.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Trade {
    #[serde(deserialize_with = "whole")]
    pub trade_id: u64,
    /// 80 hex digits, the transaction it executed in.
    pub tx_hash: String,
    #[serde(rename = "type")]
    pub kind: TradeKind,
    pub market_id: MarketId,
    #[serde(default)]
    pub market_symbol: String,
    #[serde(default, deserialize_with = "kind")]
    pub market_kind: Option<MarketKind>,
    /// In the base asset.
    pub size: Dec,
    pub price: Dec,
    /// `size * price`, in the collateral.
    pub usd_amount: Dec,
    /// The venue's index of the ask and of the bid that met.
    #[serde(deserialize_with = "whole")]
    pub ask_id: u64,
    #[serde(deserialize_with = "whole")]
    pub bid_id: u64,
    /// The client indexes they were placed with; 0 when none was given.
    #[serde(default, deserialize_with = "whole")]
    pub ask_client_id: u64,
    #[serde(default, deserialize_with = "whole")]
    pub bid_client_id: u64,
    pub ask_account_id: AccountIndex,
    pub bid_account_id: AccountIndex,
    /// Whether the ask was the resting order: then the bid took.
    pub is_maker_ask: bool,
    #[serde(default, deserialize_with = "whole")]
    pub block_height: u64,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    /// Unix microseconds.
    #[serde(default, deserialize_with = "whole")]
    pub transaction_time: u64,
    /// Fees in millionths of the notional ([`crate::utils::sign::FEE_TICK`]); absent when none was
    /// charged.
    #[serde(default)]
    pub taker_fee: Option<u32>,
    #[serde(default)]
    pub maker_fee: Option<u32>,
    #[serde(default)]
    pub taker_position_size_before: Option<Dec>,
    #[serde(default)]
    pub taker_entry_quote_before: Option<Dec>,
    #[serde(default)]
    pub maker_position_size_before: Option<Dec>,
    #[serde(default)]
    pub maker_entry_quote_before: Option<Dec>,
    /// Realised profit of each side, on an account's own trades.
    #[serde(default)]
    pub ask_account_pnl: Option<Dec>,
    #[serde(default)]
    pub bid_account_pnl: Option<Dec>,
}

fn kind<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<MarketKind>, D::Error> {
    crate::utils::order::known(deserializer)
}

impl Trade {
    /// The resting order's side.
    #[must_use]
    pub const fn maker_side(&self) -> Side {
        Side::from_is_ask(self.is_maker_ask)
    }

    /// The side that crossed the spread: a bid when the maker was an ask.
    #[must_use]
    pub const fn taker_side(&self) -> Side {
        self.maker_side().opposite()
    }

    #[must_use]
    pub const fn maker_account(&self) -> AccountIndex {
        match self.is_maker_ask {
            true => self.ask_account_id,
            false => self.bid_account_id,
        }
    }

    #[must_use]
    pub const fn taker_account(&self) -> AccountIndex {
        match self.is_maker_ask {
            true => self.bid_account_id,
            false => self.ask_account_id,
        }
    }

    /// `account`'s side in this trade, if it was in it.
    #[must_use]
    pub fn side_of(&self, account: AccountIndex) -> Option<Side> {
        match (self.ask_account_id == account, self.bid_account_id == account) {
            (true, _) => Some(Side::Ask),
            (_, true) => Some(Side::Bid),
            _ => None,
        }
    }

    /// The ask's order, when it is a venue order index.
    #[must_use]
    pub fn ask_order(&self) -> Option<OrderIndex> {
        OrderIndex::new(self.ask_id).ok()
    }

    #[must_use]
    pub fn bid_order(&self) -> Option<OrderIndex> {
        OrderIndex::new(self.bid_id).ok()
    }
}

/// The most trades one read answers, `recentTrades` and `trades` alike.
pub const MOST_TRADES: u32 = 100;

/// What `recentTrades` asks: the newest `limit` trades, from 1 to [`MOST_TRADES`] (clamped).
#[must_use]
pub fn recent_trades_call(market: MarketId, limit: u32) -> Call {
    Call::get("recentTrades").field("market_id", market).field("limit", limit.clamp(1, MOST_TRADES))
}

/// A market's newest trades, newest first.
pub async fn recent_trades(transport: &impl Transport, market: MarketId, limit: u32) -> Result<Vec<Trade>, ApiError> {
    Ok(read::<TradePage>(transport, recent_trades_call(market, limit)).await?.trades)
}

/// What `trades` orders by; always newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortBy {
    #[default]
    Timestamp,
    BlockHeight,
    TradeId,
}

impl SortBy {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            SortBy::Timestamp => "timestamp",
            SortBy::BlockHeight => "block_height",
            SortBy::TradeId => "trade_id",
        }
    }
}

/// Which of an account's trades: as maker, as taker, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Role {
    #[default]
    All,
    Maker,
    Taker,
}

impl Role {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Role::All => "all",
            Role::Maker => "maker",
            Role::Taker => "taker",
        }
    }
}

/// One `trades` read: whose, of what, and from where. Build it with [`TradeQuery::market`],
/// [`TradeQuery::account`] or [`TradeQuery::pool`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeQuery {
    pub market: Option<MarketId>,
    /// Perps or spot only; `None` for every kind.
    pub market_kind: Option<MarketKind>,
    pub account: Option<AccountIndex>,
    /// The read is the account's own and carries its auth token.
    pub auth: bool,
    /// One order's trades.
    pub order: Option<OrderIndex>,
    pub sort_by: SortBy,
    /// `next_cursor` of the page before.
    pub cursor: Option<String>,
    pub role: Role,
    /// One kind of trade; `None` for every kind.
    pub kind: Option<TradeKind>,
    /// From 1 to [`MOST_TRADES`] (clamped).
    pub limit: u32,
}

impl TradeQuery {
    fn new(market: Option<MarketId>, account: Option<AccountIndex>, auth: bool, limit: u32) -> TradeQuery {
        TradeQuery {
            market,
            market_kind: None,
            account,
            auth,
            order: None,
            sort_by: SortBy::Timestamp,
            cursor: None,
            role: Role::All,
            kind: None,
            limit,
        }
    }

    /// A market's history, public.
    #[must_use]
    pub fn market(market: MarketId, limit: u32) -> TradeQuery {
        TradeQuery::new(Some(market), None, false, limit)
    }

    /// A master account's or a sub-account's own trades, with its auth token.
    #[must_use]
    pub fn account(account: AccountIndex, limit: u32) -> TradeQuery {
        TradeQuery::new(None, Some(account), true, limit)
    }

    /// A public pool's trades, public.
    #[must_use]
    pub fn pool(pool: AccountIndex, limit: u32) -> TradeQuery {
        TradeQuery::new(None, Some(pool), false, limit)
    }

    /// Only on `market`.
    #[must_use]
    pub fn on(self, market: MarketId) -> TradeQuery {
        TradeQuery { market: Some(market), ..self }
    }

    /// Only perps, or only spot.
    #[must_use]
    pub fn of_kind(self, kind: MarketKind) -> TradeQuery {
        TradeQuery { market_kind: Some(kind), ..self }
    }

    #[must_use]
    pub fn of_order(self, order: OrderIndex) -> TradeQuery {
        TradeQuery { order: Some(order), ..self }
    }

    #[must_use]
    pub fn sorted_by(self, sort_by: SortBy) -> TradeQuery {
        TradeQuery { sort_by, ..self }
    }

    #[must_use]
    pub fn as_role(self, role: Role) -> TradeQuery {
        TradeQuery { role, ..self }
    }

    #[must_use]
    pub fn only(self, kind: TradeKind) -> TradeQuery {
        TradeQuery { kind: Some(kind), ..self }
    }

    /// The page after the one that gave `cursor`.
    #[must_use]
    pub fn after(self, cursor: impl Into<String>) -> TradeQuery {
        TradeQuery { cursor: Some(cursor.into()), ..self }
    }
}

/// One page of `trades`, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct TradePage {
    #[serde(default, deserialize_with = "listed")]
    pub trades: Vec<Trade>,
    /// Where the next page starts; `None` (or empty) after the last.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl TradePage {
    /// The next page's cursor, when there is a next page.
    #[must_use]
    pub fn next(&self) -> Option<&str> {
        self.next_cursor.as_deref().filter(|cursor| !cursor.is_empty() && !self.trades.is_empty())
    }
}

/// The word `type` takes for a kind of trade.
const fn kind_wire(kind: TradeKind) -> &'static str {
    match kind {
        TradeKind::Trade => "trade",
        TradeKind::Liquidation => "liquidation",
        TradeKind::Deleverage => "deleverage",
        TradeKind::MarketSettlement => "market-settlement",
        TradeKind::Other => "all",
    }
}

/// What `trades` asks.
#[must_use]
pub fn trades_call(query: &TradeQuery) -> Call {
    let call = Call::get("trades")
        .maybe("market_id", query.market)
        .maybe("market_type", query.market_kind.map(MarketKind::filter))
        .maybe("account_index", query.account)
        .maybe("order_index", query.order)
        .field("sort_by", query.sort_by.wire())
        .maybe("cursor", query.cursor.as_deref())
        .maybe("role", (query.role != Role::All).then_some(query.role.wire()))
        .maybe("type", query.kind.map(kind_wire))
        .field("limit", query.limit.clamp(1, MOST_TRADES));
    match query.auth {
        true => call.authed(),
        false => call,
    }
}

/// One page of trades.
pub async fn trades(transport: &impl Transport, query: &TradeQuery) -> Result<TradePage, ApiError> {
    read(transport, trades_call(query)).await
}

#[cfg(test)]
#[path = "tests/trades.rs"]
mod tests;
