//! What the market channels send, typed: the socket's own shapes, with the market each frame is
//! about read from its channel where the frame does not say it. Trades are the
//! [`crate::markets::Trade`] every read writes. Prices and sizes are exact [`crate::utils::Dec`]s
//! read from the venue's strings; the figures it writes as bare JSON floats (volumes, candles) are
//! [`crate::utils::Figure`]s, never an `f64`.
//!
//! - [`book`]: `order_book` (a whole book on subscribe, then the levels that changed) and
//!   `ticker` (the best bid and offer).
//! - [`stats`]: `market_stats`, `spot_market_stats` and `bo_market_stats`.
//! - [`candles`]: `candle`, `mark_price_candle` and `up_down_candle`, and their resolutions.

pub mod book;
pub mod candles;
pub mod stats;

pub use book::{BookUpdate, Level, Ticker};
pub use candles::{
    Candle, Candles, MarkCandle, MarkPriceCandles, Resolution, UpDownCandle, UpDownCandles, UpDownResolution,
};
pub use stats::{BoMarketStats, BoStatus, MarketStats, SpotMarketStats};

use crate::markets::Trade;
use crate::utils::MarketId;

/// `trade`: a market's new trades, liquidations apart. On subscribe, the last ones the venue
/// kept, which are history; after, each batch as it executes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trades {
    /// From the channel.
    pub market: MarketId,
    /// Two orders crossing, newest first as sent.
    pub trades: Vec<Trade>,
    /// Liquidation orders filled against the book, newest first as sent.
    pub liquidations: Vec<Trade>,
    /// The matching engine's nonce after the last of them.
    pub nonce: u64,
}

impl Trades {
    /// Trades and liquidations alike, trades first.
    pub fn all(&self) -> impl Iterator<Item = &Trade> {
        self.trades.iter().chain(&self.liquidations)
    }
}

/// `height`: the chain's newest block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Height {
    pub height: u64,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}

#[cfg(test)]
#[path = "tests/market.rs"]
mod tests;
