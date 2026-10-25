//! Live state kept from the websocket, sans-IO: hand the [`Store`] each frame and read what it
//! holds. A book is kept from its snapshot and updates with its nonce chain checked ([`Book`]);
//! tickers, market figures, a tape of recent trades and the chain's height as last told; each
//! account followed as its channels tell it ([`Account`]). Every piece says when it was last told
//! and last changed ([`Stamped`]), in the milliseconds handed in.
//!
//! ```
//! use df_lighter_core::live::store::{Folded, Store};
//! use df_lighter_core::live::{Session, Subscription};
//! use df_lighter_core::utils::{Dec, MarketId};
//! use df_lighter_core::Network;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let btc = MarketId::new(1)?;
//! let mut session = Session::new(Network::Lighter);
//! let mut store = Store::new();
//! session.subscribe(Subscription::OrderBook { market: btc })?;
//! // The socket opens: the session subscribes to everything wanted.
//! assert_eq!(session.opened(0).len(), 1);
//!
//! let snapshot = r#"{"channel":"order_book:1","order_book":{"asks":[{"price":"101","size":"2"}],
//!     "bids":[{"price":"100","size":"1"}],"nonce":7,"begin_nonce":0},"timestamp":1,
//!     "type":"subscribed/order_book"}"#;
//! let frame = session.received(1_000, snapshot)?;
//! assert_eq!(store.apply(1_000, &frame), Folded::Changed);
//! assert_eq!(store.mid(btc), Some(Dec::parse("100.5")?));
//!
//! // An update that does not follow nonce 7: the book empties, and the session owes the venue
//! // the resubscribe that brings a fresh snapshot.
//! let lost = snapshot.replace(r#""begin_nonce":0"#, r#""begin_nonce":9"#).replace("subscribed/", "update/");
//! let frame = session.received(2_000, &lost)?;
//! assert_eq!(store.apply(2_000, &frame), Folded::Changed);
//! assert_eq!(store.mid(btc), None);
//! assert_eq!(session.owed().len(), 2);
//! # Ok(())
//! # }
//! ```
//!
//! - [`book`]: one market's [`Book`].
//! - [`account`]: one account's [`Account`].
//! - [`stamp`]: when each piece was told.

pub mod account;
pub mod book;
pub mod stamp;

pub use account::{Account, KEEP};
pub use book::{Book, DEFAULT_DEPTH};
pub use stamp::Stamped;

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};

use crate::live::store::stamp::put;
use crate::live::{BoMarketStats, Data, Frame, Height, MarketStats, SpotMarketStats, Subscription, Ticker};
use crate::markets::Trade;
use crate::utils::{AccountIndex, Dec, MarketId};

/// Trades kept a market, newest first.
pub const TAPE: usize = 200;

/// What a frame did to the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folded {
    /// Something a view reads changed.
    Changed,
    /// Told again, and nothing changed but when it was heard.
    Same,
    /// Not kept: an update to something not held, a channel not kept, or not state.
    Dropped,
}

impl Folded {
    pub(crate) const fn of(changed: bool) -> Folded {
        match changed {
            true => Folded::Changed,
            false => Folded::Same,
        }
    }
}

/// One network's live state. One store is one network: market and account ids are each venue's
/// own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Store {
    books: BTreeMap<MarketId, Stamped<Book>>,
    tickers: BTreeMap<MarketId, Stamped<Ticker>>,
    perps: BTreeMap<MarketId, Stamped<MarketStats>>,
    spots: BTreeMap<MarketId, Stamped<SpotMarketStats>>,
    options: BTreeMap<MarketId, Stamped<BoMarketStats>>,
    tapes: BTreeMap<MarketId, Stamped<Vec<Trade>>>,
    height: Option<Stamped<Height>>,
    accounts: BTreeMap<AccountIndex, Account>,
}

impl Store {
    #[must_use]
    pub fn new() -> Store {
        Store::default()
    }

    /// A frame from the socket, received at `now` milliseconds. A book's snapshot starts it; an
    /// update to a book not held, or one not ready, is dropped; a gap empties the book until its
    /// next snapshot.
    pub fn apply(&mut self, now: u64, frame: &Frame) -> Folded {
        if let Frame::Gap(gap) = frame {
            let Some(book) = self.books.get_mut(&gap.market) else { return Folded::Dropped };
            book.value.reset();
            book.touch(now, true);
            return Folded::Changed;
        }
        let Some(data) = frame.data() else { return Folded::Dropped };
        let whole = frame.is_snapshot();
        match data {
            Data::OrderBook(update) if whole => {
                let book =
                    self.books.entry(update.market).or_insert_with(|| Stamped::new(now, Book::new(update.market)));
                book.value.snapshot(update);
                book.touch(now, true);
                Folded::Changed
            }
            Data::OrderBook(update) => {
                let Some(book) = self.books.get_mut(&update.market) else { return Folded::Dropped };
                match book.value.update(update) {
                    Ok(true) | Err(_) => {
                        book.touch(now, true);
                        Folded::Changed
                    }
                    Ok(false) => Folded::Dropped,
                }
            }
            Data::Ticker(ticker) => fold(&mut self.tickers, now, ticker.market, ticker),
            Data::MarketStats(rows) => {
                rows.iter().map(|row| fold(&mut self.perps, now, row.market_id, row)).fold(Folded::Dropped, or)
            }
            Data::SpotMarketStats(rows) => {
                rows.iter().map(|row| fold(&mut self.spots, now, row.market_id, row)).fold(Folded::Dropped, or)
            }
            Data::BoMarketStats(rows) => {
                rows.iter().map(|row| fold(&mut self.options, now, row.market_id, row)).fold(Folded::Dropped, or)
            }
            Data::Trades(trades) => {
                let tape = self.tapes.entry(trades.market).or_insert_with(|| Stamped::new(now, Vec::new()));
                let changed = fold_prints(&mut tape.value, trades.all());
                tape.touch(now, changed);
                Folded::of(changed)
            }
            Data::Height(height) => {
                let newer = self.height.as_ref().is_none_or(|held| held.height <= height.height);
                match newer {
                    true => Folded::of(put(&mut self.height, now, *height)),
                    false => Folded::Dropped,
                }
            }
            _ => match account_of(data) {
                Some(index) => {
                    let folded = self.accounts.entry(index).or_insert_with(|| Account::new(index)).apply(now, frame);
                    if self.accounts.get(&index) == Some(&Account::new(index)) {
                        self.accounts.remove(&index);
                    }
                    folded
                }
                None => Folded::Dropped,
            },
        }
    }

    /// A market's book, when a snapshot has come.
    #[must_use]
    pub fn book(&self, market: MarketId) -> Option<&Stamped<Book>> {
        self.books.get(&market)
    }

    #[must_use]
    pub fn ticker(&self, market: MarketId) -> Option<&Stamped<Ticker>> {
        self.tickers.get(&market)
    }

    /// A perp's figures.
    #[must_use]
    pub fn market_stats(&self, market: MarketId) -> Option<&Stamped<MarketStats>> {
        self.perps.get(&market)
    }

    #[must_use]
    pub fn spot_stats(&self, market: MarketId) -> Option<&Stamped<SpotMarketStats>> {
        self.spots.get(&market)
    }

    /// A binary option's figures.
    #[must_use]
    pub fn option_stats(&self, market: MarketId) -> Option<&Stamped<BoMarketStats>> {
        self.options.get(&market)
    }

    /// A market's recent trades and liquidations, newest first, at most [`TAPE`].
    #[must_use]
    pub fn tape(&self, market: MarketId) -> Option<&Stamped<Vec<Trade>>> {
        self.tapes.get(&market)
    }

    #[must_use]
    pub const fn height(&self) -> Option<&Stamped<Height>> {
        self.height.as_ref()
    }

    #[must_use]
    pub fn account(&self, index: AccountIndex) -> Option<&Account> {
        self.accounts.get(&index)
    }

    /// A market's mid: from its book when one is ready, else its ticker, else its figures.
    #[must_use]
    pub fn mid(&self, market: MarketId) -> Option<Dec> {
        let book = self.books.get(&market).filter(|book| book.is_ready()).and_then(|book| book.mid());
        book.or_else(|| self.tickers.get(&market).and_then(|ticker| ticker.mid()))
            .or_else(|| self.perps.get(&market).and_then(|stats| stats.mid_price))
            .or_else(|| self.spots.get(&market).and_then(|stats| stats.mid_price))
            .or_else(|| self.options.get(&market).and_then(|stats| stats.mid_price))
    }

    /// Lets go of what `subscription` fed, once it is unsubscribed: a book, a ticker, a tape, an
    /// account's channels (the account itself once nothing of it is held).
    pub fn forget(&mut self, subscription: &Subscription) {
        match subscription {
            Subscription::OrderBook { market } => {
                self.books.remove(market);
            }
            Subscription::Ticker { market } => {
                self.tickers.remove(market);
            }
            Subscription::Trade { market } => {
                self.tapes.remove(market);
            }
            other => {
                if let Some(account) = other.account() {
                    self.accounts.remove(&account);
                }
            }
        }
    }

    /// The socket closed: every book waits for its next snapshot. The rest stays as last told,
    /// its stamps saying how old.
    pub fn closed(&mut self) {
        for book in self.books.values_mut() {
            book.value.reset();
        }
    }
}

/// The account a frame is about.
fn account_of(data: &Data) -> Option<AccountIndex> {
    Some(match data {
        Data::AccountAll(all) => all.account,
        Data::AccountMarket(one) => one.account,
        Data::UserStats(stats) => stats.account,
        Data::AccountTx(txs) => txs.account,
        Data::AccountAllOrders(all) => all.account,
        Data::AccountOrders(one) => one.account,
        Data::AccountAllTrades(all) => all.account,
        Data::AccountAllPositions(all) => all.account,
        Data::AccountAllAssets(all) => all.account,
        _ => return None,
    })
}

/// A value told again for `market`.
fn fold<T: Clone + PartialEq>(
    slots: &mut BTreeMap<MarketId, Stamped<T>>,
    now: u64,
    market: MarketId,
    value: &T,
) -> Folded {
    match slots.get_mut(&market) {
        Some(held) => Folded::of(held.set(now, value.clone())),
        None => {
            slots.insert(market, Stamped::new(now, value.clone()));
            Folded::Changed
        }
    }
}

/// The more of the two: a change over a repeat over nothing kept.
const fn or(one: Folded, other: Folded) -> Folded {
    match (one, other) {
        (Folded::Changed, _) | (_, Folded::Changed) => Folded::Changed,
        (Folded::Same, _) | (_, Folded::Same) => Folded::Same,
        _ => Folded::Dropped,
    }
}

/// Trades into a tape kept newest first: one already held (the same id) is not added again, and
/// the oldest go past [`TAPE`]. Says whether any was new.
fn fold_prints<'a>(tape: &mut Vec<Trade>, prints: impl Iterator<Item = &'a Trade>) -> bool {
    let mut seen: HashSet<u64> = tape.iter().map(|print| print.trade_id).collect();
    let fresh: Vec<Trade> = prints.filter(|print| seen.insert(print.trade_id)).cloned().collect();
    if fresh.is_empty() {
        return false;
    }
    tape.extend(fresh);
    tape.sort_by_key(|print| Reverse((print.timestamp, print.trade_id)));
    tape.truncate(TAPE);
    true
}

#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;
