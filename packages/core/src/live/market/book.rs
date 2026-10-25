//! `order_book` and `ticker`. An order book frame carries each level that changed with its new
//! absolute size, a size of zero removing it; the one on subscribe carries every level. Its
//! `begin_nonce` is the `nonce` of the frame before it, which is how a lost frame shows
//! ([`crate::live::store::Book`] keeps a book and checks the chain).

use serde::{Deserialize, Deserializer};

use crate::live::frame::fields::blank;
use crate::utils::{Dec, MarketId};

/// One price level: what rests at `price`, in the base asset. Not [`crate::markets::Level`]: the
/// socket sends no count of the orders resting there, and in an update a size of zero means the
/// level is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Level {
    pub price: Dec,
    /// Zero in an update: the level is gone.
    pub size: Dec,
}

/// `order_book`: a whole book on subscribe, then every 50 ms the levels that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookUpdate {
    /// From the channel.
    pub market: MarketId,
    /// Lowest first on subscribe; in an update, as sent.
    pub asks: Vec<Level>,
    /// Highest first on subscribe; in an update, as sent.
    pub bids: Vec<Level>,
    /// The matching engine's nonce this frame brings the book to.
    pub nonce: u64,
    /// The nonce the book was at before this frame: the previous frame's `nonce`. Zero on
    /// subscribe.
    pub begin_nonce: u64,
    /// The API server's count, rising on each frame but not by one, and starting elsewhere on
    /// another server after a reconnect. Not a check of anything.
    pub offset: u64,
    /// Unix microseconds, when the book last changed.
    pub last_updated_at: u64,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}

/// `ticker`: the best bid and offer, sent on every nonce that moves the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticker {
    /// From the channel.
    pub market: MarketId,
    /// `ETH`, `ETH/USDC`.
    pub symbol: String,
    /// `None` with nothing resting on that side.
    pub ask: Option<Level>,
    pub bid: Option<Level>,
    pub nonce: u64,
    /// Unix microseconds.
    pub last_updated_at: u64,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}

impl Ticker {
    /// Halfway between the best bid and offer, exactly; `None` with a side empty.
    #[must_use]
    pub fn mid(&self) -> Option<Dec> {
        halfway(self.bid?.price, self.ask?.price)
    }
}

/// `(a + b) / 2`, exact: one place more than the finer of the two.
pub(crate) fn halfway(a: Dec, b: Dec) -> Option<Dec> {
    a.checked_add(b)?.checked_mul(Dec::lit::<1>(5))
}

/// A side of the ticker: a level, or nothing (missing, `null`, or a blank price).
pub(crate) fn side<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Level>, D::Error> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default, deserialize_with = "blank")]
        price: Option<Dec>,
        #[serde(default, deserialize_with = "blank")]
        size: Option<Dec>,
    }
    let raw: Option<Raw> = Option::deserialize(deserializer)?;
    Ok(raw.and_then(|raw| Some(Level { price: raw.price?, size: raw.size.unwrap_or(Dec::ZERO) })))
}
