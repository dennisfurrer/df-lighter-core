//! `orderBookOrders`: a market's resting orders one by one, each with the account that owns it,
//! from the touch outward, at most [`MOST_BOOK_ORDERS`] a side, to anyone. A [`Book`] gives the
//! touch, depth by price, and the walk a taker's size makes through the levels: what a market
//! order's price bound is built from.
//!
//! The venue lists orders, not levels, and cuts each side at the limit asked: a side as long as
//! the limit may run on past its worst listed price, so the last level of a cut side may hold more
//! than is shown ([`Book::is_cut`], [`Book::edge`]). A shorter side was the whole side. A trade
//! names the orders it took ([`orders_filled`]), which is how a fill is told from a pull between
//! two reads.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::markets::trades::Trade;
use crate::utils::dec::{Dec, MAX_SCALE, Round};
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, MarketId, OrderIndex, Side};

/// The most orders a side the venue lists: it refuses 251.
pub const MOST_BOOK_ORDERS: u32 = 250;

/// One resting order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BookOrder {
    /// The venue's order index; `order_id` writes the same digits.
    #[serde(deserialize_with = "whole")]
    pub order_index: u64,
    #[serde(default)]
    pub order_id: String,
    pub owner_account_index: AccountIndex,
    /// The size it was placed with, in the base asset.
    pub initial_base_amount: Dec,
    /// What is left to fill.
    pub remaining_base_amount: Dec,
    pub price: Dec,
    /// When it lapses, unix milliseconds.
    #[serde(default, deserialize_with = "whole")]
    pub order_expiry: u64,
    /// Written 0 here.
    #[serde(default, deserialize_with = "whole")]
    pub transaction_time: u64,
}

impl BookOrder {
    /// The order, from `order_id`'s digits, else `order_index`.
    #[must_use]
    pub fn order(&self) -> Option<OrderIndex> {
        let index = self.order_id.parse::<u64>().unwrap_or(self.order_index);
        OrderIndex::new(index).ok()
    }

    /// What has filled already.
    #[must_use]
    pub fn filled(&self) -> Dec {
        self.initial_base_amount.checked_sub(self.remaining_base_amount).unwrap_or(Dec::ZERO)
    }
}

/// One price level: every order resting at `price`, summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub price: Dec,
    /// In the base asset.
    pub size: Dec,
    /// How many orders rest there.
    pub orders: u32,
}

/// A taker's size walked through one side of a [`Book`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walk {
    /// What the listed orders fill: the size asked, or all there is.
    pub filled: Dec,
    /// `price * size` over every level taken, exact.
    pub notional: Dec,
    /// `notional / filled` to eighteen places, rounded against the taker: up for a buy, down
    /// for a sell. `None` when nothing fills.
    pub average: Option<Dec>,
    /// The last level the walk reached: the price bound that fills it all. `None` when nothing
    /// fills.
    pub worst: Option<Dec>,
    /// Whether the listed orders fill the whole size. A side cut at the limit may hold more past
    /// what was read.
    pub complete: bool,
}

/// A market's resting orders as one `orderBookOrders` read listed them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Book {
    /// The limit the read asked each side for; 0 when it is not known, and then no side counts
    /// as cut.
    #[serde(skip)]
    pub limit: u32,
    /// Highest first.
    #[serde(default, deserialize_with = "listed")]
    pub bids: Vec<BookOrder>,
    /// Lowest first.
    #[serde(default, deserialize_with = "listed")]
    pub asks: Vec<BookOrder>,
}

impl Book {
    /// One side's orders, best first.
    #[must_use]
    pub fn side(&self, side: Side) -> &[BookOrder] {
        match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        }
    }

    /// One side's orders that still have size, best first: the owners' book.
    pub fn resting(&self, side: Side) -> impl Iterator<Item = &BookOrder> {
        self.side(side).iter().filter(|order| order.remaining_base_amount.is_positive() && order.price.is_positive())
    }

    #[must_use]
    pub fn best_bid(&self) -> Option<Dec> {
        self.levels(Side::Bid).first().map(|level| level.price)
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Dec> {
        self.levels(Side::Ask).first().map(|level| level.price)
    }

    /// The best bid and the best ask.
    #[must_use]
    pub fn touch(&self) -> (Option<Dec>, Option<Dec>) {
        (self.best_bid(), self.best_ask())
    }

    /// Exactly halfway between the best bid and ask, one place finer than either. `None` when a
    /// side is empty.
    #[must_use]
    pub fn mid(&self) -> Option<Dec> {
        let (Some(bid), Some(ask)) = self.touch() else { return None };
        let places = bid.places().max(ask.places()).saturating_add(1);
        bid.checked_add(ask)?.checked_div(Dec::from_int(2), places, Round::Down)
    }

    /// The best ask less the best bid.
    #[must_use]
    pub fn spread(&self) -> Option<Dec> {
        let (Some(bid), Some(ask)) = self.touch() else { return None };
        ask.checked_sub(bid)
    }

    /// Whether `side` came back as long as the limit: more may rest past its worst price.
    #[must_use]
    pub fn is_cut(&self, side: Side) -> bool {
        self.limit > 0 && self.side(side).len() >= usize::try_from(self.limit).unwrap_or(usize::MAX)
    }

    /// The worst price `side` lists when it was cut, where its queue may run on; `None` for a
    /// side that was whole. Every row counts, an empty one too.
    #[must_use]
    pub fn edge(&self, side: Side) -> Option<Dec> {
        if !self.is_cut(side) {
            return None;
        }
        let prices = self.side(side).iter().map(|order| order.price);
        match side {
            Side::Bid => prices.min(),
            Side::Ask => prices.max(),
        }
    }

    /// One side's orders summed by price, best first.
    #[must_use]
    pub fn levels(&self, side: Side) -> Vec<Level> {
        let mut by_price: BTreeMap<Dec, Level> = BTreeMap::new();
        for order in self.resting(side) {
            let level = by_price.entry(order.price).or_insert(Level { price: order.price, size: Dec::ZERO, orders: 0 });
            level.size = level.size.checked_add(order.remaining_base_amount).unwrap_or(level.size);
            level.orders = level.orders.saturating_add(1);
        }
        let mut levels: Vec<Level> = by_price.into_values().collect();
        if side == Side::Bid {
            levels.reverse();
        }
        levels
    }

    /// At most `depth` levels of `side` whose size is known in full: a cut side's last level is
    /// left out, since more may rest at that price.
    #[must_use]
    pub fn whole_levels(&self, side: Side, depth: usize) -> Vec<Level> {
        let mut levels = self.levels(side);
        if self.is_cut(side) {
            levels.pop();
        }
        levels.truncate(depth);
        levels
    }

    /// The size resting on `side` at `price` or better: what a limit at `price` would take
    /// from it.
    #[must_use]
    pub fn size_to(&self, side: Side, price: Dec) -> Dec {
        let within = |level: &&Level| match side {
            Side::Bid => level.price >= price,
            Side::Ask => level.price <= price,
        };
        let levels = self.levels(side);
        Dec::checked_sum(levels.iter().filter(within).map(|level| level.size)).unwrap_or(Dec::ZERO)
    }

    /// `size` taken by a taker on `taker`'s side: a bid buys up through the asks, an ask sells
    /// down through the bids. Exact: the average is the notional over the size, and the worst is
    /// the last level reached.
    #[must_use]
    pub fn walk(&self, taker: Side, size: Dec) -> Walk {
        let mut walk = Walk { filled: Dec::ZERO, notional: Dec::ZERO, average: None, worst: None, complete: false };
        if !size.is_positive() {
            walk.complete = size.is_zero();
            return walk;
        }
        for level in self.levels(taker.opposite()) {
            let Some(left) = size.checked_sub(walk.filled).filter(|left| left.is_positive()) else { break };
            let take = left.min(level.size);
            let (Some(filled), Some(notional)) = (
                walk.filled.checked_add(take),
                take.checked_mul(level.price).and_then(|cost| walk.notional.checked_add(cost)),
            ) else {
                break;
            };
            walk.filled = filled;
            walk.notional = notional;
            walk.worst = Some(level.price);
        }
        let round = match taker {
            Side::Bid => Round::Up,
            Side::Ask => Round::Down,
        };
        walk.average = walk.notional.checked_div(walk.filled, MAX_SCALE, round);
        walk.complete = walk.filled >= size;
        walk
    }
}

/// An order a trade took, maker or taker, with its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilledOrder {
    pub order: OrderIndex,
    pub owner: AccountIndex,
    /// The trade's time, unix milliseconds.
    pub at_ms: u64,
}

/// The orders `trades` named, the ask's then the bid's of each: what tells a fill from a pull
/// between two reads of a book. A side with no venue order index is left out.
pub fn orders_filled<'a>(trades: impl IntoIterator<Item = &'a Trade>) -> Vec<FilledOrder> {
    let mut filled = Vec::new();
    for trade in trades {
        let sides = [(trade.ask_order(), trade.ask_account_id), (trade.bid_order(), trade.bid_account_id)];
        for (order, owner) in sides {
            if let Some(order) = order {
                filled.push(FilledOrder { order, owner, at_ms: trade.timestamp });
            }
        }
    }
    filled
}

/// What `orderBookOrders` asks: `limit` orders a side, from 1 to [`MOST_BOOK_ORDERS`] (clamped).
#[must_use]
pub fn order_book_orders_call(market: MarketId, limit: u32) -> Call {
    Call::get("orderBookOrders").field("market_id", market).field("limit", limit.clamp(1, MOST_BOOK_ORDERS))
}

/// A market's resting orders, `limit` a side, with the limit kept to tell a cut side.
pub async fn order_book_orders(transport: &impl Transport, market: MarketId, limit: u32) -> Result<Book, ApiError> {
    let book: Book = read(transport, order_book_orders_call(market, limit)).await?;
    Ok(Book { limit: limit.clamp(1, MOST_BOOK_ORDERS), ..book })
}

#[cfg(test)]
#[path = "tests/book.rs"]
mod tests;
