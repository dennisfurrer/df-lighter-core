//! Binary options, Lighter only (Robinhood Chain answers 403): `events`, each a question with its
//! observation window and the markets that trade it, and `upDownCandles`, the underlying price an
//! Up/Down market settles on. Their fee tiers are [`crate::markets::stats::fee_tiers`] with
//! [`crate::markets::stats::FeeFilter::BinaryOptions`].
//!
//! A market is one order book per window, named `BTC-UPDOWN-15M-1792917900` (the window's start in
//! unix seconds). Its price is one Up share's in the collateral, `0.01` to `0.99`, and a winning
//! share pays `settlement_cap` ticks (`100`, 1.00). Binary options are left out of
//! `orderBookDetails`; their grid, minimums and window are here. A market id is never reused, and
//! a settled market stays readable with its outcome. Every time here is unix milliseconds but
//! `upDownCandles`' seconds.

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::books::MarketStatus;
use crate::markets::listed;
use crate::utils::dec::Dec;
use crate::utils::figure::Figure;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, AssetId, Grid, MarketId, MarketKind};

/// How a binary option settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Outcome {
    /// Not settled, or settled at the default price because no outcome could be found.
    #[default]
    Undetermined,
    /// No, or Down: the Down shares pay.
    No,
    /// Yes, or Up: the Up shares pay.
    Yes,
    /// A code this build does not know.
    Other(u8),
}

impl Outcome {
    #[must_use]
    pub const fn from_code(code: u8) -> Outcome {
        match code {
            0 => Outcome::Undetermined,
            1 => Outcome::No,
            2 => Outcome::Yes,
            other => Outcome::Other(other),
        }
    }

    /// `0` none, `1` No/Down, `2` Yes/Up.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Outcome::Undetermined => 0,
            Outcome::No => 1,
            Outcome::Yes => 2,
            Outcome::Other(code) => code,
        }
    }
}

/// From the venue's code.
impl<'de> Deserialize<'de> for Outcome {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Outcome::from_code(u8::deserialize(deserializer)?))
    }
}

/// A binary option market's live statistics.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BinaryStats {
    pub symbol: String,
    pub market_id: MarketId,
    /// Prices of one Up share: `0.00` where there is none.
    pub mid_price: Dec,
    pub best_ask_price: Dec,
    pub best_bid_price: Dec,
    pub last_trade_price: Dec,
    /// In shares.
    pub daily_base_token_volume: Figure,
    pub daily_quote_token_volume: Figure,
    pub daily_price_low: Figure,
    pub daily_price_high: Figure,
    pub daily_price_change: Figure,
    pub lifetime_quote_token_volume: Figure,
    /// Shares traded over the market's life.
    pub lifetime_base_token_volume: Figure,
    /// Matched Up and Down pairs outstanding, each once.
    pub open_interest: Dec,
    pub status: MarketStatus,
    pub outcome: Outcome,
}

/// One binary option market, as an event lists it. Fees are the most a trade pays, in percent of
/// the notional, before the account's tier and the price's discount.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BinaryMarket {
    pub symbol: String,
    pub market_id: MarketId,
    /// `binary_options`.
    #[serde(default, deserialize_with = "kind")]
    pub market_type: Option<MarketKind>,
    /// The collateral, both.
    pub base_asset_id: AssetId,
    pub quote_asset_id: AssetId,
    pub status: MarketStatus,
    pub taker_fee: Dec,
    pub maker_fee: Dec,
    #[serde(default)]
    pub is_taker_fee_enabled: bool,
    #[serde(default)]
    pub is_maker_fee_enabled: bool,
    pub liquidation_fee: Dec,
    /// The least shares an order may have.
    pub min_base_amount: Dec,
    pub min_quote_amount: Dec,
    pub order_quote_limit: Dec,
    pub supported_size_decimals: u32,
    pub supported_price_decimals: u32,
    pub supported_quote_decimals: u32,
    /// The decimals an order's shares and price are signed at: 4 and 2 for Up/Down.
    pub size_decimals: u32,
    pub price_decimals: u32,
    #[serde(deserialize_with = "whole")]
    pub created_at: u64,
    pub multiplier: Dec,
    /// The trading window, which may open before the event's observation window; a 0 start
    /// trades from listing. Creates and modifies outside it are refused.
    #[serde(deserialize_with = "whole")]
    pub start_timestamp: u64,
    #[serde(deserialize_with = "whole")]
    pub end_timestamp: u64,
    /// Matched pairs outstanding.
    pub open_interest: Dec,
    /// Set at settlement.
    pub outcome: Outcome,
    /// The account that creates and settles it.
    pub operator_account_index: AccountIndex,
    /// A winning share's payout in price ticks: `100` is 1.00. An order's price is below it.
    pub settlement_cap: u32,
    /// 0 discrete (Up/Down), 1 continuous.
    pub settlement_type: u8,
    /// Set at settlement, in price ticks.
    pub settlement_price: u32,
    /// What each side is paid per share, in ticks, when no outcome can be found: 50 is 0.50.
    pub default_price: u32,
    /// The operator froze the book: creates and modifies are refused.
    pub is_frozen: bool,
    #[serde(default)]
    pub market_deployer: Option<String>,
    #[serde(default)]
    pub stats: Option<BinaryStats>,
}

impl BinaryMarket {
    #[must_use]
    pub const fn grid(&self) -> Grid {
        Grid::new(self.size_decimals, self.price_decimals)
    }

    /// Whether an order may be created at `now_ms`: active, not frozen, inside its window.
    #[must_use]
    pub fn is_open(&self, now_ms: u64) -> bool {
        self.status == MarketStatus::Active
            && !self.is_frozen
            && self.start_timestamp <= now_ms
            && (self.end_timestamp == 0 || now_ms <= self.end_timestamp)
    }

    /// A winning share's payout, in the collateral: `settlement_cap` at the price's decimals.
    #[must_use]
    pub fn payout(&self) -> Dec {
        self.grid().price_of(self.settlement_cap)
    }

    /// The settlement price, in the collateral; `None` before settlement.
    #[must_use]
    pub fn settled_at(&self) -> Option<Dec> {
        (self.status == MarketStatus::Settled).then(|| self.grid().price_of(self.settlement_price))
    }
}

/// Free-form series data an event carries; Up/Down series write every field here.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct EventMetadata {
    /// `updown`.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// The underlying, lowercase: `btc`.
    #[serde(default)]
    pub symbol: Option<String>,
    /// The window: `15m`.
    #[serde(default)]
    pub duration: Option<String>,
    /// The price source: `chainlink`.
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub feed_id: Option<String>,
    /// The event's market.
    #[serde(default)]
    pub market_id: Option<MarketId>,
    /// The perp on the underlying: 1 for BTC.
    #[serde(default)]
    pub underlying_market_id: Option<MarketId>,
}

/// Whether an event happens once or repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    OneOff,
    Recurrent,
    #[serde(other)]
    Other,
}

impl EventKind {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            EventKind::OneOff => "one_off",
            EventKind::Recurrent => "recurrent",
            EventKind::Other => "",
        }
    }
}

/// One event: a question, the window it is measured over, and its markets.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Event {
    #[serde(deserialize_with = "whole")]
    pub event_id: u64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub creator_account_index: AccountIndex,
    #[serde(deserialize_with = "whole")]
    pub created_at: u64,
    /// The series: `btc-updown-15m`.
    pub tag: String,
    pub event_kind: EventKind,
    #[serde(default, deserialize_with = "whole")]
    pub recurrence_period_seconds: u64,
    /// The observation window.
    #[serde(deserialize_with = "whole")]
    pub start_at: u64,
    #[serde(deserialize_with = "whole")]
    pub end_at: u64,
    #[serde(default)]
    pub metadata: EventMetadata,
    #[serde(default, deserialize_with = "listed")]
    pub order_books: Vec<BinaryMarket>,
    pub lifetime_quote_token_volume: Figure,
}

/// One page of `events`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct EventPage {
    #[serde(default, deserialize_with = "listed")]
    pub events: Vec<Event>,
    /// The event the next page starts after; `None` after the last.
    #[serde(default)]
    pub next_cursor: Option<u64>,
}

/// How `events` orders its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EventSort {
    #[default]
    Id,
    StartAt,
}

/// One `events` read. Every filter is optional; ids by event and by market exclude each other.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventQuery {
    /// At most 50.
    pub event_ids: Vec<u64>,
    /// At most 50: each market's event, once.
    pub market_ids: Vec<MarketId>,
    pub creator: Option<AccountIndex>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<u64>,
    /// `Some(true)`: an outcome still trades. `Some(false)`: scheduled and closed events.
    pub active: Option<bool>,
    /// 1 to 100.
    pub limit: Option<u32>,
    pub tag: Option<String>,
    pub event_kind: Option<EventKind>,
    /// Observation windows starting at or after this, unix milliseconds.
    pub start_from: Option<u64>,
    /// Starting before this.
    pub start_before: Option<u64>,
    pub sort: EventSort,
    /// Oldest first; the venue's default is newest.
    pub ascending: bool,
}

impl EventQuery {
    /// What the venue's own discovery asks: windows still trading, soonest first.
    #[must_use]
    pub fn tradable(limit: u32) -> EventQuery {
        EventQuery {
            active: Some(true),
            limit: Some(limit),
            sort: EventSort::StartAt,
            ascending: true,
            ..EventQuery::default()
        }
    }

    /// One series by its tag.
    #[must_use]
    pub fn series(self, tag: impl Into<String>) -> EventQuery {
        EventQuery { tag: Some(tag.into()), ..self }
    }

    /// The events these markets belong to.
    #[must_use]
    pub fn of_markets(markets: &[MarketId]) -> EventQuery {
        EventQuery { market_ids: markets.to_vec(), ..EventQuery::default() }
    }

    /// The page after `cursor`.
    #[must_use]
    pub fn after(self, cursor: u64) -> EventQuery {
        EventQuery { cursor: Some(cursor), ..self }
    }
}

fn joined<T: ToString>(ids: &[T]) -> Option<String> {
    (!ids.is_empty()).then(|| ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(","))
}

/// What `events` asks.
#[must_use]
pub fn events_call(query: &EventQuery) -> Call {
    Call::get("events")
        .maybe("event_ids", joined(&query.event_ids))
        .maybe("market_ids", joined(&query.market_ids))
        .maybe("creator_account_index", query.creator)
        .maybe("cursor", query.cursor)
        .maybe("active", query.active)
        .maybe("limit", query.limit.map(|limit| limit.clamp(1, 100)))
        .maybe("tag", query.tag.as_deref())
        .maybe("event_kind", query.event_kind.map(EventKind::wire))
        .maybe("start_from", query.start_from)
        .maybe("start_before", query.start_before)
        .maybe("sort", (query.sort == EventSort::StartAt).then_some("start_at"))
        .maybe("order", query.ascending.then_some("asc"))
}

/// One page of events.
pub async fn events(transport: &impl Transport, query: &EventQuery) -> Result<EventPage, ApiError> {
    read(transport, events_call(query)).await
}

/// The underlying's candle width: each observation, or thirty seconds. The `up_down_candle`
/// channel takes the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum UpDownResolution {
    #[default]
    Second,
    ThirtySeconds,
}

impl UpDownResolution {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            UpDownResolution::Second => "1s",
            UpDownResolution::ThirtySeconds => "30s",
        }
    }

    /// Only the venue's exact spellings.
    #[must_use]
    pub fn parse(text: &str) -> Option<UpDownResolution> {
        [UpDownResolution::Second, UpDownResolution::ThirtySeconds]
            .into_iter()
            .find(|resolution| resolution.wire() == text)
    }
}

impl std::fmt::Display for UpDownResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.wire())
    }
}

/// The most candles one `upDownCandles` read answers.
pub const MOST_UP_DOWN_CANDLES: u32 = 3_600;

/// One candle of an Up/Down series' underlying price, exact as the provider wrote it (eighteen
/// places).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpDownCandle {
    pub symbol: String,
    pub duration: String,
    pub resolution: String,
    /// The bucket's start, unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    pub open: Dec,
    pub high: Dec,
    pub low: Dec,
    pub close: Dec,
    /// The feed of its last observation.
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub feed_id: String,
}

/// One page of `upDownCandles`, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct UpDownPage {
    #[serde(default, deserialize_with = "listed")]
    pub candles: Vec<UpDownCandle>,
    /// The next page's `start_timestamp`, unix seconds; 0 when the range is done.
    #[serde(default, deserialize_with = "whole")]
    pub next_cursor: u64,
}

impl UpDownPage {
    /// Where the next page starts, when there is one.
    #[must_use]
    pub const fn next(&self) -> Option<u64> {
        match self.next_cursor {
            0 => None,
            cursor => Some(cursor),
        }
    }
}

/// What `upDownCandles` asks: a series by its underlying and window (`btc`, `15m`, from an
/// event's metadata), over at most 24 hours in unix seconds, both ends inclusive.
#[must_use]
pub fn up_down_candles_call(
    symbol: &str,
    duration: &str,
    resolution: UpDownResolution,
    start_s: u64,
    end_s: u64,
    limit: Option<u32>,
) -> Call {
    Call::get("upDownCandles")
        .field("symbol", symbol)
        .field("duration", duration)
        .field("resolution", resolution.wire())
        .field("start_timestamp", start_s)
        .field("end_timestamp", end_s)
        .maybe("limit", limit.map(|limit| limit.clamp(1, MOST_UP_DOWN_CANDLES)))
}

/// One page of a series' underlying candles; page on with [`UpDownPage::next`] as the start.
pub async fn up_down_candles(
    transport: &impl Transport,
    symbol: &str,
    duration: &str,
    resolution: UpDownResolution,
    start_s: u64,
    end_s: u64,
    limit: Option<u32>,
) -> Result<UpDownPage, ApiError> {
    read(transport, up_down_candles_call(symbol, duration, resolution, start_s, end_s, limit)).await
}

fn kind<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<MarketKind>, D::Error> {
    crate::utils::order::known(deserializer)
}

#[cfg(test)]
#[path = "tests/binary.rs"]
mod tests;
