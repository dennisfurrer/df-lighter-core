//! Every channel the docs list, named as a subscribe names it and as its frames do, read back
//! either way, with the docs' word on which need the auth token.

use super::*;
use crate::live::fixtures::{every, lighter};

fn market(id: u64) -> MarketId {
    MarketId::new(id).unwrap()
}

fn account(index: u64) -> AccountIndex {
    AccountIndex::new(index).unwrap()
}

/// Each channel as the docs' examples write it, whether it needs the token, and the channel its
/// frames name.
fn every_channel() -> Vec<(Subscription, &'static str, bool, &'static str)> {
    vec![
        (Subscription::OrderBook { market: market(0) }, "order_book/0", false, "order_book:0"),
        (Subscription::Ticker { market: market(0) }, "ticker/0", false, "ticker:0"),
        (Subscription::MarketStats { market: Some(market(0)) }, "market_stats/0", false, "market_stats:0"),
        (Subscription::MarketStats { market: None }, "market_stats/all", false, "market_stats:all"),
        (
            Subscription::SpotMarketStats { market: Some(market(2048)) },
            "spot_market_stats/2048",
            false,
            "spot_market_stats:2048",
        ),
        (Subscription::SpotMarketStats { market: None }, "spot_market_stats/all", false, "spot_market_stats:all"),
        (Subscription::BoMarketStats { market: None }, "bo_market_stats/all", false, "bo_market_stats:all"),
        (Subscription::Trade { market: market(0) }, "trade/0", false, "trade:0"),
        (
            Subscription::Candle { market: market(0), resolution: Resolution::OneMinute },
            "candle/0/1m",
            false,
            "candle:0:1m",
        ),
        (
            Subscription::MarkPriceCandle { market: market(0), resolution: Resolution::OneMinute },
            "mark_price_candle/0/1m",
            false,
            "mark_price_candle:0:1m",
        ),
        (
            Subscription::UpDownCandle {
                symbol: "eth".to_owned(),
                duration: "15m".to_owned(),
                resolution: UpDownResolution::Second,
            },
            "up_down_candle/eth/15m/1s",
            false,
            "up_down_candle:eth:15m:1s",
        ),
        (Subscription::Height, "height", false, "height"),
        (Subscription::AccountAll { account: account(1) }, "account_all/1", false, "account_all:1"),
        (
            Subscription::AccountMarket { market: market(0), account: account(40) },
            "account_market/0/40",
            true,
            "account_market:0:40",
        ),
        (Subscription::UserStats { account: account(1234) }, "user_stats/1234", false, "user_stats:1234"),
        (Subscription::AccountTx { account: account(1) }, "account_tx/1", true, "account_tx:1"),
        (Subscription::AccountAllOrders { account: account(1) }, "account_all_orders/1", true, "account_all_orders:1"),
        (Subscription::PoolData { account: account(1) }, "pool_data/1", false, "pool_data:1"),
        (Subscription::PoolInfo { account: account(1) }, "pool_info/1", false, "pool_info:1"),
        (Subscription::Notification { account: account(1) }, "notification/1", true, "notification:1"),
        (
            Subscription::AccountOrders { market: market(0), account: account(1) },
            "account_orders/0/1",
            true,
            "account_orders:0",
        ),
        (Subscription::AccountAllTrades { account: account(1) }, "account_all_trades/1", false, "account_all_trades:1"),
        (
            Subscription::AccountAllPositions { account: account(1) },
            "account_all_positions/1",
            false,
            "account_all_positions:1",
        ),
        (
            Subscription::AccountAllAssets { account: account(1234) },
            "account_all_assets/1234",
            true,
            "account_all_assets:1234",
        ),
        (
            Subscription::AccountSpotAvgEntryPrices { account: account(1234) },
            "account_spot_avg_entry_prices/1234",
            true,
            "account_spot_avg_entry_prices:1234",
        ),
        (Subscription::Rfq, "rfq", true, "rfq"),
        (Subscription::LivePoints { account: account(1234) }, "live_points/1234", true, "live_points:1234"),
    ]
}

#[test]
fn every_channel_is_named_as_the_docs_write_it_and_read_back_either_way() {
    for (subscription, channel, needs_auth, answered) in every_channel() {
        assert_eq!(subscription.channel(), channel);
        assert_eq!(subscription.to_string(), channel);
        assert_eq!(subscription.needs_auth(), needs_auth, "{channel}");
        assert_eq!(subscription.answered_on(), answered, "{channel}");
        assert_eq!(Subscription::parse(channel), Ok(subscription.clone()), "{channel}");
        assert!(subscription.channel().starts_with(subscription.kind()), "{channel}");
        if !matches!(subscription, Subscription::AccountOrders { .. }) {
            assert_eq!(Subscription::parse(answered), Ok(subscription.clone()), "{answered}");
        }
    }
    let kinds: std::collections::BTreeSet<_> = every_channel().iter().map(|(each, ..)| each.kind()).collect();
    assert_eq!(kinds.len(), 25, "every channel the docs list");
}

/// What every recorded frame names reads back to a channel: the venue writes them as the docs do.
#[test]
fn every_recorded_channel_reads_back() {
    for network in Network::ALL {
        for line in every(network) {
            let Some((_, rest)) = line.split_once(r#""channel":""#) else { continue };
            let channel = rest.split('"').next().unwrap();
            let subscription = Subscription::parse(channel).unwrap();
            assert_eq!(subscription.answered_on(), channel, "{network}");
        }
    }
    assert!(lighter("subscribed/up_down_candle", "").contains("up_down_candle:btc:15m:1s"));
}

/// A pool's channel carries the token when there is one: a public pool needs none, another
/// account's pool its own. The rest that take it need it.
#[test]
fn pools_take_the_token_without_needing_it() {
    for (subscription, ..) in every_channel() {
        let pool = matches!(subscription, Subscription::PoolData { .. } | Subscription::PoolInfo { .. });
        assert_eq!(subscription.takes_auth(), subscription.needs_auth() || pool);
    }
}

#[test]
fn what_is_not_a_channel_says_why() {
    let wrong = |kind, part, expected| BadSubscription::Wrong { kind, part, expected };
    assert_eq!(Subscription::parse("no_such_channel/1"), Err(BadSubscription::Unknown("no_such_channel/1".to_owned())));
    assert_eq!(Subscription::parse("order_book"), Err(BadSubscription::Missing { kind: "order_book", part: "market" }));
    assert_eq!(Subscription::parse("order_book/255"), Err(wrong("order_book", "market", "a market id")));
    assert_eq!(
        Subscription::parse("candle/1/2m"),
        Err(wrong("candle", "resolution", "1m, 5m, 15m, 30m, 1h, 4h, 12h or 1d"))
    );
    assert_eq!(Subscription::parse("height/1"), Err(BadSubscription::TooLong("height/1".to_owned())));
    assert_eq!(
        Subscription::parse("account_orders:1"),
        Err(BadSubscription::Missing { kind: "account_orders", part: "account" }),
        "as its frames name it"
    );
}

/// Binary options are Lighter's, live points Robinhood Chain's; an Up/Down series names a plain
/// symbol and window.
#[test]
fn a_channel_another_network_serves_is_refused() {
    let options = Subscription::BoMarketStats { market: None };
    let points = Subscription::LivePoints { account: account(1) };
    assert_eq!(options.check(Network::Lighter), Ok(()));
    assert_eq!(
        options.check(Network::Robinhood),
        Err(BadSubscription::OtherNetwork { kind: "bo_market_stats", network: Network::Robinhood })
    );
    assert_eq!(points.check(Network::Robinhood), Ok(()));
    assert!(points.check(Network::Lighter).is_err());
    let series = |symbol: &str, duration: &str| Subscription::UpDownCandle {
        symbol: symbol.to_owned(),
        duration: duration.to_owned(),
        resolution: UpDownResolution::ThirtySeconds,
    };
    assert_eq!(series("btc", "15m").check(Network::Lighter), Ok(()));
    assert!(series("btc/eth", "15m").check(Network::Lighter).is_err());
    assert!(series("btc", "").check(Network::Lighter).is_err());
}

#[test]
fn a_subscription_names_its_account_and_its_one_market() {
    assert_eq!(Subscription::AccountOrders { market: market(3), account: account(7) }.account(), Some(account(7)));
    assert_eq!(Subscription::AccountOrders { market: market(3), account: account(7) }.market(), Some(market(3)));
    assert_eq!(Subscription::MarketStats { market: None }.market(), None);
    assert_eq!(Subscription::Height.account(), None);
}
