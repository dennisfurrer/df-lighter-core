//! Writing a subscription as the channel a subscribe names ([`Subscription::channel`]), and as
//! the channel its frames come back on ([`Subscription::answered_on`]).

use std::fmt;

use crate::live::Subscription;

impl Subscription {
    /// The channel a subscribe or an unsubscribe names: its parts joined by `/`.
    #[must_use]
    pub fn channel(&self) -> String {
        self.parts('/')
    }

    /// The channel its frames name: its parts joined by `:`, except `account_orders`, whose
    /// frames name the market alone (`account_orders:1`) and carry the account inside.
    #[must_use]
    pub fn answered_on(&self) -> String {
        match self {
            Subscription::AccountOrders { market, .. } => format!("account_orders:{market}"),
            other => other.parts(':'),
        }
    }

    fn parts(&self, between: char) -> String {
        let kind = self.kind();
        let all = |market: &Option<crate::utils::MarketId>| match market {
            Some(market) => market.to_string(),
            None => "all".to_owned(),
        };
        match self {
            Subscription::Height | Subscription::Rfq => kind.to_owned(),
            Subscription::OrderBook { market } | Subscription::Ticker { market } | Subscription::Trade { market } => {
                format!("{kind}{between}{market}")
            }
            Subscription::MarketStats { market }
            | Subscription::SpotMarketStats { market }
            | Subscription::BoMarketStats { market } => format!("{kind}{between}{}", all(market)),
            Subscription::Candle { market, resolution } | Subscription::MarkPriceCandle { market, resolution } => {
                format!("{kind}{between}{market}{between}{resolution}")
            }
            Subscription::UpDownCandle { symbol, duration, resolution } => {
                format!("{kind}{between}{symbol}{between}{duration}{between}{resolution}")
            }
            Subscription::AccountMarket { market, account } | Subscription::AccountOrders { market, account } => {
                format!("{kind}{between}{market}{between}{account}")
            }
            Subscription::AccountAll { account }
            | Subscription::UserStats { account }
            | Subscription::AccountTx { account }
            | Subscription::AccountAllOrders { account }
            | Subscription::PoolData { account }
            | Subscription::PoolInfo { account }
            | Subscription::Notification { account }
            | Subscription::AccountAllTrades { account }
            | Subscription::AccountAllPositions { account }
            | Subscription::AccountAllAssets { account }
            | Subscription::AccountSpotAvgEntryPrices { account }
            | Subscription::LivePoints { account } => format!("{kind}{between}{account}"),
        }
    }
}

impl fmt::Display for Subscription {
    /// The channel a subscribe names: `order_book/1`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.channel())
    }
}
