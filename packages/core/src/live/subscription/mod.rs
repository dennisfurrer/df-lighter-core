//! Every channel the venue serves (the websocket docs' list), as the channel a subscribe names:
//! `order_book/1`, `market_stats/all`, `candle/1/1m`, `up_down_candle/btc/15m/1s`,
//! `account_all/10`, `height`. A frame names its channel with colons instead (`order_book:1`),
//! and `account_orders` names its market alone. Whether one needs the account's auth token is the
//! docs' word; the venue refuses one without it (`auth field is required`).

mod parse;
mod wire;

use crate::client::Network;
use crate::live::market::{Resolution, UpDownResolution};
use crate::utils::{AccountIndex, MarketId};

/// One channel. `None` for a market is every market (`/all`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subscription {
    /// A market's whole book on subscribe, then every 50 ms the levels that changed.
    OrderBook {
        market: MarketId,
    },
    /// The best bid and offer, on every nonce that moves the book.
    Ticker {
        market: MarketId,
    },
    /// A perp's figures, or every perp's. Excludes binary options.
    MarketStats {
        market: Option<MarketId>,
    },
    /// A spot market's figures, or every spot market's.
    SpotMarketStats {
        market: Option<MarketId>,
    },
    /// A binary option's figures, or every binary option's. Lighter only.
    BoMarketStats {
        market: Option<MarketId>,
    },
    /// A market's trades and liquidations.
    Trade {
        market: MarketId,
    },
    Candle {
        market: MarketId,
        resolution: Resolution,
    },
    MarkPriceCandle {
        market: MarketId,
        resolution: Resolution,
    },
    /// An Up/Down series' underlying price, from the symbol and window `events` gives it
    /// (`btc`, `15m`). Lighter only.
    UpDownCandle {
        symbol: String,
        duration: String,
        resolution: UpDownResolution,
    },
    /// The chain's newest block.
    Height,
    /// Everything an account holds, with its recent trades and funding.
    AccountAll {
        account: AccountIndex,
    },
    /// One account on one market: orders, trades, position, balances.
    AccountMarket {
        market: MarketId,
        account: AccountIndex,
    },
    /// What an account is worth and may spend.
    UserStats {
        account: AccountIndex,
    },
    /// The transactions that touch an account, as they execute.
    AccountTx {
        account: AccountIndex,
    },
    AccountAllOrders {
        account: AccountIndex,
    },
    /// A pool's trades, orders, positions and shares, by the pool's account.
    PoolData {
        account: AccountIndex,
    },
    /// A pool's terms and returns, by the pool's account.
    PoolInfo {
        account: AccountIndex,
    },
    /// Liquidations, deleverages and announcements for an account.
    Notification {
        account: AccountIndex,
    },
    AccountOrders {
        market: MarketId,
        account: AccountIndex,
    },
    AccountAllTrades {
        account: AccountIndex,
    },
    AccountAllPositions {
        account: AccountIndex,
    },
    AccountAllAssets {
        account: AccountIndex,
    },
    /// What each spot asset cost the account on average.
    AccountSpotAvgEntryPrices {
        account: AccountIndex,
    },
    /// Open requests for a quote. The program is by invitation.
    Rfq,
    /// An account's points so far. Robinhood Chain only.
    LivePoints {
        account: AccountIndex,
    },
}

/// Why a channel is not one the venue serves, or not on this network.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadSubscription {
    #[error("`{0}` is not a channel the venue serves")]
    Unknown(String),
    #[error("`{kind}` needs its {part}")]
    Missing { kind: &'static str, part: &'static str },
    #[error("the {part} of `{kind}` must be {expected}")]
    Wrong { kind: &'static str, part: &'static str, expected: &'static str },
    #[error("`{0}` has more parts than its channel takes")]
    TooLong(String),
    #[error("`{kind}` is not served on {network}")]
    OtherNetwork { kind: &'static str, network: Network },
}

impl Subscription {
    /// Its name, the channel's first part.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Subscription::OrderBook { .. } => "order_book",
            Subscription::Ticker { .. } => "ticker",
            Subscription::MarketStats { .. } => "market_stats",
            Subscription::SpotMarketStats { .. } => "spot_market_stats",
            Subscription::BoMarketStats { .. } => "bo_market_stats",
            Subscription::Trade { .. } => "trade",
            Subscription::Candle { .. } => "candle",
            Subscription::MarkPriceCandle { .. } => "mark_price_candle",
            Subscription::UpDownCandle { .. } => "up_down_candle",
            Subscription::Height => "height",
            Subscription::AccountAll { .. } => "account_all",
            Subscription::AccountMarket { .. } => "account_market",
            Subscription::UserStats { .. } => "user_stats",
            Subscription::AccountTx { .. } => "account_tx",
            Subscription::AccountAllOrders { .. } => "account_all_orders",
            Subscription::PoolData { .. } => "pool_data",
            Subscription::PoolInfo { .. } => "pool_info",
            Subscription::Notification { .. } => "notification",
            Subscription::AccountOrders { .. } => "account_orders",
            Subscription::AccountAllTrades { .. } => "account_all_trades",
            Subscription::AccountAllPositions { .. } => "account_all_positions",
            Subscription::AccountAllAssets { .. } => "account_all_assets",
            Subscription::AccountSpotAvgEntryPrices { .. } => "account_spot_avg_entry_prices",
            Subscription::Rfq => "rfq",
            Subscription::LivePoints { .. } => "live_points",
        }
    }

    /// Whether the venue refuses it without the account's auth token.
    #[must_use]
    pub const fn needs_auth(&self) -> bool {
        matches!(
            self,
            Subscription::AccountMarket { .. }
                | Subscription::AccountTx { .. }
                | Subscription::AccountAllOrders { .. }
                | Subscription::Notification { .. }
                | Subscription::AccountOrders { .. }
                | Subscription::AccountAllAssets { .. }
                | Subscription::AccountSpotAvgEntryPrices { .. }
                | Subscription::Rfq
                | Subscription::LivePoints { .. }
        )
    }

    /// Whether its subscribe carries the token when there is one: every channel that needs it,
    /// and a pool's, which needs it for a pool that is not public.
    #[must_use]
    pub const fn takes_auth(&self) -> bool {
        self.needs_auth() || matches!(self, Subscription::PoolData { .. } | Subscription::PoolInfo { .. })
    }

    /// The account it follows. The venue allows an IP 5000.
    #[must_use]
    pub const fn account(&self) -> Option<AccountIndex> {
        match self {
            Subscription::AccountAll { account }
            | Subscription::AccountMarket { account, .. }
            | Subscription::UserStats { account }
            | Subscription::AccountTx { account }
            | Subscription::AccountAllOrders { account }
            | Subscription::PoolData { account }
            | Subscription::PoolInfo { account }
            | Subscription::Notification { account }
            | Subscription::AccountOrders { account, .. }
            | Subscription::AccountAllTrades { account }
            | Subscription::AccountAllPositions { account }
            | Subscription::AccountAllAssets { account }
            | Subscription::AccountSpotAvgEntryPrices { account }
            | Subscription::LivePoints { account } => Some(*account),
            _ => None,
        }
    }

    /// The one market it follows; `None` for every market or none.
    #[must_use]
    pub const fn market(&self) -> Option<MarketId> {
        match self {
            Subscription::OrderBook { market }
            | Subscription::Ticker { market }
            | Subscription::Trade { market }
            | Subscription::Candle { market, .. }
            | Subscription::MarkPriceCandle { market, .. }
            | Subscription::AccountMarket { market, .. }
            | Subscription::AccountOrders { market, .. } => Some(*market),
            Subscription::MarketStats { market }
            | Subscription::SpotMarketStats { market }
            | Subscription::BoMarketStats { market } => *market,
            _ => None,
        }
    }

    /// Whether `network` serves it: binary options are Lighter's, live points Robinhood Chain's.
    #[must_use]
    pub const fn serves(&self, network: Network) -> bool {
        match self {
            Subscription::BoMarketStats { .. } | Subscription::UpDownCandle { .. } => network.has_binary_options(),
            Subscription::LivePoints { .. } => matches!(network, Network::Robinhood),
            _ => true,
        }
    }

    /// Whether the venue would take it on `network`: an Up/Down series names its symbol and
    /// window, each one plain word.
    pub fn check(&self, network: Network) -> Result<(), BadSubscription> {
        if !self.serves(network) {
            return Err(BadSubscription::OtherNetwork { kind: self.kind(), network });
        }
        if let Subscription::UpDownCandle { symbol, duration, .. } = self {
            let plain = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric());
            if !plain(symbol) {
                return Err(BadSubscription::Wrong { kind: self.kind(), part: "symbol", expected: "one plain word" });
            }
            if !plain(duration) {
                return Err(BadSubscription::Wrong { kind: self.kind(), part: "duration", expected: "one plain word" });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/subscription.rs"]
mod tests;
