//! Reading a channel back into its subscription ([`Subscription::parse`]), as a subscribe names it
//! (`candle/1/1m`) or as a frame does (`candle:1:1m`).

use crate::live::market::{Resolution, UpDownResolution};
use crate::live::{BadSubscription, Subscription};
use crate::utils::{AccountIndex, MarketId};

/// Every channel, in the docs' order.
const KINDS: [&str; 25] = [
    "order_book",
    "ticker",
    "market_stats",
    "spot_market_stats",
    "bo_market_stats",
    "trade",
    "candle",
    "mark_price_candle",
    "up_down_candle",
    "height",
    "account_all",
    "account_market",
    "user_stats",
    "account_tx",
    "account_all_orders",
    "pool_data",
    "pool_info",
    "notification",
    "account_orders",
    "account_all_trades",
    "account_all_positions",
    "account_all_assets",
    "account_spot_avg_entry_prices",
    "rfq",
    "live_points",
];

impl Subscription {
    /// Reads a channel, its parts split by `/` or by `:`. `account_orders` as a frame names it
    /// (the market alone) is missing its account.
    pub fn parse(channel: &str) -> Result<Subscription, BadSubscription> {
        let mut parts = channel.split(['/', ':']);
        let named = parts.next().unwrap_or_default();
        let kind = KINDS
            .into_iter()
            .find(|kind| *kind == named)
            .ok_or_else(|| BadSubscription::Unknown(channel.to_owned()))?;
        let mut parts = Parts { parts, kind };
        let subscription = match kind {
            "order_book" => Subscription::OrderBook { market: parts.market()? },
            "ticker" => Subscription::Ticker { market: parts.market()? },
            "market_stats" => Subscription::MarketStats { market: parts.markets()? },
            "spot_market_stats" => Subscription::SpotMarketStats { market: parts.markets()? },
            "bo_market_stats" => Subscription::BoMarketStats { market: parts.markets()? },
            "trade" => Subscription::Trade { market: parts.market()? },
            "candle" => Subscription::Candle { market: parts.market()?, resolution: parts.resolution()? },
            "mark_price_candle" => {
                Subscription::MarkPriceCandle { market: parts.market()?, resolution: parts.resolution()? }
            }
            "up_down_candle" => Subscription::UpDownCandle {
                symbol: parts.word("symbol")?,
                duration: parts.word("duration")?,
                resolution: parts.up_down_resolution()?,
            },
            "height" => Subscription::Height,
            "account_all" => Subscription::AccountAll { account: parts.account()? },
            "account_market" => Subscription::AccountMarket { market: parts.market()?, account: parts.account()? },
            "user_stats" => Subscription::UserStats { account: parts.account()? },
            "account_tx" => Subscription::AccountTx { account: parts.account()? },
            "account_all_orders" => Subscription::AccountAllOrders { account: parts.account()? },
            "pool_data" => Subscription::PoolData { account: parts.account()? },
            "pool_info" => Subscription::PoolInfo { account: parts.account()? },
            "notification" => Subscription::Notification { account: parts.account()? },
            "account_orders" => Subscription::AccountOrders { market: parts.market()?, account: parts.account()? },
            "account_all_trades" => Subscription::AccountAllTrades { account: parts.account()? },
            "account_all_positions" => Subscription::AccountAllPositions { account: parts.account()? },
            "account_all_assets" => Subscription::AccountAllAssets { account: parts.account()? },
            "account_spot_avg_entry_prices" => Subscription::AccountSpotAvgEntryPrices { account: parts.account()? },
            "rfq" => Subscription::Rfq,
            "live_points" => Subscription::LivePoints { account: parts.account()? },
            _ => return Err(BadSubscription::Unknown(channel.to_owned())),
        };
        match parts.parts.next() {
            Some(_) => Err(BadSubscription::TooLong(channel.to_owned())),
            None => Ok(subscription),
        }
    }
}

/// The parts after a channel's name, read in turn for the `kind` it names.
struct Parts<'a, I: Iterator<Item = &'a str>> {
    parts: I,
    kind: &'static str,
}

impl<'a, I: Iterator<Item = &'a str>> Parts<'a, I> {
    fn next(&mut self, part: &'static str) -> Result<&'a str, BadSubscription> {
        self.parts.next().filter(|text| !text.is_empty()).ok_or(BadSubscription::Missing { kind: self.kind, part })
    }

    fn wrong(&self, part: &'static str, expected: &'static str) -> BadSubscription {
        BadSubscription::Wrong { kind: self.kind, part, expected }
    }

    fn market(&mut self) -> Result<MarketId, BadSubscription> {
        self.next("market")?.parse().map_err(|_| self.wrong("market", "a market id"))
    }

    /// A market, or `all`.
    fn markets(&mut self) -> Result<Option<MarketId>, BadSubscription> {
        match self.next("market")? {
            "all" => Ok(None),
            text => text.parse().map(Some).map_err(|_| self.wrong("market", "a market id or `all`")),
        }
    }

    fn account(&mut self) -> Result<AccountIndex, BadSubscription> {
        self.next("account")?.parse().map_err(|_| self.wrong("account", "an account index"))
    }

    fn resolution(&mut self) -> Result<Resolution, BadSubscription> {
        Resolution::parse(self.next("resolution")?)
            .ok_or_else(|| self.wrong("resolution", "1m, 5m, 15m, 30m, 1h, 4h, 12h or 1d"))
    }

    fn up_down_resolution(&mut self) -> Result<UpDownResolution, BadSubscription> {
        UpDownResolution::parse(self.next("resolution")?).ok_or_else(|| self.wrong("resolution", "1s or 30s"))
    }

    fn word(&mut self, part: &'static str) -> Result<String, BadSubscription> {
        self.next(part).map(ToOwned::to_owned)
    }
}
