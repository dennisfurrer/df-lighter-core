//! The account's own trades (`trades` with its index, newest first, a page at a time; the auth
//! token is required for a master or a sub-account, not for a public pool) and each read as the
//! account's fill: its side, whether it made or took, the fee it paid and the profit it realized.
//!
//! A fee is the trade's notional times the side's rate in millionths, to six places. Realized
//! profit is the part of the fill that reduces the position held just before it, at the fill's
//! price against that position's average entry, fees not included; the venue also writes its own
//! figure on the account's trades (`ask_account_pnl`, `bid_account_pnl`).

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::markets::{Trade, TradeKind};
use crate::utils::lenient::list;
use crate::utils::units::COLLATERAL_DECIMALS;
use crate::utils::{AccountIndex, ClientOrderIndex, Dec, MarketId, MarketKind, OrderIndex, Round, Side};

use super::order_reads::kinds_filter;

/// The most trades one page holds.
pub const MOST_TRADES: u8 = 100;

/// How `trades` orders its pages, newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TradeSort {
    #[default]
    Timestamp,
    BlockHeight,
    TradeId,
}

impl TradeSort {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            TradeSort::Timestamp => "timestamp",
            TradeSort::BlockHeight => "block_height",
            TradeSort::TradeId => "trade_id",
        }
    }
}

/// Which side of the book the account was on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Maker,
    Taker,
}

impl Role {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Role::Maker => "maker",
            Role::Taker => "taker",
        }
    }
}

/// What `trades` asks for one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeQuery {
    pub account: AccountIndex,
    pub market: Option<MarketId>,
    /// Empty for every kind of market.
    pub kinds: Vec<MarketKind>,
    pub sort: TradeSort,
    pub cursor: Option<String>,
    /// One side only (`ask_filter`); `None` for both.
    pub side: Option<Side>,
    pub role: Option<Role>,
    /// One kind of trade only: trades, liquidations, deleverages or settlements.
    pub kind: Option<TradeKind>,
    /// The trades of one order.
    pub order: Option<OrderIndex>,
    /// 1 to 100.
    pub limit: u8,
}

impl TradeQuery {
    /// The newest hundred of every market, by time.
    #[must_use]
    pub const fn new(account: AccountIndex) -> TradeQuery {
        TradeQuery {
            account,
            market: None,
            kinds: Vec::new(),
            sort: TradeSort::Timestamp,
            cursor: None,
            side: None,
            role: None,
            kind: None,
            order: None,
            limit: MOST_TRADES,
        }
    }

    /// The next page after `page`, or `None` after the last.
    #[must_use]
    pub fn after(&self, page: &TradePage) -> Option<TradeQuery> {
        let cursor = page.next()?;
        Some(TradeQuery { cursor: Some(cursor.to_owned()), ..self.clone() })
    }
}

/// A page of trades, newest first, and the cursor to the next.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TradePage {
    #[serde(default, deserialize_with = "list")]
    pub trades: Vec<Trade>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl TradePage {
    /// The cursor to the next page, when there is one.
    #[must_use]
    pub fn next(&self) -> Option<&str> {
        self.next_cursor.as_deref().filter(|cursor| !cursor.is_empty())
    }
}

/// `type` as a filter writes it.
pub(crate) const fn kind_filter(kind: TradeKind) -> Option<&'static str> {
    match kind {
        TradeKind::Trade => Some("trade"),
        TradeKind::Liquidation => Some("liquidation"),
        TradeKind::Deleverage => Some("deleverage"),
        TradeKind::MarketSettlement => Some("market-settlement"),
        TradeKind::Other => None,
    }
}

/// What `trades` asks for `query`: the account's own, with its token. Weight 200 on Lighter, 600
/// on Robinhood Chain.
#[must_use]
pub fn account_trades_call(query: &TradeQuery) -> Call {
    Call::get("trades")
        .field("account_index", query.account)
        .maybe("market_id", query.market)
        .maybe("market_type", kinds_filter(&query.kinds))
        .field("sort_by", query.sort.name())
        .maybe("cursor", query.cursor.as_deref())
        .maybe("ask_filter", query.side.map(|side| u8::from(side.is_ask())))
        .maybe("role", query.role.map(Role::name))
        .maybe("type", query.kind.and_then(kind_filter))
        .maybe("order_index", query.order)
        .field("limit", query.limit.clamp(1, MOST_TRADES))
        .authed()
}

/// A page of the account's trades.
pub async fn account_trades(transport: &impl Transport, query: &TradeQuery) -> Result<TradePage, ApiError> {
    read(transport, account_trades_call(query)).await
}

/// The account's fills since `since_ms` (unix milliseconds), newest first, reading at most `pages`
/// pages of a hundred: it stops at the first page that reaches back past `since_ms` or at the last.
/// More than `pages * 100` fills since then come back cut at the oldest page read.
pub async fn fills_since(
    transport: &impl Transport,
    account: AccountIndex,
    kinds: &[MarketKind],
    since_ms: u64,
    pages: usize,
) -> Result<Vec<Fill>, ApiError> {
    let mut query = TradeQuery { kinds: kinds.to_vec(), ..TradeQuery::new(account) };
    let mut fills = Vec::new();
    for _ in 0..pages {
        let page = account_trades(transport, &query).await?;
        let reached = page.trades.iter().any(|trade| trade.timestamp < since_ms);
        fills.extend(
            page.trades.iter().filter(|trade| trade.timestamp >= since_ms).filter_map(|trade| Fill::of(trade, account)),
        );
        match query.after(&page) {
            Some(next) if !reached && !page.trades.is_empty() => query = next,
            _ => break,
        }
    }
    Ok(fills)
}

/// One trade as one account's fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    pub trade: Trade,
    /// Whose fill it is.
    pub account: AccountIndex,
    /// Bid for a buy, ask for a sell.
    pub side: Side,
    /// Whether the account's order was resting.
    pub is_maker: bool,
    /// The side's fee rate, millionths of notional; 0 when none was charged.
    pub fee_rate: u32,
    /// What it paid, in the collateral, to six places.
    pub fee: Dec,
    /// Profit realized by the part that reduced the position held before, fees not included; zero
    /// for a fill that only added; `None` when the trade does not carry the position before.
    pub realized_pnl: Option<Dec>,
}

impl Fill {
    /// `trade` as `account`'s fill; `None` when the account is on neither side.
    #[must_use]
    pub fn of(trade: &Trade, account: AccountIndex) -> Option<Fill> {
        let side = trade.side_of(account)?;
        let is_maker = trade.maker_side() == side;
        let (rate, before, entry_quote) = match is_maker {
            true => (trade.maker_fee, trade.maker_position_size_before, trade.maker_entry_quote_before),
            false => (trade.taker_fee, trade.taker_position_size_before, trade.taker_entry_quote_before),
        };
        let fee_rate = rate.unwrap_or(0);
        Some(Fill {
            trade: trade.clone(),
            account,
            side,
            is_maker,
            fee_rate,
            fee: fee(trade.usd_amount, fee_rate),
            realized_pnl: realized(side, trade.size, trade.price, before, entry_quote),
        })
    }

    #[must_use]
    pub const fn market(&self) -> MarketId {
        self.trade.market_id
    }

    #[must_use]
    pub const fn price(&self) -> Dec {
        self.trade.price
    }

    /// In the base asset.
    #[must_use]
    pub const fn size(&self) -> Dec {
        self.trade.size
    }

    /// Unix milliseconds.
    #[must_use]
    pub const fn timestamp_ms(&self) -> u64 {
        self.trade.timestamp
    }

    /// The venue's index of the account's order.
    #[must_use]
    pub const fn order_index(&self) -> u64 {
        match self.side {
            Side::Bid => self.trade.bid_id,
            Side::Ask => self.trade.ask_id,
        }
    }

    /// The client index the account's order was placed with, if any.
    #[must_use]
    pub fn client_order_index(&self) -> Option<ClientOrderIndex> {
        let index = match self.side {
            Side::Bid => self.trade.bid_client_id,
            Side::Ask => self.trade.ask_client_id,
        };
        ClientOrderIndex::new(index).ok()
    }

    /// Whether it came from a liquidation or a deleverage rather than two orders crossing.
    #[must_use]
    pub const fn is_liquidation(&self) -> bool {
        matches!(self.trade.kind, TradeKind::Liquidation | TradeKind::Deleverage)
    }

    /// The venue's own realized profit for the account's side, when it wrote one.
    #[must_use]
    pub const fn venue_pnl(&self) -> Option<Dec> {
        match self.side {
            Side::Bid => self.trade.bid_account_pnl,
            Side::Ask => self.trade.ask_account_pnl,
        }
    }
}

/// `notional * rate / 1,000,000` ([`FEE_TICK`](crate::utils::sign::FEE_TICK)), to six places.
#[must_use]
pub fn fee(notional: Dec, rate: u32) -> Dec {
    let fraction = Dec::new(i128::from(rate), 6).unwrap_or(Dec::ZERO);
    notional.checked_mul(fraction).map_or(Dec::ZERO, |fee| fee.round(COLLATERAL_DECIMALS, Round::Nearest))
}

/// Profit realized by a fill of `size` at `price` on `side`, against the position held before
/// (`before`, signed: negative short) and what it cost (`entry_quote`, its average entry times its
/// size). Only a fill against the position closes any of it: `closed * (price - entry)` for a long,
/// negated for a short, to six places, worked as one division so nothing rounds twice.
#[must_use]
pub fn realized(side: Side, size: Dec, price: Dec, before: Option<Dec>, entry_quote: Option<Dec>) -> Option<Dec> {
    let (before, entry_quote) = (before?, entry_quote?);
    let long = before.is_positive();
    if before.is_zero() || (long && side == Side::Bid) || (!long && side == Side::Ask) {
        return Some(Dec::ZERO);
    }
    let held = before.abs();
    let closed = size.min(held);
    // closed * price - closed * entry_quote / held, over held
    let top = closed.checked_mul(price)?.checked_mul(held)?.checked_sub(closed.checked_mul(entry_quote.abs())?)?;
    let pnl = top.checked_div(held, COLLATERAL_DECIMALS, Round::Nearest)?;
    Some(match long {
        true => pnl,
        false => pnl.negate(),
    })
}

#[cfg(test)]
#[path = "tests/fills.rs"]
mod tests;
