//! Funding: a perp's settlements (`fundings`) and the current rates on Lighter and three other
//! venues (`funding-rates`). A perp settles every hour, on the hour.
//!
//! The units differ by read, so each is converted by name:
//!
//! - `fundings` writes `timestamp` in seconds and `rate` in percent of the notional for the hour
//!   (`0.0012` is 0.000012), paid by the side `direction` names; `value` is what one unit of the
//!   base paid, in the collateral. [`Funding::fraction`] is the rate as a fraction, positive when
//!   longs paid.
//! - `funding-rates` writes each venue's current rate as a fraction over eight hours, a bare
//!   float; [`FundingRate::hourly`] is an hour's, which is what Lighter settles.

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::markets::stats::stat_value;
use crate::utils::MarketId;
use crate::utils::dec::{Dec, MAX_SCALE, Round};
use crate::utils::figure::Figure;
use crate::utils::ids::whole;
use crate::utils::time::{HOUR_MS, ms_to_seconds, seconds_to_ms};

/// A perp settles funding once an hour.
pub const FUNDING_INTERVAL_MS: u64 = HOUR_MS;

/// The most settlements one `fundings` read answers.
pub const FUNDINGS_PER_CALL: u64 = 750;

/// When the next settlement after `now_ms` is due: the next whole hour.
#[must_use]
pub const fn next_funding(now_ms: u64) -> u64 {
    (now_ms / FUNDING_INTERVAL_MS).saturating_add(1).saturating_mul(FUNDING_INTERVAL_MS)
}

/// The step `fundings` reads at: each hour's settlement, or a day's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FundingResolution {
    #[default]
    Hour,
    Day,
}

impl FundingResolution {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            FundingResolution::Hour => "1h",
            FundingResolution::Day => "1d",
        }
    }
}

/// The side that paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Payer {
    Long,
    Short,
    #[serde(other)]
    Other,
}

/// One settlement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Funding {
    /// Unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    /// What one unit of the base paid, in the collateral.
    pub value: Dec,
    /// Percent of the notional for the hour, unsigned.
    pub rate: Dec,
    #[serde(rename = "direction")]
    pub payer: Payer,
}

impl Funding {
    /// Unix milliseconds.
    #[must_use]
    pub const fn at_ms(&self) -> u64 {
        seconds_to_ms(self.timestamp)
    }

    /// The rate as a fraction, positive when longs paid shorts and negative when shorts paid;
    /// `None` for a side this build does not know.
    #[must_use]
    pub fn fraction(&self) -> Option<Dec> {
        let fraction = self.rate.checked_div(Dec::from_int(100), MAX_SCALE, Round::Nearest)?;
        match self.payer {
            Payer::Long => Some(fraction),
            Payer::Short => Some(fraction.negate()),
            Payer::Other => None,
        }
    }
}

#[derive(Deserialize)]
struct Fundings {
    #[serde(default, deserialize_with = "listed")]
    fundings: Vec<Funding>,
}

/// What `fundings` asks: `count_back` settlements back from `end_s`, the times unix seconds.
#[must_use]
pub fn fundings_call(
    market: MarketId,
    resolution: FundingResolution,
    start_s: u64,
    end_s: u64,
    count_back: u64,
) -> Call {
    Call::get("fundings")
        .field("market_id", market)
        .field("resolution", resolution.wire())
        .field("start_timestamp", start_s)
        .field("end_timestamp", end_s)
        .field("count_back", count_back.min(FUNDINGS_PER_CALL))
}

/// One window's settlements, oldest first.
pub async fn fundings(
    transport: &impl Transport,
    market: MarketId,
    resolution: FundingResolution,
    start_s: u64,
    end_s: u64,
    count_back: u64,
) -> Result<Vec<Funding>, ApiError> {
    Ok(read::<Fundings>(transport, fundings_call(market, resolution, start_s, end_s, count_back)).await?.fundings)
}

/// How many hourly settlements to ask for over `start_ms..=end_ms`. For a range that has ended
/// the venue answers one fewer than `count_back`, back from its end, so one more than its hours
/// covers its first: the hours, plus two, at most [`FUNDINGS_PER_CALL`].
#[must_use]
pub fn funding_count_back(start_ms: u64, end_ms: u64) -> u64 {
    (end_ms.saturating_sub(start_ms) / FUNDING_INTERVAL_MS).saturating_add(2).clamp(1, FUNDINGS_PER_CALL)
}

/// A perp's hourly settlements over `start_ms..=end_ms`, oldest first: one read, so at most
/// [`FUNDINGS_PER_CALL`] hours back from the end.
pub async fn funding_history(
    transport: &impl Transport,
    market: MarketId,
    start_ms: u64,
    end_ms: u64,
) -> Result<Vec<Funding>, ApiError> {
    let (start_s, end_s) = (ms_to_seconds(start_ms), ms_to_seconds(end_ms));
    let count = funding_count_back(start_ms, end_ms);
    let mut rows = fundings(transport, market, FundingResolution::Hour, start_s, end_s, count).await?;
    rows.retain(|row| (start_ms..=end_ms).contains(&row.at_ms()));
    rows.sort_by_key(|row| row.timestamp);
    Ok(rows)
}

/// A venue `funding-rates` compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Exchange {
    Lighter,
    Binance,
    Bybit,
    Hyperliquid,
    #[serde(other)]
    Other,
}

/// One venue's current rate for one of this network's perps.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FundingRate {
    /// This network's market the symbol is.
    pub market_id: MarketId,
    pub exchange: Exchange,
    pub symbol: String,
    /// A fraction over eight hours, as the venue wrote it.
    pub rate: Figure,
}

impl FundingRate {
    /// The fraction over eight hours. The venue computes it in floating point, so it is read
    /// through [`stat_value`]: `0.00009599999999999999` is `0.000096`.
    #[must_use]
    pub fn eight_hour(&self) -> Option<Dec> {
        stat_value(&self.rate)
    }

    /// The fraction for one hour: an eighth of the eight hours' (`0.000096` is `0.000012`).
    #[must_use]
    pub fn hourly(&self) -> Option<Dec> {
        self.eight_hour()?.checked_div(Dec::from_int(8), MAX_SCALE, Round::Nearest)
    }
}

#[derive(Deserialize)]
struct FundingRates {
    #[serde(default, deserialize_with = "listed")]
    funding_rates: Vec<FundingRate>,
}

/// What `funding-rates` asks.
#[must_use]
pub fn funding_rates_call() -> Call {
    Call::get("funding-rates")
}

/// Every venue's current rate for each of this network's perps it lists.
pub async fn funding_rates(transport: &impl Transport) -> Result<Vec<FundingRate>, ApiError> {
    Ok(read::<FundingRates>(transport, funding_rates_call()).await?.funding_rates)
}

#[cfg(test)]
#[path = "tests/funding.rs"]
mod tests;
