//! Profit over time: the account's PnL chart (`pnl`), each funding payment on its positions
//! (`positionFunding`), and the venue's public PnL leaderboard (`pnlLeaderboard`).
//!
//! The chart's figures are the venue's floats, cumulative since the account began, kept as
//! written ([`Figure`]); a funding payment's are decimal strings.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::lenient::{list, text, zero_dec};
use crate::utils::{AccountIndex, Address, Dec, Figure, MarketId, Side};

/// A chart's step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resolution {
    Minute,
    FiveMinutes,
    FifteenMinutes,
    Hour,
    FourHours,
    Day,
}

impl Resolution {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Resolution::Minute => "1m",
            Resolution::FiveMinutes => "5m",
            Resolution::FifteenMinutes => "15m",
            Resolution::Hour => "1h",
            Resolution::FourHours => "4h",
            Resolution::Day => "1d",
        }
    }
}

/// One step of the chart, every figure cumulative, in the collateral.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PnlPoint {
    /// Unix seconds, the step's start.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    /// Perps and spot trading profit.
    #[serde(default)]
    pub trade_pnl: Option<Figure>,
    #[serde(default)]
    pub trade_spot_pnl: Option<Figure>,
    /// Money in and out of perps, of spot, of pools and of staking.
    #[serde(default)]
    pub inflow: Option<Figure>,
    #[serde(default)]
    pub outflow: Option<Figure>,
    #[serde(default)]
    pub spot_inflow: Option<Figure>,
    #[serde(default)]
    pub spot_outflow: Option<Figure>,
    #[serde(default)]
    pub pool_pnl: Option<Figure>,
    #[serde(default)]
    pub pool_inflow: Option<Figure>,
    #[serde(default)]
    pub pool_outflow: Option<Figure>,
    #[serde(default)]
    pub pool_total_shares: Option<Figure>,
    #[serde(default)]
    pub staked_lit: Option<Figure>,
    #[serde(default)]
    pub staking_pnl: Option<Figure>,
    #[serde(default)]
    pub staking_inflow: Option<Figure>,
    #[serde(default)]
    pub staking_outflow: Option<Figure>,
    #[serde(default)]
    pub volume: Option<Figure>,
}

/// What `pnl` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PnlChart {
    #[serde(default, deserialize_with = "text")]
    pub resolution: String,
    #[serde(default, deserialize_with = "list")]
    pub pnl: Vec<PnlPoint>,
}

/// What `pnl` asks: `count` steps of `resolution` ending at `to_ms`, from `from_ms` (unix
/// milliseconds). `ignore_transfers` leaves money moved between accounts out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PnlQuery {
    pub account: AccountIndex,
    pub resolution: Resolution,
    pub from_ms: u64,
    pub to_ms: u64,
    pub count: u32,
    pub ignore_transfers: bool,
}

/// What `pnl` asks for `query`, with the token: the docs list the chart among the private reads.
#[must_use]
pub fn pnl_call(query: &PnlQuery) -> Call {
    Call::get("pnl")
        .field("by", "index")
        .field("value", query.account)
        .field("resolution", query.resolution.name())
        .field("start_timestamp", query.from_ms)
        .field("end_timestamp", query.to_ms)
        .field("count_back", query.count)
        .maybe("ignore_transfers", query.ignore_transfers.then_some(true))
        .authed()
}

pub async fn pnl(transport: &impl Transport, query: &PnlQuery) -> Result<PnlChart, ApiError> {
    read(transport, pnl_call(query)).await
}

/// One funding payment on one position.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PositionFunding {
    /// Unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    pub market_id: MarketId,
    #[serde(default, deserialize_with = "whole")]
    pub funding_id: u64,
    /// What the account's collateral changed by: negative when it paid.
    #[serde(deserialize_with = "zero_dec")]
    pub change: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub discount: Dec,
    /// The hour's rate, in percent as the venue writes it.
    #[serde(default, deserialize_with = "zero_dec")]
    pub rate: Dec,
    /// The position's size when it settled.
    #[serde(default, deserialize_with = "zero_dec")]
    pub position_size: Dec,
    /// `long` or `short`.
    #[serde(default, deserialize_with = "text")]
    pub position_side: String,
}

impl PositionFunding {
    /// Bid for a long, ask for a short; `None` for a word this build does not know.
    #[must_use]
    pub fn side(&self) -> Option<Side> {
        match self.position_side.as_str() {
            "long" => Some(Side::Bid),
            "short" => Some(Side::Ask),
            _ => None,
        }
    }
}

/// A page of funding payments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FundingPage {
    #[serde(default, deserialize_with = "list")]
    pub position_fundings: Vec<PositionFunding>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// What `positionFunding` asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingQuery {
    pub account: AccountIndex,
    /// Empty for every market.
    pub markets: Vec<MarketId>,
    /// Longs or shorts only; `None` for both.
    pub side: Option<Side>,
    /// Unix milliseconds as the venue takes them.
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub cursor: Option<String>,
    /// 1 to 100.
    pub limit: u8,
}

impl FundingQuery {
    #[must_use]
    pub const fn new(account: AccountIndex) -> FundingQuery {
        FundingQuery { account, markets: Vec::new(), side: None, from: None, to: None, cursor: None, limit: 100 }
    }
}

/// What `positionFunding` asks for `query`, with the token (a master's is refused without).
#[must_use]
pub fn position_funding_call(query: &FundingQuery) -> Call {
    let markets: Vec<String> = query.markets.iter().map(ToString::to_string).collect();
    Call::get("positionFunding")
        .field("account_index", query.account)
        .maybe("cursor", query.cursor.as_deref())
        .field("limit", query.limit.clamp(1, 100))
        .maybe(
            "side",
            query.side.map(|side| match side {
                Side::Bid => "long",
                Side::Ask => "short",
            }),
        )
        .maybe("start_timestamp", query.from)
        .maybe("end_timestamp", query.to)
        .maybe("market_ids", (!markets.is_empty()).then(|| markets.join(",")))
        .authed()
}

pub async fn position_funding(transport: &impl Transport, query: &FundingQuery) -> Result<FundingPage, ApiError> {
    read(transport, position_funding_call(query)).await
}

/// The window a leaderboard ranks over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Window {
    Day,
    Week,
    Month,
    All,
}

impl Window {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Window::Day => "24h",
            Window::Week => "7d",
            Window::Month => "30d",
            Window::All => "all",
        }
    }
}

/// What a leaderboard ranks by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RankBy {
    Pnl,
    Roi,
    Volume,
    AccountValue,
}

impl RankBy {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            RankBy::Pnl => "pnl",
            RankBy::Roi => "roi",
            RankBy::Volume => "volume",
            RankBy::AccountValue => "account_value",
        }
    }
}

/// One trader on the leaderboard. The figures are the venue's floats.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeaderboardEntry {
    #[serde(deserialize_with = "whole")]
    pub rank: u64,
    pub l1_address: Address,
    pub account_value: Figure,
    pub pnl: Figure,
    pub roi: Figure,
    pub volume: Figure,
}

/// What `pnlLeaderboard` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PnlLeaderboard {
    #[serde(default, deserialize_with = "list")]
    pub entries: Vec<LeaderboardEntry>,
    #[serde(default, deserialize_with = "whole")]
    pub total: u64,
    /// Unix seconds as the venue writes it.
    #[serde(default, deserialize_with = "whole")]
    pub updated_at: u64,
}

/// What `pnlLeaderboard` asks: `limit` (1 to 100) from `offset`, best first unless `ascending`.
/// `search` narrows it to matching addresses.
#[must_use]
pub fn pnl_leaderboard_call(
    window: Window,
    rank_by: RankBy,
    ascending: bool,
    limit: u8,
    offset: u32,
    search: Option<&str>,
) -> Call {
    Call::get("pnlLeaderboard")
        .field("time_window", window.name())
        .field("sort_by", rank_by.name())
        .field(
            "sort_dir",
            match ascending {
                true => "asc",
                false => "desc",
            },
        )
        .field("limit", limit.clamp(1, 100))
        .field("offset", offset.min(100_000))
        .maybe("search", search)
}

/// The public leaderboard.
pub async fn pnl_leaderboard(
    transport: &impl Transport,
    window: Window,
    rank_by: RankBy,
    limit: u8,
    offset: u32,
) -> Result<PnlLeaderboard, ApiError> {
    read(transport, pnl_leaderboard_call(window, rank_by, false, limit, offset, None)).await
}

#[cfg(test)]
#[path = "tests/pnl.rs"]
mod tests;
