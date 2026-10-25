//! Assets and where their prices come from: `assetDetails` (each asset the venue holds, its
//! decimals, margin terms and price), `tokenlist` (the names, logos and categories the venue's app
//! shows), `syntheticSpotInfo` (a perp priced off a futures contract, and its carry) and
//! `priceOracleInfo` (the sources behind each market's index and mark).
//!
//! An asset counts in two decimals: `decimals` on the venue, which a transfer or withdrawal is
//! signed in, and `l1_decimals` for its token on the L1 (ETH 8 and 18). Lighter holds ETH, LIT,
//! USDC and a few tokens; Robinhood Chain holds USDG, ETH and tokenised US stocks and funds.

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::utils::dec::Dec;
use crate::utils::figure::Figure;
use crate::utils::ids::whole;
use crate::utils::{Address, AssetId};

/// Whether an asset counts as perps margin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetMarginMode {
    /// Collateral: its balance counts at its loan-to-value.
    Enabled,
    /// Held and traded, never margin.
    Disabled,
    /// Priced by the venue but not taken as margin.
    PricedOnly,
    #[serde(other)]
    Other,
}

/// One asset as `assetDetails` writes it. The margin terms are fractions: `0.8000` is 80%.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssetDetail {
    pub asset_id: AssetId,
    pub symbol: String,
    /// Its token's decimals on the L1, which a deposit counts in.
    pub l1_decimals: u32,
    /// The decimals the venue counts it in, which a transfer or a withdrawal is signed in.
    pub decimals: u32,
    pub min_transfer_amount: Dec,
    pub min_withdrawal_amount: Dec,
    pub margin_mode: AssetMarginMode,
    /// In the collateral.
    pub index_price: Dec,
    pub price_decimals: u32,
    /// Its token on the L1; zero for the L1's own coin.
    pub l1_address: Address,
    /// The share of its value that counts toward the account's value.
    pub loan_to_value: Dec,
    /// The share that counts toward the liquidation threshold.
    pub liquidation_threshold: Dec,
    /// The least share of its value a liquidation pays the holder.
    pub liquidation_factor: Dec,
    pub liquidation_fee: Dec,
    /// The most all accounts, and one account, may hold as margin; zero when it is no margin.
    pub global_supply_cap: Dec,
    pub user_supply_cap: Dec,
    pub total_supplied: Dec,
    /// Units of the underlying one unit stands for: above 1 for a fund that accrues (rhSPY, SPY).
    pub multiplier: Dec,
}

impl AssetDetail {
    /// Collateral: `margin_mode` is `enabled`.
    #[must_use]
    pub fn is_margin(&self) -> bool {
        self.margin_mode == AssetMarginMode::Enabled
    }

    /// One unit is one of what its symbol names: no multiplier. A tokenised fund that accrues
    /// (rhQQQ) is not.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.multiplier == Dec::ONE
    }
}

#[derive(Deserialize)]
struct AssetDetails {
    #[serde(default, deserialize_with = "listed")]
    asset_details: Vec<AssetDetail>,
}

/// What `assetDetails` asks: one asset, or every asset when `asset` is `None`.
#[must_use]
pub fn asset_details_call(asset: Option<AssetId>) -> Call {
    Call::get("assetDetails").maybe("asset_id", asset)
}

/// Every asset the venue holds.
pub async fn asset_details(transport: &impl Transport) -> Result<Vec<AssetDetail>, ApiError> {
    Ok(read::<AssetDetails>(transport, asset_details_call(None)).await?.asset_details)
}

/// One asset; `None` when the venue lists none by that id.
pub async fn asset_detail(transport: &impl Transport, asset: AssetId) -> Result<Option<AssetDetail>, ApiError> {
    let details = read::<AssetDetails>(transport, asset_details_call(Some(asset))).await?.asset_details;
    Ok(details.into_iter().find(|detail| detail.asset_id == asset))
}

/// Which of the app's lists a token is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TokenMarket {
    Perps,
    Spot,
    #[serde(other)]
    Other,
}

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AssetClass {
    Crypto,
    /// A real-world asset: a stock, a fund, a commodity, a currency.
    Rwa,
    #[serde(other)]
    Other,
}

/// One token as the venue's app lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Token {
    pub symbol: String,
    /// The market's own symbol when it differs from the app's.
    #[serde(default)]
    pub backend_symbol: Option<String>,
    pub name: String,
    /// The logo's file name, without its extension.
    #[serde(default)]
    pub logo: String,
    /// `svg` or `png`.
    #[serde(default)]
    pub logo_extension: String,
    #[serde(default)]
    pub description_key: String,
    /// CoinGecko's and CoinPaprika's ids; empty when none.
    #[serde(default)]
    pub gecko_id: String,
    #[serde(default)]
    pub paprika_id: String,
    pub market: TokenMarket,
    pub asset_type: AssetClass,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub is_allowed_mainnet: bool,
    #[serde(default)]
    pub is_asset_allowed_mainnet: bool,
    /// The price step the app groups the book by, in ticks.
    #[serde(default, deserialize_with = "whole")]
    pub default_order_book_group_by: u64,
}

#[derive(Deserialize)]
struct TokenList {
    #[serde(default, deserialize_with = "listed")]
    tokens: Vec<Token>,
}

/// What `tokenlist` asks.
#[must_use]
pub fn token_list_call() -> Call {
    Call::get("tokenlist")
}

/// Every token the venue's app lists.
pub async fn token_list(transport: &impl Transport) -> Result<Vec<Token>, ApiError> {
    Ok(read::<TokenList>(transport, token_list_call()).await?.tokens)
}

/// A perp priced off a futures contract: the contract, and the carry taken off its price to make
/// a spot price.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SyntheticSpot {
    /// The futures contract: `EMU6` for US500.
    pub symbol: String,
    /// The price feed.
    pub source: String,
    /// The carry, in basis points a day (a bare float).
    pub bps_per_day: Figure,
    /// When the contract expires, unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub expiry_time_ms: u64,
    /// The underlying spot market's close the venue reckons with, unix milliseconds (the docs do
    /// not say more).
    #[serde(deserialize_with = "whole")]
    pub spot_close_ms: u64,
}

/// What `syntheticSpotInfo` asks, by the perp's symbol.
#[must_use]
pub fn synthetic_spot_info_call(symbol: &str) -> Call {
    Call::get("syntheticSpotInfo").field("symbol", symbol)
}

/// A perp's synthetic spot. The venue refuses a symbol that has none (`20001`).
pub async fn synthetic_spot_info(transport: &impl Transport, symbol: &str) -> Result<SyntheticSpot, ApiError> {
    read(transport, synthetic_spot_info_call(symbol)).await
}

/// Where one of a market's prices comes from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OraclePrice {
    /// `median`, or the one source in use.
    pub source: String,
    pub using_median: bool,
    /// The venue's own price from its book, when the feeds are stale.
    pub using_internal_price: bool,
    /// The sources the median is taken over.
    #[serde(default)]
    pub median_sources: Vec<String>,
    /// The cap around the reference price, in basis points; 0 when there is none.
    pub price_cap_bps: Figure,
    /// The cap's bounds are `reference * (1 -/+ bps / 10000)`.
    pub price_cap_reference_price: Figure,
}

/// One market's oracles.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OracleMarket {
    pub symbol: String,
    /// The venue's pre-market flag; only Lighter writes it.
    #[serde(default)]
    pub pre_market: bool,
    pub index: OraclePrice,
    pub mark: OraclePrice,
}

/// `priceOracleInfo`: every market's oracles, or one's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OracleInfo {
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub computed_at_ms: u64,
    #[serde(default, deserialize_with = "listed")]
    pub markets: Vec<OracleMarket>,
}

/// What `priceOracleInfo` asks: one market by symbol, or every market.
#[must_use]
pub fn price_oracle_info_call(symbol: Option<&str>) -> Call {
    Call::get("priceOracleInfo").maybe("symbol", symbol)
}

pub async fn price_oracle_info(transport: &impl Transport, symbol: Option<&str>) -> Result<OracleInfo, ApiError> {
    read(transport, price_oracle_info_call(symbol)).await
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
