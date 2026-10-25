//! `candle`, `mark_price_candle` and `up_down_candle`. On subscribe the candle forming (an
//! Up/Down series: up to the last 300); after, the candle as it stands, batched every 500 ms, and
//! two at once when a period rolls over, the one just closed first. Trade and mark candles write
//! their prices as bare JSON floats ([`crate::utils::Figure`]); an Up/Down candle's come as decimal strings.
//! Each candle is the row the read of the same candles answers: `candles`, `markPriceCandles`
//! and `upDownCandles` ([`Candle`], [`MarkCandle`], [`UpDownCandle`]).

use crate::utils::MarketId;

pub use crate::markets::{Candle, MarkCandle, Resolution, UpDownCandle, UpDownResolution};

/// `candle`: one market's candles at one resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candles {
    /// From the channel.
    pub market: MarketId,
    pub resolution: Resolution,
    /// Oldest first.
    pub candles: Vec<Candle>,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}

/// `mark_price_candle`: one market's mark candles at one resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkPriceCandles {
    /// From the channel.
    pub market: MarketId,
    pub resolution: Resolution,
    /// Oldest first.
    pub candles: Vec<MarkCandle>,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}

/// `up_down_candle`: one series' underlying candles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpDownCandles {
    /// From the channel: the underlying, the series' window, the resolution.
    pub symbol: String,
    pub duration: String,
    pub resolution: UpDownResolution,
    /// Oldest first.
    pub candles: Vec<UpDownCandle>,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}
