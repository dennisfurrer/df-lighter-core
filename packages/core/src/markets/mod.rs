//! What is listed and what it trades at, on either network: every public market read, as plain
//! rows and free functions over any [`crate::client::Transport`]. Nothing is cached here; a
//! program keeps what it read.
//!
//! - [`books`]: the listing (`orderBooks`) and each perp's and spot pair's details
//!   (`orderBookDetails`): grid, minimums, fees, margin, prices and the day's stats, and a
//!   [`Markets`] snapshot looked up by id or symbol.
//! - [`book`]: a market's resting orders one by one (`orderBookOrders`), with their owners: the
//!   touch, depth by price, and the walk a taker's size makes through it.
//! - [`trades`](mod@trades): a trade as every read writes it; the tape (`recentTrades`) and the
//!   paged history (`trades`).
//! - [`candles`](mod@candles): trade candles (`candles`), mark price candles
//!   (`markPriceCandles`), the day's hourly prices (`marketPriceCharts`), and reading a long range
//!   in pages.
//! - [`funding`]: each hour's settlements (`fundings`) and the current rates on Lighter and
//!   three other venues (`funding-rates`).
//! - [`assets`]: assets (`assetDetails`), the token list (`tokenlist`), a synthetic spot's carry
//!   (`syntheticSpotInfo`) and the oracles behind each price (`priceOracleInfo`).
//! - [`stats`]: the exchange's day (`exchangeStats`), its history (`exchangeMetrics`), slippage
//!   against other venues (`executeStats`) and the fee tiers (`feeTiers`).
//! - [`system`]: the venue's own accounts and limits (`systemConfig`), its contract (`/info`),
//!   its L1 (`layer1BasicInfo`), the secure withdrawal delay (`withdrawalDelay`) and its
//!   announcements (`announcement`).
//! - [`binary`]: binary options, Lighter only: events and their markets (`events`) and the
//!   underlying price they settle on (`upDownCandles`).
//!
//! The venue names a market by an integer per network ([`crate::utils::MarketId`]); whether it is
//! a perp, a spot pair or a binary option is its `market_type`, never the id's range. Prices and
//! sizes are exact decimals. A few statistics come as bare JSON floats ([`crate::utils::Figure`]),
//! read as written; [`stat_value`] turns one into a decimal.

pub mod assets;
pub mod binary;
pub mod book;
pub mod books;
pub mod candles;
pub mod funding;
pub mod stats;
pub mod system;
pub mod trades;

pub use assets::{
    AssetClass, AssetDetail, AssetMarginMode, OracleInfo, OracleMarket, OraclePrice, SyntheticSpot, Token, TokenMarket,
    asset_detail, asset_details, price_oracle_info, synthetic_spot_info, token_list,
};
pub use binary::{
    BinaryMarket, BinaryStats, Event, EventKind, EventMetadata, EventPage, EventQuery, EventSort, Outcome,
    UpDownCandle, UpDownPage, UpDownResolution, events, up_down_candles,
};
pub use book::{Book, BookOrder, FilledOrder, Level, MOST_BOOK_ORDERS, Walk, order_book_orders, orders_filled};
pub use books::{
    MarketConfig, MarketDetail, MarketFilter, MarketListing, MarketStatus, Markets, listing, market_detail,
    order_book_details, order_books, underlying,
};
pub use candles::{
    Bar, CANDLES_PER_CALL, Candle, CandleQuery, CandleWindow, MarkCandle, PriceChart, Resolution, UnknownResolution,
    candle_history, candle_window, candles, mark_price_candles, market_price_charts,
};
pub use funding::{
    Exchange, FUNDING_INTERVAL_MS, Funding, FundingRate, FundingResolution, Payer, funding_history, funding_rates,
    fundings, next_funding,
};
pub use stats::{
    BinaryFeeTier, ExchangeStats, ExecuteStat, FeeFilter, FeeTiers, MarketStats, Metric, MetricKind, Period, Slippage,
    StakingFeeTier, UserFeeTier, exchange_metrics, exchange_stats, execute_stats, fee_tiers, stat_value,
};
pub use system::{
    Announcement, L1Info, L1Provider, NamedContract, SystemConfig, Validator, announcements, contract_address,
    layer1_basic_info, system_config, withdrawal_delay,
};
pub use trades::{MOST_TRADES, Role, SortBy, Trade, TradeKind, TradePage, TradeQuery, recent_trades, trades};

use serde::{Deserialize, Deserializer};

/// A list the venue may write as `null` (Robinhood Chain's `spot_order_book_details`) or leave
/// out: either is empty.
pub(crate) fn listed<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
