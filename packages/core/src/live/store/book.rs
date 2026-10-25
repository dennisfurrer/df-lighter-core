//! One market's order book, kept from its snapshot and the updates after it ([`Book`]), sans-IO:
//! hand it each `order_book` frame. An update carries each changed level's new absolute size,
//! zero removing it, and its `begin_nonce` must be the nonce the book is at. One that is not means
//! a lost update, and a book patched past one is wrong in ways nothing downstream can see: the
//! book empties, says so ([`Gap`]), and waits for the fresh snapshot a resubscribe brings
//! ([`crate::live::resubscribe`]). Prices compare by value, so `84492.0` and `84492.00` are one
//! level.

use std::collections::BTreeMap;

use crate::live::market::book::halfway;
use crate::live::{BookUpdate, Data, Frame, Gap, Level};
use crate::utils::{Dec, MarketId};

/// Levels a side a book view gives unless asked for another depth, as an `l2Book` and globe.li
/// show.
pub const DEFAULT_DEPTH: usize = 20;

/// One market's book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    market: MarketId,
    bids: BTreeMap<Dec, Dec>,
    asks: BTreeMap<Dec, Dec>,
    /// The nonce of the last frame applied; `None` until a snapshot, and after a gap.
    nonce: Option<u64>,
    last_updated_at: u64,
    timestamp: u64,
}

impl Book {
    /// An empty book for `market`, waiting for its snapshot.
    #[must_use]
    pub const fn new(market: MarketId) -> Book {
        Book { market, bids: BTreeMap::new(), asks: BTreeMap::new(), nonce: None, last_updated_at: 0, timestamp: 0 }
    }

    #[must_use]
    pub const fn market(&self) -> MarketId {
        self.market
    }

    /// Between a snapshot and the next gap.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.nonce.is_some()
    }

    /// The nonce of the last frame applied.
    #[must_use]
    pub const fn nonce(&self) -> Option<u64> {
        self.nonce
    }

    /// Unix microseconds, when the book last changed on the venue.
    #[must_use]
    pub const fn last_updated_at(&self) -> u64 {
        self.last_updated_at
    }

    /// Unix milliseconds, when the API server sent the last frame applied.
    #[must_use]
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Replaces everything with a snapshot (`subscribed/order_book`).
    pub fn snapshot(&mut self, update: &BookUpdate) {
        self.bids.clear();
        self.asks.clear();
        put(&mut self.bids, &update.bids);
        put(&mut self.asks, &update.asks);
        self.nonce = Some(update.nonce);
        self.last_updated_at = update.last_updated_at;
        self.timestamp = update.timestamp;
    }

    /// Applies an update (`update/order_book`). `Ok(true)` when applied; `Ok(false)` while
    /// waiting for a snapshot, or for another market. One that does not follow the book empties
    /// it until the next snapshot.
    pub fn update(&mut self, update: &BookUpdate) -> Result<bool, Gap> {
        let Some(expected) = self.nonce.filter(|_| update.market == self.market) else { return Ok(false) };
        if update.begin_nonce != expected {
            self.reset();
            return Err(Gap { market: self.market, expected, got: update.begin_nonce });
        }
        put(&mut self.bids, &update.bids);
        put(&mut self.asks, &update.asks);
        self.nonce = Some(update.nonce);
        self.last_updated_at = update.last_updated_at;
        self.timestamp = update.timestamp;
        Ok(true)
    }

    /// Any frame: this market's snapshot, update or gap; everything else passes by. `Ok(true)`
    /// when the book changed.
    pub fn apply(&mut self, frame: &Frame) -> Result<bool, Gap> {
        match frame {
            Frame::Snapshot(Data::OrderBook(update)) if update.market == self.market => {
                self.snapshot(update);
                Ok(true)
            }
            Frame::Update(Data::OrderBook(update)) => self.update(update),
            Frame::Gap(gap) if gap.market == self.market => {
                self.reset();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Forgets the book (a gap, a closed socket): not ready until the next snapshot.
    pub fn reset(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.nonce = None;
    }

    /// Bids, highest first.
    pub fn bids(&self) -> impl Iterator<Item = Level> + '_ {
        self.bids.iter().rev().map(|(price, size)| Level { price: *price, size: *size })
    }

    /// Asks, lowest first.
    pub fn asks(&self) -> impl Iterator<Item = Level> + '_ {
        self.asks.iter().map(|(price, size)| Level { price: *price, size: *size })
    }

    /// The best `depth` levels a side: bids highest first, asks lowest first.
    #[must_use]
    pub fn top(&self, depth: usize) -> (Vec<Level>, Vec<Level>) {
        (self.bids().take(depth).collect(), self.asks().take(depth).collect())
    }

    /// Levels a side.
    #[must_use]
    pub fn depth(&self) -> (usize, usize) {
        (self.bids.len(), self.asks.len())
    }

    #[must_use]
    pub fn best_bid(&self) -> Option<Level> {
        self.bids().next()
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Level> {
        self.asks().next()
    }

    /// Halfway between the best bid and offer, exactly; `None` with a side empty.
    #[must_use]
    pub fn mid(&self) -> Option<Dec> {
        halfway(self.best_bid()?.price, self.best_ask()?.price)
    }

    /// The best offer less the best bid.
    #[must_use]
    pub fn spread(&self) -> Option<Dec> {
        self.best_ask()?.price.checked_sub(self.best_bid()?.price)
    }
}

/// Each level's new absolute size into a side; zero, or less, removes it.
fn put(side: &mut BTreeMap<Dec, Dec>, levels: &[Level]) {
    for level in levels {
        match level.size.is_positive() {
            true => side.insert(level.price, level.size),
            false => side.remove(&level.price),
        };
    }
}

#[cfg(test)]
#[path = "tests/book.rs"]
mod tests;
