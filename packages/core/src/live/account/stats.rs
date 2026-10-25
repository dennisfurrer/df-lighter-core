//! `user_stats`: what an account is worth and may still spend, in the collateral, for the whole
//! account and for its cross margin alone. Public: no auth token.

use serde::Deserialize;

use crate::utils::{AccountIndex, Dec};

/// One set of an account's figures, in the collateral.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Stats {
    pub collateral: Dec,
    pub portfolio_value: Dec,
    pub leverage: Dec,
    /// Free to open positions or withdraw.
    pub available_balance: Dec,
    /// In percent.
    pub margin_usage: Dec,
    pub buying_power: Dec,
}

/// `user_stats`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserStats {
    /// From the channel.
    pub account: AccountIndex,
    pub stats: Stats,
    /// The account's trading mode, as the venue numbers it.
    pub trading_mode: u8,
    /// Cross margin alone.
    pub cross: Option<Stats>,
    /// Cross and isolated together.
    pub total: Option<Stats>,
    /// Unix milliseconds, when the API server sent it.
    pub timestamp: u64,
}
