//! The account's liquidations and deleverages (`liquidations`, with the auth token): the trade that
//! closed the position, and the account as it stood (positions, assets, risk before and after,
//! the mark prices used).
//!
//! A partial liquidation sells into the book; a deleverage closes the position against a
//! counterparty's (auto-deleveraging). Prices and amounts are decimal strings; mark prices are the
//! venue's floats.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::account::holdings::positions::{Asset, Position};
use crate::client::{ApiError, Call, Transport, read};
use crate::utils::lenient::{blank_dec, list, maybe_market, unsigned, zero_dec};
use crate::utils::{AccountIndex, Dec, Figure, MarketId};

/// What kind of liquidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LiquidationKind {
    /// Sold into the book.
    Partial,
    /// Closed against a counterparty's position.
    Deleverage,
    #[serde(other)]
    Other,
}

/// The trade that closed it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LiquidationTrade {
    pub price: Dec,
    pub size: Dec,
    /// Rates as the venue writes them.
    #[serde(default, deserialize_with = "blank_dec")]
    pub taker_fee: Option<Dec>,
    #[serde(default, deserialize_with = "blank_dec")]
    pub maker_fee: Option<Dec>,
    /// Unix microseconds.
    #[serde(default, deserialize_with = "unsigned")]
    pub transaction_time: u64,
    pub ask_account_id: AccountIndex,
    pub bid_account_id: AccountIndex,
}

/// An account's risk for its cross positions, or one isolated position.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RiskParameters {
    /// The isolated position's market; `None` for the cross figures (the venue writes 255).
    #[serde(default, deserialize_with = "maybe_market")]
    pub market_id: Option<MarketId>,
    #[serde(default, deserialize_with = "zero_dec")]
    pub collateral: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_account_value: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub initial_margin_req: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub maintenance_margin_req: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub close_out_margin_req: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_account_liquidation_threshold: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub usdc_collateral_with_funding: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub usdc_portfolio_value: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_portfolio_value: Dec,
}

/// The account's risk at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RiskInfo {
    pub cross_risk_parameters: Option<RiskParameters>,
    #[serde(default, deserialize_with = "list")]
    pub isolated_risk_parameters: Vec<RiskParameters>,
}

/// The account as it stood when liquidated.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LiquidationState {
    #[serde(default, deserialize_with = "list")]
    pub positions: Vec<Position>,
    #[serde(default, deserialize_with = "list")]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub risk_info_before: Option<RiskInfo>,
    #[serde(default)]
    pub risk_info_after: Option<RiskInfo>,
    /// By market id, as the venue keys them.
    #[serde(default)]
    pub mark_prices: BTreeMap<String, Figure>,
    /// By asset id.
    #[serde(default)]
    pub asset_index_prices: BTreeMap<String, String>,
}

/// One liquidation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Liquidation {
    #[serde(deserialize_with = "unsigned")]
    pub id: u64,
    pub market_id: MarketId,
    #[serde(rename = "type")]
    pub kind: LiquidationKind,
    pub trade: LiquidationTrade,
    pub info: Option<LiquidationState>,
    /// As the venue writes it (unix milliseconds).
    #[serde(default, deserialize_with = "unsigned")]
    pub executed_at: u64,
}

/// A page of liquidations.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LiquidationPage {
    #[serde(default, deserialize_with = "list")]
    pub liquidations: Vec<Liquidation>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// What `liquidations` asks: every market when `market` is `None`; `limit` 1 to 100.
#[must_use]
pub fn liquidations_call(account: AccountIndex, market: Option<MarketId>, cursor: Option<&str>, limit: u8) -> Call {
    Call::get("liquidations")
        .field("account_index", account)
        .maybe("market_id", market)
        .maybe("cursor", cursor)
        .field("limit", limit.clamp(1, 100))
        .authed()
}

pub async fn liquidations(
    transport: &impl Transport,
    account: AccountIndex,
    market: Option<MarketId>,
    cursor: Option<&str>,
    limit: u8,
) -> Result<LiquidationPage, ApiError> {
    read(transport, liquidations_call(account, market, cursor, limit)).await
}

#[cfg(test)]
#[path = "tests/liquidations.rs"]
mod tests;
