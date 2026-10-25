//! The exchange as a whole: the last 24 hours per market (`exchangeStats`), a metric's history
//! (`exchangeMetrics`), slippage against other venues (`executeStats`) and the fee tiers
//! (`feeTiers`, Lighter only). Statistics are bare JSON floats, read as written ([`Figure`]);
//! [`stat_value`] gives the decimal the venue meant.

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::utils::dec::{Dec, Round};
use crate::utils::figure::Figure;
use crate::utils::ids::whole;

/// Significant figures [`stat_value`] keeps of a float.
pub const STAT_FIGURES: u32 = 12;

/// A statistic the venue computed in floating point, as a decimal: its digits cut to
/// [`STAT_FIGURES`] significant figures, which drops the float's noise in the last places and
/// keeps every digit meant (`349.8602000000589` is `349.8602`, `0.00009599999999999999` is
/// `0.000096`). `None` for a figure past what a [`Dec`] holds.
#[must_use]
pub fn stat_value(figure: &Figure) -> Option<Dec> {
    let value = figure.to_dec()?;
    Some(value.round_sig(STAT_FIGURES, Round::Nearest))
}

/// One market's last 24 hours.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarketStats {
    pub symbol: String,
    pub last_trade_price: Figure,
    #[serde(deserialize_with = "whole")]
    pub daily_trades_count: u64,
    pub daily_base_token_volume: Figure,
    pub daily_quote_token_volume: Figure,
    /// Percent.
    pub daily_price_change: Figure,
}

/// `exchangeStats`: every market's last 24 hours, binary options among them, and the totals.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExchangeStats {
    /// How many markets are listed below.
    #[serde(deserialize_with = "whole")]
    pub total: u64,
    #[serde(default, deserialize_with = "listed")]
    pub order_book_stats: Vec<MarketStats>,
    /// In the collateral.
    pub daily_usd_volume: Figure,
    #[serde(deserialize_with = "whole")]
    pub daily_trades_count: u64,
}

/// What `exchangeStats` asks.
#[must_use]
pub fn exchange_stats_call() -> Call {
    Call::get("exchangeStats")
}

pub async fn exchange_stats(transport: &impl Transport) -> Result<ExchangeStats, ApiError> {
    read(transport, exchange_stats_call()).await
}

/// How far back a history reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Period {
    /// `exchangeMetrics` only.
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
    All,
}

impl Period {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Period::Hour => "h",
            Period::Day => "d",
            Period::Week => "w",
            Period::Month => "m",
            Period::Quarter => "q",
            Period::Year => "y",
            Period::All => "all",
        }
    }
}

/// What an `exchangeMetrics` history counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    Volume,
    MakerFee,
    TakerFee,
    LiquidationFee,
    TradeCount,
    LiquidationCount,
    LiquidationVolume,
    Inflow,
    Outflow,
    TransferFee,
    WithdrawFee,
    OpenInterest,
    AccountCount,
    ActiveAccountCount,
    Tps,
    Buyback,
    BuybackUsdc,
}

impl MetricKind {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            MetricKind::Volume => "volume",
            MetricKind::MakerFee => "maker_fee",
            MetricKind::TakerFee => "taker_fee",
            MetricKind::LiquidationFee => "liquidation_fee",
            MetricKind::TradeCount => "trade_count",
            MetricKind::LiquidationCount => "liquidation_count",
            MetricKind::LiquidationVolume => "liquidation_volume",
            MetricKind::Inflow => "inflow",
            MetricKind::Outflow => "outflow",
            MetricKind::TransferFee => "transfer_fee",
            MetricKind::WithdrawFee => "withdraw_fee",
            MetricKind::OpenInterest => "open_interest",
            MetricKind::AccountCount => "account_count",
            MetricKind::ActiveAccountCount => "active_account_count",
            MetricKind::Tps => "tps",
            MetricKind::Buyback => "buyback",
            MetricKind::BuybackUsdc => "buyback_usdc",
        }
    }
}

/// One point of a metric's history.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Metric {
    /// The day it opens, unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    pub data: Figure,
}

#[derive(Deserialize)]
struct Metrics {
    #[serde(default, deserialize_with = "listed")]
    metrics: Vec<Metric>,
}

/// What `exchangeMetrics` asks: one metric over `period`, for the whole exchange or for one
/// market by its symbol. Not every period suits every kind: `d` volume is refused (`20001`), `w`
/// is daily points over a week.
#[must_use]
pub fn exchange_metrics_call(period: Period, kind: MetricKind, market: Option<&str>) -> Call {
    let call = Call::get("exchangeMetrics").field("period", period.wire()).field("kind", kind.wire());
    match market {
        Some(symbol) => call.field("filter", "byMarket").field("value", symbol),
        None => call,
    }
}

/// A metric's history, oldest first.
pub async fn exchange_metrics(
    transport: &impl Transport,
    period: Period,
    kind: MetricKind,
    market: Option<&str>,
) -> Result<Vec<Metric>, ApiError> {
    Ok(read::<Metrics>(transport, exchange_metrics_call(period, kind, market)).await?.metrics)
}

/// The average slippage of one order size on one venue's market.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Slippage {
    /// `lighter`, `binance`, `hyperliquid`, `bybit`, `aster`.
    pub exchange: String,
    pub market: String,
    /// The order's notional, in USD.
    #[serde(deserialize_with = "whole")]
    pub size_usd: u64,
    /// In basis points.
    pub avg_slippage: Figure,
    /// How many samples it averages.
    #[serde(deserialize_with = "whole")]
    pub data_count: u64,
}

/// One day's slippage, by venue, market and size.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExecuteStat {
    /// The day, unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    #[serde(default, deserialize_with = "listed")]
    pub slippage: Vec<Slippage>,
}

#[derive(Deserialize)]
struct ExecuteStats {
    #[serde(default, deserialize_with = "listed")]
    result: Vec<ExecuteStat>,
}

/// What `executeStats` asks; every period but [`Period::Hour`].
#[must_use]
pub fn execute_stats_call(period: Period) -> Call {
    Call::get("executeStats").field("period", period.wire())
}

/// Slippage on Lighter against other venues, a day at a time. Heavy: its answer runs to
/// hundreds of kilobytes.
pub async fn execute_stats(transport: &impl Transport, period: Period) -> Result<Vec<ExecuteStat>, ApiError> {
    Ok(read::<ExecuteStats>(transport, execute_stats_call(period)).await?.result)
}

/// Which tiers `feeTiers` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FeeFilter {
    #[default]
    All,
    /// The account tiers, which perps and spot share.
    Perps,
    Spot,
    /// The binary options volume tiers and the LIT staking discounts.
    BinaryOptions,
}

impl FeeFilter {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            FeeFilter::All => "all",
            FeeFilter::Perps => "perps",
            FeeFilter::Spot => "spot",
            FeeFilter::BinaryOptions => "bo",
        }
    }
}

/// An account tier's fees, in millionths of the notional (a trade's fee unit): 280 is 0.028%.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UserFeeTier {
    /// `standard`, `plus`, `premium`, `premium_1` ...
    pub name: String,
    pub level: u32,
    pub maker_fee_ticks: u32,
    pub taker_fee_ticks: u32,
    /// LIT staked to reach it.
    #[serde(deserialize_with = "whole")]
    pub required_lit_stakes: u64,
    #[serde(deserialize_with = "whole")]
    pub required_volume: u64,
}

/// A binary options volume tier: its multiplier on each market's fee.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BinaryFeeTier {
    pub name: String,
    /// Trailing volume to reach it, in the collateral.
    #[serde(deserialize_with = "whole")]
    pub required_volume: u64,
    pub fee_rate: Figure,
}

/// A LIT staking discount on binary options fees: its multiplier.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StakingFeeTier {
    pub name: String,
    #[serde(deserialize_with = "whole")]
    pub required_lit_stakes: u64,
    pub fee_rate: Figure,
}

/// `feeTiers`: what the filter asked for, the rest empty. An account's binary options multiplier
/// is its volume tier's rate times its staking tier's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct FeeTiers {
    #[serde(default, deserialize_with = "listed")]
    pub user_tiers: Vec<UserFeeTier>,
    #[serde(default, deserialize_with = "listed")]
    pub bo_tiers: Vec<BinaryFeeTier>,
    #[serde(default, deserialize_with = "listed")]
    pub staking_tiers: Vec<StakingFeeTier>,
}

/// What `feeTiers` asks.
#[must_use]
pub fn fee_tiers_call(filter: FeeFilter) -> Call {
    Call::get("feeTiers").field("filter", filter.wire())
}

/// The fee tiers. Lighter only: Robinhood Chain answers 403 with no body.
pub async fn fee_tiers(transport: &impl Transport, filter: FeeFilter) -> Result<FeeTiers, ApiError> {
    read(transport, fee_tiers_call(filter)).await
}

#[cfg(test)]
#[path = "tests/stats.rs"]
mod tests;
