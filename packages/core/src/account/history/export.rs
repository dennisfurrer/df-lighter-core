//! Whole histories as a file: `export`, the account's trades (up to a million) or funding payments
//! over any range, answered as a link to a CSV to download; and an integrator's attributed volume
//! and fees (`partnerStats`, public).

use serde::Deserialize;

use crate::account::history::fills::{Role, kind_filter};
use crate::client::{ApiError, Call, Transport, read};
use crate::markets::TradeKind;
use crate::utils::ids::whole;
use crate::utils::lenient::{text, zero_dec};
use crate::utils::{AccountIndex, Dec, MarketId, Side};

/// What an export holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportKind {
    Trades,
    Funding,
}

impl ExportKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            ExportKind::Trades => "trade",
            ExportKind::Funding => "funding",
        }
    }
}

/// What `export` asks. Both bounds `None` export everything there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportQuery {
    pub account: AccountIndex,
    pub kind: ExportKind,
    pub market: Option<MarketId>,
    /// Unix milliseconds.
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
    /// Longs or shorts only.
    pub side: Option<Side>,
    pub role: Option<Role>,
    /// One kind of trade only.
    pub trade_kind: Option<TradeKind>,
}

impl ExportQuery {
    /// Everything of `kind` there is.
    #[must_use]
    pub const fn new(account: AccountIndex, kind: ExportKind) -> ExportQuery {
        ExportQuery {
            account,
            kind,
            market: None,
            from_ms: None,
            to_ms: None,
            side: None,
            role: None,
            trade_kind: None,
        }
    }
}

#[derive(Deserialize)]
struct Export {
    #[serde(default, deserialize_with = "text")]
    data_url: String,
}

/// What `export` asks for `query`, with the token.
#[must_use]
pub fn export_call(query: &ExportQuery) -> Call {
    Call::get("export")
        .field("account_index", query.account)
        .maybe("market_id", query.market)
        .field("type", query.kind.name())
        .maybe("start_timestamp", query.from_ms)
        .maybe("end_timestamp", query.to_ms)
        .maybe(
            "side",
            query.side.map(|side| match side {
                Side::Bid => "long",
                Side::Ask => "short",
            }),
        )
        .maybe("role", query.role.map(Role::name))
        .maybe("trade_type", query.trade_kind.and_then(kind_filter))
        .authed()
}

/// The link to the export's file.
pub async fn export(transport: &impl Transport, query: &ExportQuery) -> Result<String, ApiError> {
    Ok(read::<Export>(transport, export_call(query)).await?.data_url)
}

/// What an integrator earned from the trades attributed to it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PartnerStats {
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_fees_earned: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_taker_fees_earned: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_maker_fees_earned: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_taker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_maker_volume: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub total_trades: u64,
    #[serde(default, deserialize_with = "whole")]
    pub total_taker_trades: u64,
    #[serde(default, deserialize_with = "whole")]
    pub total_maker_trades: u64,
    #[serde(default, deserialize_with = "whole")]
    pub unique_clients: u64,
}

/// What `partnerStats` asks: all time when both bounds are `None`. `period` is the venue's own
/// word, passed as given.
#[must_use]
pub fn partner_stats_call(integrator: AccountIndex, from: Option<u64>, to: Option<u64>, period: Option<&str>) -> Call {
    Call::get("partnerStats")
        .field("account_index", integrator)
        .maybe("start_timestamp", from)
        .maybe("end_timestamp", to)
        .maybe("period", period)
}

pub async fn partner_stats(
    transport: &impl Transport,
    integrator: AccountIndex,
    from: Option<u64>,
    to: Option<u64>,
) -> Result<PartnerStats, ApiError> {
    read(transport, partner_stats_call(integrator, from, to, None)).await
}

#[cfg(test)]
#[path = "tests/export.rs"]
mod tests;
