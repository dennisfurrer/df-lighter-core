//! `market_stats`, `spot_market_stats` and `bo_market_stats`: one market's figures or, on the
//! `all` channel, a map of them by market, of which an update carries only the markets that moved.
//! Prices come as strings, blank on a side with nothing resting (`None` here); daily volumes and
//! price moves as bare JSON floats ([`Figure`]). Perps and spot exclude binary options, which have
//! their own channel.

use serde::Deserialize;

use crate::live::frame::fields::blank;
use crate::utils::ids::whole;
use crate::utils::{Dec, Figure, MarketId};

/// One perp market's figures. Not [`crate::markets::MarketStats`], which is one row of
/// `exchangeStats`' 24 hour summary (last price, trade count, volumes, change): this carries the
/// index, mark and mid, the best bid and offer, open interest, and the funding rates and clamps
/// besides.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarketStats {
    pub symbol: String,
    pub market_id: MarketId,
    pub index_price: Dec,
    pub mark_price: Dec,
    #[serde(default, deserialize_with = "blank")]
    pub mid_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub best_ask_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub best_bid_price: Option<Dec>,
    /// In the base asset.
    pub open_interest: Dec,
    pub open_interest_limit: Dec,
    /// In the collateral; not on every frame.
    #[serde(default, deserialize_with = "blank")]
    pub open_interest_usd: Option<Dec>,
    pub funding_clamp_small: Dec,
    pub funding_clamp_big: Dec,
    /// Zero before the first trade.
    pub last_trade_price: Dec,
    /// The estimate of the next funding payment, in percent.
    pub current_funding_rate: Dec,
    /// The last funding payment, at `funding_timestamp`, in percent.
    pub funding_rate: Dec,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub funding_timestamp: u64,
    pub daily_base_token_volume: Figure,
    pub daily_quote_token_volume: Figure,
    pub daily_price_low: Figure,
    pub daily_price_high: Figure,
    /// In percent.
    pub daily_price_change: Figure,
    #[serde(default, deserialize_with = "blank")]
    pub base_interest_rate: Option<Dec>,
    /// Mark over index, in percent.
    #[serde(default, deserialize_with = "blank")]
    pub premium: Option<Dec>,
}

/// One spot market's figures.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SpotMarketStats {
    /// `ETH/USDC`.
    pub symbol: String,
    pub market_id: MarketId,
    #[serde(default, deserialize_with = "blank")]
    pub index_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub mid_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub best_ask_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub best_bid_price: Option<Dec>,
    /// Zero before the first trade.
    pub last_trade_price: Dec,
    pub daily_base_token_volume: Figure,
    pub daily_quote_token_volume: Figure,
    pub daily_price_low: Figure,
    pub daily_price_high: Figure,
    /// In percent.
    pub daily_price_change: Figure,
}

/// Where a binary option is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoStatus {
    /// Listed, not yet trading.
    Inactive,
    /// Trading, inside its window and unless frozen (the market's definition in `events` says).
    Active,
    /// Its window closed; the outcome is not known yet.
    WaitingForSettlement,
    InSettlement,
    Settled,
    #[serde(other)]
    Other,
}

/// One binary option's figures. The `all` channel also carries old, settled and not yet open
/// markets: filter by `status`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BoMarketStats {
    /// `BTC-UPDOWN-15M-1792917000`.
    pub symbol: String,
    pub market_id: MarketId,
    #[serde(default, deserialize_with = "blank")]
    pub mid_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub best_ask_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub best_bid_price: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub last_trade_price: Option<Dec>,
    pub daily_base_token_volume: Figure,
    pub daily_quote_token_volume: Figure,
    pub daily_price_low: Figure,
    pub daily_price_high: Figure,
    pub daily_price_change: Figure,
    pub lifetime_quote_token_volume: Figure,
    pub lifetime_base_token_volume: Figure,
    #[serde(default, deserialize_with = "blank")]
    pub open_interest: Option<Dec>,
    pub status: BoStatus,
    /// `0` none yet, `1` No (Down), `2` Yes (Up).
    #[serde(default)]
    pub outcome: u8,
}
