//! Candles: a market's trades (`candles`), its mark price (`markPriceCandles`), and every market's
//! last 24 hourly prices (`marketPriceCharts`). Times are unix milliseconds; each read answers the
//! newest [`CANDLES_PER_CALL`] inside its window, oldest first, so a long range is read in pages
//! back from its end ([`candle_window`], [`candle_history`]).
//!
//! The venue writes a candle's figures as bare JSON floats ([`Figure`]) and leaves out a field
//! whose value is zero. [`Candle::bar`] reads one as decimals the way a chart draws it. A binary
//! option's book has no candles: its underlying's are in [`crate::markets::binary`].

use std::collections::BTreeMap;
use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, de};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::markets::stats::stat_value;
use crate::utils::MarketId;
use crate::utils::dec::Dec;
use crate::utils::figure::Figure;
use crate::utils::ids::whole;
use crate::utils::time::{DAY_MS, HOUR_MS, MINUTE_MS};

/// The most candles one read answers.
pub const CANDLES_PER_CALL: u32 = 500;

/// A candle's width: every resolution the venue serves, over REST and the stream alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resolution {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    FourHours,
    TwelveHours,
    OneDay,
}

impl Resolution {
    /// Finest first.
    pub const ALL: [Resolution; 8] = [
        Resolution::OneMinute,
        Resolution::FiveMinutes,
        Resolution::FifteenMinutes,
        Resolution::ThirtyMinutes,
        Resolution::OneHour,
        Resolution::FourHours,
        Resolution::TwelveHours,
        Resolution::OneDay,
    ];

    /// As the venue spells it.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Resolution::OneMinute => "1m",
            Resolution::FiveMinutes => "5m",
            Resolution::FifteenMinutes => "15m",
            Resolution::ThirtyMinutes => "30m",
            Resolution::OneHour => "1h",
            Resolution::FourHours => "4h",
            Resolution::TwelveHours => "12h",
            Resolution::OneDay => "1d",
        }
    }

    /// Only the venue's exact spellings.
    #[must_use]
    pub fn parse(text: &str) -> Option<Resolution> {
        Resolution::ALL.into_iter().find(|resolution| resolution.wire() == text)
    }

    /// Its width in milliseconds.
    #[must_use]
    pub const fn ms(self) -> u64 {
        match self {
            Resolution::OneMinute => MINUTE_MS,
            Resolution::FiveMinutes => 5 * MINUTE_MS,
            Resolution::FifteenMinutes => 15 * MINUTE_MS,
            Resolution::ThirtyMinutes => 30 * MINUTE_MS,
            Resolution::OneHour => HOUR_MS,
            Resolution::FourHours => 4 * HOUR_MS,
            Resolution::TwelveHours => 12 * HOUR_MS,
            Resolution::OneDay => DAY_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a resolution; the venue takes 1m 5m 15m 30m 1h 4h 12h 1d")]
pub struct UnknownResolution(pub String);

impl FromStr for Resolution {
    type Err = UnknownResolution;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Resolution::parse(text).ok_or_else(|| UnknownResolution(text.to_owned()))
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

impl<'de> Deserialize<'de> for Resolution {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// One trade candle as the venue writes it; a field it leaves out was zero.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Candle {
    /// Open time, unix milliseconds.
    #[serde(rename = "t", deserialize_with = "whole")]
    pub open_time: u64,
    #[serde(rename = "o", default)]
    pub open: Option<Figure>,
    #[serde(rename = "h", default)]
    pub high: Option<Figure>,
    #[serde(rename = "l", default)]
    pub low: Option<Figure>,
    #[serde(rename = "c", default)]
    pub close: Option<Figure>,
    /// In the base asset.
    #[serde(rename = "v", default)]
    pub volume: Option<Figure>,
    /// In the collateral.
    #[serde(rename = "V", default)]
    pub quote_volume: Option<Figure>,
    /// The last trade inside it.
    #[serde(rename = "i", default, deserialize_with = "whole")]
    pub last_trade_id: u64,
    /// The prices before the mark was merged in, when `merge_mark_price` asked for it.
    #[serde(rename = "O", default)]
    pub open_raw: Option<Figure>,
    #[serde(rename = "H", default)]
    pub high_raw: Option<Figure>,
    #[serde(rename = "L", default)]
    pub low_raw: Option<Figure>,
    #[serde(rename = "C", default)]
    pub close_raw: Option<Figure>,
}

/// A candle as decimals, every figure there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    /// Unix milliseconds.
    pub open_time: u64,
    pub open: Dec,
    pub high: Dec,
    pub low: Dec,
    pub close: Dec,
    /// In the base asset.
    pub volume: Dec,
    /// In the collateral.
    pub quote_volume: Dec,
}

impl Candle {
    /// The candle as a chart draws it: `None` without a close; a missing open is the close, a
    /// missing high or low the larger or smaller of the two, a missing volume zero. Each float is
    /// read through [`stat_value`].
    #[must_use]
    pub fn bar(&self) -> Option<Bar> {
        let value = |figure: &Option<Figure>| figure.as_ref().and_then(stat_value);
        let close = value(&self.close)?;
        let open = value(&self.open).unwrap_or(close);
        Some(Bar {
            open_time: self.open_time,
            open,
            high: value(&self.high).unwrap_or(open.max(close)),
            low: value(&self.low).unwrap_or(open.min(close)),
            close,
            volume: value(&self.volume).unwrap_or(Dec::ZERO),
            quote_volume: value(&self.quote_volume).unwrap_or(Dec::ZERO),
        })
    }
}

/// One mark price candle.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarkCandle {
    /// Open time, unix milliseconds.
    #[serde(rename = "t", deserialize_with = "whole")]
    pub open_time: u64,
    #[serde(rename = "o")]
    pub open: Figure,
    #[serde(rename = "h")]
    pub high: Figure,
    #[serde(rename = "l")]
    pub low: Figure,
    #[serde(rename = "c")]
    pub close: Figure,
    /// How many mark prices it was made of.
    #[serde(rename = "sc", default, deserialize_with = "whole")]
    pub samples: u64,
}

/// One market's last 24 hourly prices, oldest first: the mark for a perp, the index for a spot
/// pair.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PriceChart {
    pub market_id: MarketId,
    #[serde(default, deserialize_with = "listed")]
    pub prices: Vec<Dec>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "C: Deserialize<'de>"))]
struct Candles<C> {
    #[serde(default, deserialize_with = "listed")]
    c: Vec<C>,
}

#[derive(Deserialize)]
struct PriceCharts {
    #[serde(default, deserialize_with = "listed")]
    price_charts: Vec<PriceChart>,
}

/// A window of candles to ask for: from `start_ms` to `end_ms`, both inclusive, and how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandleWindow {
    pub start_ms: u64,
    pub end_ms: u64,
    /// `count_back`: at most [`CANDLES_PER_CALL`].
    pub count: u32,
}

/// One `candles` read: the window, and two switches the venue offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandleQuery {
    pub market: MarketId,
    pub resolution: Resolution,
    pub window: CandleWindow,
    /// Stamp each candle with its close time rather than its open.
    pub set_timestamp_to_end: bool,
    /// Fold the mark price into the prices; the trades' own come back as `O` `H` `L` `C`.
    pub merge_mark_price: bool,
}

impl CandleQuery {
    #[must_use]
    pub const fn new(market: MarketId, resolution: Resolution, window: CandleWindow) -> CandleQuery {
        CandleQuery { market, resolution, window, set_timestamp_to_end: false, merge_mark_price: false }
    }

    #[must_use]
    pub const fn stamped_at_end(self) -> CandleQuery {
        CandleQuery { set_timestamp_to_end: true, ..self }
    }

    #[must_use]
    pub const fn with_mark_price(self) -> CandleQuery {
        CandleQuery { merge_mark_price: true, ..self }
    }
}

/// What `candles` asks. A switch left off is left out.
#[must_use]
pub fn candles_call(query: &CandleQuery) -> Call {
    window_call("candles", query.market, query.resolution, query.window)
        .maybe("set_timestamp_to_end", query.set_timestamp_to_end.then_some(true))
        .maybe("merge_mark_price", query.merge_mark_price.then_some(true))
}

/// One window's trade candles, oldest first.
pub async fn candles(transport: &impl Transport, query: &CandleQuery) -> Result<Vec<Candle>, ApiError> {
    Ok(read::<Candles<Candle>>(transport, candles_call(query)).await?.c)
}

/// What `markPriceCandles` asks.
#[must_use]
pub fn mark_price_candles_call(market: MarketId, resolution: Resolution, window: CandleWindow) -> Call {
    window_call("markPriceCandles", market, resolution, window)
}

/// One window's mark price candles, oldest first.
pub async fn mark_price_candles(
    transport: &impl Transport,
    market: MarketId,
    resolution: Resolution,
    window: CandleWindow,
) -> Result<Vec<MarkCandle>, ApiError> {
    Ok(read::<Candles<MarkCandle>>(transport, mark_price_candles_call(market, resolution, window)).await?.c)
}

fn window_call(path: &'static str, market: MarketId, resolution: Resolution, window: CandleWindow) -> Call {
    Call::get(path)
        .field("market_id", market)
        .field("resolution", resolution.wire())
        .field("start_timestamp", window.start_ms)
        .field("end_timestamp", window.end_ms)
        .field("count_back", window.count.min(CANDLES_PER_CALL))
}

/// What `marketPriceCharts` asks: `market_ids` once per market; none asks for every market.
#[must_use]
pub fn market_price_charts_call(markets: &[MarketId]) -> Call {
    markets.iter().fold(Call::get("marketPriceCharts"), |call, market| call.field("market_ids", market))
}

/// The last 24 hourly prices of `markets`, or of every market.
pub async fn market_price_charts(
    transport: &impl Transport,
    markets: &[MarketId],
) -> Result<Vec<PriceChart>, ApiError> {
    Ok(read::<PriceCharts>(transport, market_price_charts_call(markets)).await?.price_charts)
}

/// The newest window inside `start_ms..=end_ms`: the candles opening in it, back from the last
/// to open by `end_ms`, at most [`CANDLES_PER_CALL`]. Candles open on whole multiples of their
/// width since the epoch (a day at 00:00 UTC). `None` when no candle opens in the range.
#[must_use]
pub fn candle_window(start_ms: u64, end_ms: u64, resolution: Resolution) -> Option<CandleWindow> {
    let step = resolution.ms();
    let newest = end_ms / step * step;
    let first = start_ms.div_ceil(step).checked_mul(step)?;
    if newest < first {
        return None;
    }
    let count = ((newest - first) / step).saturating_add(1).min(u64::from(CANDLES_PER_CALL));
    let start = newest - (count - 1) * step;
    Some(CandleWindow { start_ms: start, end_ms, count: u32::try_from(count).unwrap_or(CANDLES_PER_CALL) })
}

/// Whether a page reached back to its window's start: if not, the market's history begins inside
/// it and an older window would come back empty.
#[must_use]
pub fn reaches_start(page: &[Candle], window: CandleWindow, resolution: Resolution) -> bool {
    let full = page.len() >= usize::try_from(window.count).unwrap_or(usize::MAX);
    let oldest = page.iter().map(|candle| candle.open_time).min();
    full && oldest.is_some_and(|oldest| oldest <= window.start_ms.saturating_add(resolution.ms()))
}

/// Pages of candles as one series: those opening inside `start_ms..=end_ms`, oldest first, one
/// per open time (a later page's wins).
#[must_use]
pub fn merge_candles(start_ms: u64, end_ms: u64, pages: Vec<Vec<Candle>>) -> Vec<Candle> {
    let mut by_time: BTreeMap<u64, Candle> = BTreeMap::new();
    for candle in pages.into_iter().flatten() {
        if (start_ms..=end_ms).contains(&candle.open_time) {
            by_time.insert(candle.open_time, candle);
        }
    }
    by_time.into_values().collect()
}

/// A market's trade candles over a range of any length, read back from its end in windows of
/// [`CANDLES_PER_CALL`], at most `max_calls` reads (four is 2,000 bars), stopping where the
/// market's history begins. Oldest first.
pub async fn candle_history(
    transport: &impl Transport,
    market: MarketId,
    resolution: Resolution,
    start_ms: u64,
    end_ms: u64,
    max_calls: u32,
) -> Result<Vec<Candle>, ApiError> {
    let mut pages = Vec::new();
    let mut end = end_ms;
    for _ in 0..max_calls {
        let Some(window) = candle_window(start_ms, end, resolution) else { break };
        let page = candles(transport, &CandleQuery::new(market, resolution, window)).await?;
        let more = reaches_start(&page, window, resolution) && window.start_ms > start_ms;
        pages.push(page);
        match (more, window.start_ms.checked_sub(1)) {
            (true, Some(before)) => end = before,
            _ => break,
        }
    }
    Ok(merge_candles(start_ms, end_ms, pages))
}

#[cfg(test)]
#[path = "tests/candles.rs"]
mod tests;
