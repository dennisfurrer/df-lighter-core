//! What a network lists: `orderBooks`, every market in brief, and `orderBookDetails`, each perp's
//! and spot pair's grid, minimums, fees, margin, prices and the day's stats. Binary options are
//! left out of both but for `orderBooks` asked by one id: [`crate::markets::binary`] lists them.
//!
//! A market's kind is its `market_type`, never its id: spot ids start at 2048, but so do some
//! perps (STONK is 4095 on Lighter, UNI 4095 on Robinhood Chain). A [`Markets`] snapshot holds one
//! `orderBookDetails` answer and finds a market by id or by symbol (`BTC`, `ETH/USDC`,
//! `AAPL/USDG`). Robinhood Chain writes no deployer and no open interest in USD or cap.

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::binary::Outcome;
use crate::markets::listed;
use crate::markets::stats::stat_value;
use crate::utils::dec::{Dec, Round};
use crate::utils::figure::Figure;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, AssetId, Grid, MarketId, MarketKind};

/// The margin fractions are in ten-thousandths of the notional: 500 is 5%, 20x.
pub const MARGIN_FRACTION_ONE: u32 = 10_000;

/// Which markets a listing asks for. The venue takes `all`, `perp` and `spot`; binary options
/// are not a filter (`bo` is refused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MarketFilter {
    #[default]
    All,
    Perps,
    Spot,
}

impl MarketFilter {
    /// The word `filter` takes.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            MarketFilter::All => "all",
            MarketFilter::Perps => "perp",
            MarketFilter::Spot => "spot",
        }
    }
}

/// Where a market is in its life. Perps and spot are `active` or `inactive`; a binary option
/// moves on through settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarketStatus {
    /// Listed but not trading: delisted, or a binary option before its window opens.
    Inactive,
    Active,
    /// A binary option whose window closed, its outcome pending: cancels only.
    WaitingForSettlement,
    /// A binary option the operator is settling: cancels only.
    InSettlement,
    /// A binary option paid out; its outcome and settlement price are final.
    Settled,
    #[serde(other)]
    Other,
}

/// One market as `orderBooks` lists it: no prices and no grid of its own beyond the decimals it
/// supports. Fees are in percent of the notional.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarketListing {
    /// `BTC` for a perp, `ETH/USDC` for a spot pair, `BTC-UPDOWN-15M-1792917900` for a binary
    /// option.
    pub symbol: String,
    pub market_id: MarketId,
    /// `None` for a word this build does not know.
    #[serde(default, deserialize_with = "kind")]
    pub market_type: Option<MarketKind>,
    /// A spot pair's assets; `None` (the venue's 0) for a perp.
    #[serde(default, deserialize_with = "asset")]
    pub base_asset_id: Option<AssetId>,
    #[serde(default, deserialize_with = "asset")]
    pub quote_asset_id: Option<AssetId>,
    pub status: MarketStatus,
    /// Percent of the notional: `0.0000` on most markets.
    pub taker_fee: Dec,
    pub maker_fee: Dec,
    /// Percent of the notional a liquidation pays the LLP.
    pub liquidation_fee: Dec,
    #[serde(default)]
    pub is_taker_fee_enabled: bool,
    #[serde(default)]
    pub is_maker_fee_enabled: bool,
    /// The least size a resting order may have, in the base asset.
    pub min_base_amount: Dec,
    /// The least notional a resting order may have, in the collateral.
    pub min_quote_amount: Dec,
    /// The most notional one order may have.
    pub order_quote_limit: Dec,
    pub supported_size_decimals: u32,
    pub supported_price_decimals: u32,
    pub supported_quote_decimals: u32,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub created_at: u64,
    /// Units of the underlying one unit of the base stands for: above 1 for a tokenised fund
    /// that accrues (rhSPY, AAPL/USDG).
    pub multiplier: Dec,
    /// Legacy for a perp (use [`MarketDetail::open_interest`]); outstanding contracts for a
    /// binary option, each matched pair once.
    pub open_interest: Dec,
    /// A perp's open interest over both sides at the mark, in the collateral. Lighter only.
    #[serde(default)]
    pub open_interest_usd: Option<Dec>,
    /// A perp's cap over both sides, in the base at the mark. Lighter only.
    #[serde(default)]
    pub open_interest_limit: Option<Dec>,
    /// A binary option's trading window, unix milliseconds; 0 for a perp or spot pair, and a 0
    /// start trades from listing.
    #[serde(default, deserialize_with = "whole")]
    pub start_timestamp: u64,
    #[serde(default, deserialize_with = "whole")]
    pub end_timestamp: u64,
    /// A binary option's, set at settlement.
    #[serde(default)]
    pub outcome: Outcome,
    /// The account that creates and settles a binary option; 1 elsewhere.
    #[serde(default, deserialize_with = "whole")]
    pub operator_account_index: u64,
    /// A binary option's full payout per share, in price ticks: 100 is 1.00.
    #[serde(default)]
    pub settlement_cap: u32,
    /// 0 discrete, 1 continuous.
    #[serde(default)]
    pub settlement_type: u8,
    #[serde(default)]
    pub settlement_price: u32,
    #[serde(default)]
    pub default_price: u32,
    /// The operator froze the book: creates and modifies are refused.
    #[serde(default)]
    pub is_frozen: bool,
    /// `lighter`, or another deployer's label. Lighter only.
    #[serde(default)]
    pub market_deployer: Option<String>,
}

impl MarketListing {
    /// The grid the venue supports, which is the one it signs at on every market listed today
    /// ([`MarketDetail::grid`] reads both).
    #[must_use]
    pub const fn grid(&self) -> Grid {
        Grid::new(self.supported_size_decimals, self.supported_price_decimals)
    }
}

/// Settings a perp's details carry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarketConfig {
    /// `0` cross and isolated, `1` isolated only.
    #[serde(default)]
    pub market_margin_mode: u8,
    /// The account that insures its liquidations: the LLP, or 2^48 - 1, which is no account
    /// (about half the perps on Lighter, every one on Robinhood Chain);
    /// [`MarketConfig::insurance_fund`] reads that as `None`.
    #[serde(default, deserialize_with = "whole")]
    pub insurance_fund_account_index: u64,
    #[serde(default)]
    pub liquidation_mode: u8,
    /// Only reduce-only orders are taken.
    #[serde(default)]
    pub force_reduce_only: bool,
    #[serde(default)]
    pub funding_fee_discounts_enabled: bool,
    /// Empty when it trades around the clock.
    #[serde(default)]
    pub trading_hours: String,
    /// The venue's own apps do not show it.
    #[serde(default)]
    pub hidden: bool,
    /// Large orders may be quoted by market makers (perpRFQ).
    #[serde(default)]
    pub rfq_enabled: bool,
}

impl MarketConfig {
    #[must_use]
    pub fn insurance_fund(&self) -> Option<AccountIndex> {
        AccountIndex::new(self.insurance_fund_account_index).ok()
    }
}

/// One perp or spot pair as `orderBookDetails` writes it. The fields after `open_interest` are a
/// perp's; a spot pair has none of them. The day's figures are bare JSON floats.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarketDetail {
    pub symbol: String,
    pub market_id: MarketId,
    #[serde(default, deserialize_with = "kind")]
    pub market_type: Option<MarketKind>,
    #[serde(default, deserialize_with = "asset")]
    pub base_asset_id: Option<AssetId>,
    #[serde(default, deserialize_with = "asset")]
    pub quote_asset_id: Option<AssetId>,
    pub status: MarketStatus,
    /// Percent of the notional.
    pub taker_fee: Dec,
    pub maker_fee: Dec,
    pub liquidation_fee: Dec,
    #[serde(default)]
    pub is_taker_fee_enabled: bool,
    #[serde(default)]
    pub is_maker_fee_enabled: bool,
    /// The least size a resting order may have; a taker is exempt.
    pub min_base_amount: Dec,
    /// The least notional a resting order may have.
    pub min_quote_amount: Dec,
    pub order_quote_limit: Dec,
    pub supported_size_decimals: u32,
    pub supported_price_decimals: u32,
    pub supported_quote_decimals: u32,
    /// The decimals an order's size and price are signed at.
    pub size_decimals: u32,
    pub price_decimals: u32,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub created_at: u64,
    pub multiplier: Dec,
    #[serde(default)]
    pub market_deployer: Option<String>,
    pub last_trade_price: Figure,
    #[serde(deserialize_with = "whole")]
    pub daily_trades_count: u64,
    /// The last 24 hours, in the base asset.
    pub daily_base_token_volume: Figure,
    /// The last 24 hours, in the collateral.
    pub daily_quote_token_volume: Figure,
    pub daily_price_low: Figure,
    pub daily_price_high: Figure,
    /// Percent.
    pub daily_price_change: Figure,
    /// A perp's open interest on one side, in the base (a float); a spot pair's `0.000000`.
    pub open_interest: Figure,
    #[serde(default)]
    pub mark_price: Option<Dec>,
    #[serde(default)]
    pub index_price: Option<Dec>,
    /// Ten-thousandths of the notional ([`MARGIN_FRACTION_ONE`]): the margin a new position
    /// takes unless its leverage is set.
    #[serde(default)]
    pub default_initial_margin_fraction: Option<u32>,
    /// The least initial margin, which sets the most leverage.
    #[serde(default)]
    pub min_initial_margin_fraction: Option<u32>,
    #[serde(default)]
    pub maintenance_margin_fraction: Option<u32>,
    #[serde(default)]
    pub closeout_margin_fraction: Option<u32>,
    #[serde(default)]
    pub quote_multiplier: Option<u64>,
    /// Over both sides at the mark, in the collateral. Lighter only.
    #[serde(default)]
    pub open_interest_usd: Option<Dec>,
    /// The cap over both sides, in the base at the mark; 0 before the first mark. Lighter only.
    #[serde(default)]
    pub open_interest_limit: Option<Dec>,
    #[serde(default)]
    pub market_config: Option<MarketConfig>,
    /// The LLP strategy the market belongs to.
    #[serde(default)]
    pub strategy_index: Option<u32>,
    #[serde(default)]
    pub market_flags: Option<u64>,
    /// Funding's premium weight, in hundredths: 100 for crypto, 50 for RWAs, 1 for pre-IPO.
    #[serde(default)]
    pub funding_premium_multiplier: Option<u32>,
    /// Funding's clamps and interest rate, in percent over eight hours: an hour settles an eighth.
    #[serde(default)]
    pub funding_clamp_small: Option<Dec>,
    #[serde(default)]
    pub funding_clamp_big: Option<Dec>,
    #[serde(default)]
    pub base_interest_rate: Option<Dec>,
}

impl MarketDetail {
    #[must_use]
    pub fn is_perp(&self) -> bool {
        self.market_type == Some(MarketKind::Perps)
    }

    #[must_use]
    pub fn is_spot(&self) -> bool {
        self.market_type == Some(MarketKind::Spot)
    }

    /// The decimals an order signs at.
    #[must_use]
    pub const fn grid(&self) -> Grid {
        Grid::new(self.size_decimals, self.price_decimals)
    }

    /// The smallest size step valid on both the signed grid and the one the venue supports (the
    /// two agree on every market today).
    #[must_use]
    pub fn size_step(&self) -> Dec {
        step(self.size_decimals.min(self.supported_size_decimals))
    }

    /// The smallest price move, on both grids.
    #[must_use]
    pub fn tick(&self) -> Dec {
        step(self.price_decimals.min(self.supported_price_decimals))
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == MarketStatus::Active
    }

    /// `market_config.hidden`: the venue's own apps do not show it.
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.market_config.as_ref().is_some_and(|config| config.hidden)
    }

    /// Listed for trading: active and shown. A market that is not stays a market, so positions in
    /// it still resolve.
    #[must_use]
    pub fn is_listed(&self) -> bool {
        self.is_active() && !self.is_hidden()
    }

    /// `market_config.force_reduce_only`: the venue takes only reduce-only orders.
    #[must_use]
    pub fn force_reduce_only(&self) -> bool {
        self.market_config.as_ref().is_some_and(|config| config.force_reduce_only)
    }

    /// No new exposure: reduce-only by the venue's setting, or not active.
    #[must_use]
    pub fn is_close_only(&self) -> bool {
        self.force_reduce_only() || !self.is_active()
    }

    /// `market_margin_mode` 1: isolated margin only.
    #[must_use]
    pub fn is_isolated_only(&self) -> bool {
        self.market_config.as_ref().is_some_and(|config| config.market_margin_mode == 1)
    }

    /// The most leverage a perp takes: `10000 / min_initial_margin_fraction`, rounded down and at
    /// least 1 (200 is 50x, 3333 is 3x). `None` for a spot pair.
    #[must_use]
    pub fn max_leverage(&self) -> Option<u32> {
        let fraction = self.min_initial_margin_fraction.filter(|fraction| *fraction > 0)?;
        Some((MARGIN_FRACTION_ONE / fraction).max(1))
    }

    /// What a perp tracks and how many of it one contract is ([`underlying`]).
    #[must_use]
    pub fn underlying(&self) -> (&str, Dec) {
        underlying(&self.symbol)
    }

    /// The last trade's price; `None` before the first.
    #[must_use]
    pub fn last_price(&self) -> Option<Dec> {
        stat_value(&self.last_trade_price).filter(|price| price.is_positive())
    }

    /// What a list shows: the last trade, else the mark.
    #[must_use]
    pub fn price(&self) -> Option<Dec> {
        self.last_price().or(self.mark_price.filter(|mark| mark.is_positive()))
    }

    /// The last 24 hours' change as a fraction: `0.0027` is 0.27%.
    #[must_use]
    pub fn daily_change(&self) -> Option<Dec> {
        stat_value(&self.daily_price_change)?.checked_div(Dec::from_int(100), 18, Round::Nearest)
    }

    /// The last 24 hours' volume in the collateral.
    #[must_use]
    pub fn daily_volume(&self) -> Option<Dec> {
        stat_value(&self.daily_quote_token_volume)
    }

    /// A perp's open interest on one side, in the base.
    #[must_use]
    pub fn open_interest_base(&self) -> Option<Dec> {
        stat_value(&self.open_interest)
    }
}

/// `10^-decimals`.
fn step(decimals: u32) -> Dec {
    Dec::new(1, decimals).unwrap_or(Dec::ONE)
}

/// What a perp's symbol tracks and how many units of it one contract is: `1000PEPE` is a thousand
/// PEPE; `0G` and `2Z` are names, as is any symbol not `1000` and letters.
#[must_use]
pub fn underlying(symbol: &str) -> (&str, Dec) {
    match symbol.strip_prefix("1000") {
        Some(rest) if !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_alphabetic()) => {
            (rest, Dec::from_int(1000))
        }
        _ => (symbol, Dec::ONE),
    }
}

/// One `orderBookDetails` answer: the perps and the spot pairs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Markets {
    #[serde(rename = "order_book_details", default, deserialize_with = "listed")]
    pub perps: Vec<MarketDetail>,
    /// Robinhood Chain may write `null`: none.
    #[serde(rename = "spot_order_book_details", default, deserialize_with = "listed")]
    pub spot: Vec<MarketDetail>,
}

impl Markets {
    /// Every market, perps first.
    pub fn iter(&self) -> impl Iterator<Item = &MarketDetail> {
        self.perps.iter().chain(&self.spot)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.perps.len() + self.spot.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn get(&self, market: MarketId) -> Option<&MarketDetail> {
        self.iter().find(|detail| detail.market_id == market)
    }

    /// By symbol as the venue writes it, else ignoring case: `BTC`, `ETH/USDC`, `weeth/usdc`.
    #[must_use]
    pub fn by_symbol(&self, symbol: &str) -> Option<&MarketDetail> {
        self.iter()
            .find(|detail| detail.symbol == symbol)
            .or_else(|| self.iter().find(|detail| detail.symbol.eq_ignore_ascii_case(symbol)))
    }

    /// Active and shown.
    pub fn listed(&self) -> impl Iterator<Item = &MarketDetail> {
        self.iter().filter(|detail| detail.is_listed())
    }

    /// Another answer's markets in: one that is here already is replaced, a new one added.
    pub fn extend(&mut self, other: Markets) {
        for detail in other.perps.into_iter().chain(other.spot) {
            self.perps.retain(|kept| kept.market_id != detail.market_id);
            self.spot.retain(|kept| kept.market_id != detail.market_id);
            match detail.is_spot() {
                true => self.spot.push(detail),
                false => self.perps.push(detail),
            }
        }
    }
}

#[derive(Deserialize)]
struct Listings {
    #[serde(default, deserialize_with = "listed")]
    order_books: Vec<MarketListing>,
}

/// What `orderBooks` asks: every market `filter` names.
#[must_use]
pub fn order_books_call(filter: MarketFilter) -> Call {
    Call::get("orderBooks").field("filter", filter.wire())
}

/// What `orderBooks` asks for one market: a binary option too.
#[must_use]
pub fn listing_call(market: MarketId) -> Call {
    Call::get("orderBooks").field("market_id", market)
}

/// Every perp and spot pair `filter` names, in brief.
pub async fn order_books(transport: &impl Transport, filter: MarketFilter) -> Result<Vec<MarketListing>, ApiError> {
    Ok(read::<Listings>(transport, order_books_call(filter)).await?.order_books)
}

/// One market in brief, a binary option too; `None` when the venue lists none by that id.
pub async fn listing(transport: &impl Transport, market: MarketId) -> Result<Option<MarketListing>, ApiError> {
    let listings = read::<Listings>(transport, listing_call(market)).await?.order_books;
    Ok(listings.into_iter().find(|listing| listing.market_id == market))
}

/// What `orderBookDetails` asks: every perp and spot pair `filter` names.
#[must_use]
pub fn order_book_details_call(filter: MarketFilter) -> Call {
    Call::get("orderBookDetails").field("filter", filter.wire())
}

/// What `orderBookDetails` asks for one market.
#[must_use]
pub fn market_detail_call(market: MarketId) -> Call {
    Call::get("orderBookDetails").field("market_id", market)
}

/// Every perp and spot pair `filter` names, in full.
pub async fn order_book_details(transport: &impl Transport, filter: MarketFilter) -> Result<Markets, ApiError> {
    read(transport, order_book_details_call(filter)).await
}

/// One perp or spot pair in full; `None` for a binary option or an id the venue does not know.
pub async fn market_detail(transport: &impl Transport, market: MarketId) -> Result<Option<MarketDetail>, ApiError> {
    let markets: Markets = read(transport, market_detail_call(market)).await?;
    Ok(markets.get(market).cloned())
}

fn kind<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<MarketKind>, D::Error> {
    crate::utils::order::known(deserializer)
}

/// An asset id, or `None` for the venue's 0.
fn asset<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<AssetId>, D::Error> {
    match whole(deserializer)? {
        0 => Ok(None),
        id => u16::try_from(id)
            .ok()
            .and_then(|id| AssetId::new(id).ok())
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom(format!("{id} is not an asset"))),
    }
}

#[cfg(test)]
#[path = "tests/books.rs"]
mod tests;
