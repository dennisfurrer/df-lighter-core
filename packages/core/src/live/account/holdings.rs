//! What an account holds, as the socket sends it: `account_all` (everything, with recent trades
//! and funding), `account_all_positions`, `account_all_assets` and
//! `account_spot_avg_entry_prices`. On subscribe, the whole; an update carries only what changed
//! (one market's position, a few trades), and its assets may be left out when none moved. A
//! binary option position and a funding payment are the rows `account` and `positionFunding`
//! answer ([`BoPosition`], [`PositionFunding`]).

use serde::Deserialize;

use crate::account::history::PositionFunding;
use crate::account::holdings::BoPosition;
use crate::account::{Asset, PoolShare, Position};
use crate::markets::Trade;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, AssetId, Dec, Figure, MarketId};

/// What the account traded, over four windows. Sent on subscribe only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volumes {
    /// In the collateral.
    pub daily: Figure,
    pub weekly: Figure,
    pub monthly: Figure,
    pub total: Figure,
    /// Trades in each window, where the channel counts them.
    pub daily_trades: Option<u64>,
    pub weekly_trades: Option<u64>,
    pub monthly_trades: Option<u64>,
    pub total_trades: Option<u64>,
}

/// `account_all`: every position, balance, pool share, binary option position, recent trade and
/// funding payment of one account. Public: no auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountAll {
    pub account: AccountIndex,
    /// Empty on an update where no balance moved.
    pub assets: Vec<Asset>,
    /// Every market the account ever traded on a snapshot, flat ones included; on an update, the
    /// ones that changed.
    pub positions: Vec<Position>,
    pub bo_positions: Vec<BoPosition>,
    pub shares: Vec<PoolShare>,
    pub trades: Vec<Trade>,
    pub fundings: Vec<PositionFunding>,
    /// On subscribe only.
    pub volumes: Option<Volumes>,
    /// Unix microseconds.
    pub transaction_time: u64,
}

/// `account_all_positions`: the positions and pool shares alone. Public: no auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountAllPositions {
    /// From the channel.
    pub account: AccountIndex,
    pub positions: Vec<Position>,
    pub bo_positions: Vec<BoPosition>,
    pub shares: Vec<PoolShare>,
    /// On an update after a funding round: each market's last funding payment, and the discount
    /// taken off it.
    pub last_funding_round: Vec<(MarketId, Dec)>,
    pub last_funding_discount: Vec<(MarketId, Dec)>,
    /// Unix microseconds.
    pub transaction_time: u64,
}

/// `account_all_assets`: every balance. `balance` is in the asset, not the collateral. Needs the
/// account's auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountAllAssets {
    /// From the channel.
    pub account: AccountIndex,
    pub assets: Vec<Asset>,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
    /// Unix microseconds.
    pub transaction_time: u64,
}

/// What a spot asset cost the account on average: every trade, deposit, withdrawal, transfer and
/// stake counted as a buy or a sell at the index price.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvgEntryPrice {
    pub asset_id: AssetId,
    pub avg_entry_price: Dec,
    pub asset_size: Dec,
    /// The figures hold up to this trade.
    #[serde(default, deserialize_with = "whole")]
    pub last_trade_id: u64,
}

/// `account_spot_avg_entry_prices`. Needs the account's auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvgEntryPrices {
    /// From the channel.
    pub account: AccountIndex,
    pub prices: Vec<AvgEntryPrice>,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}
