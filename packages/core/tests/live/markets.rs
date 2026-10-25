//! What is listed and what it trades at, read from both venues: every read of
//! [`df_lighter_core::markets`], each held against its network's recording right after it is
//! read. Robinhood Chain lists no binary options: its `events` and `feeTiers` answer 403.

use df_lighter_core::Client;
use df_lighter_core::Network::{self, Lighter, Robinhood};
use df_lighter_core::client::http::codes;
use df_lighter_core::client::native::clock;
use df_lighter_core::client::{ApiError, TransportError};
use df_lighter_core::markets::{
    CandleQuery, EventQuery, Exchange, FeeFilter, MarketFilter, MetricKind, Period, Resolution, TradeQuery,
    UpDownResolution, announcements, asset_detail, asset_details, candle_window, candles, contract_address, events,
    exchange_metrics, exchange_stats, execute_stats, fee_tiers, funding_history, funding_rates, layer1_basic_info,
    listing, mark_price_candles, market_detail, market_price_charts, order_book_details, order_book_orders,
    order_books, price_oracle_info, recent_trades, stat_value, synthetic_spot_info, system_config, token_list, trades,
    up_down_candles, withdrawal_delay,
};
use df_lighter_core::utils::time::{DAY_MS, HOUR_MS};
use df_lighter_core::utils::{AccountIndex, AssetId, Dec, MarketId, Side};

use crate::tap::{Tap, reader, recording, same_shape, same_shape_as_any};

const BTC: MarketId = MarketId::of::<1>();

/// Each venue's first spot pair with a recorded book: ETH/USDC on Lighter, AAPL/USDG on
/// Robinhood Chain.
fn spot_pair(network: Network) -> (MarketId, &'static str) {
    match network {
        Lighter => (MarketId::of::<2048>(), "orderBookOrders_2048.json"),
        Robinhood => (MarketId::of::<2049>(), "orderBookOrders_2049.json"),
    }
}

fn is_forbidden(error: &ApiError) -> bool {
    matches!(error, ApiError::Transport(TransportError::Status { status: 403, .. }))
}

#[tokio::test]
#[ignore = "live"]
async fn every_market_is_listed_with_its_details_and_btc_reads_by_its_id() {
    for network in Network::ALL {
        let client = reader(network);
        let listed = order_books(&client, MarketFilter::All).await.unwrap();
        same_shape(&client, &recording(network, "orderBooks.json"));
        assert!(listed.iter().any(|row| (row.market_id, row.symbol.as_str()) == (BTC, "BTC")), "{network}");
        let details = order_book_details(&client, MarketFilter::All).await.unwrap();
        same_shape(&client, &recording(network, "orderBookDetails.json"));
        assert!(!details.perps.is_empty() && !details.spot.is_empty(), "{network}");
        let btc = details.by_symbol("BTC").unwrap();
        assert!(btc.is_perp() && btc.is_listed() && btc.mark_price.is_some(), "{network}: {btc:?}");
        let pair = details.get(spot_pair(network).0).unwrap();
        assert!(pair.is_spot() && pair.mark_price.is_none(), "{network}: {pair:?}");

        let one = listing(&client, BTC).await.unwrap().unwrap();
        same_shape(&client, &recording(network, "orderBooks.json"));
        assert_eq!(one.symbol, "BTC");
        let one = market_detail(&client, BTC).await.unwrap().unwrap();
        same_shape(&client, &recording(network, "orderBookDetails.json"));
        assert_eq!((one.symbol.as_str(), one.size_step(), one.tick()), ("BTC", btc.size_step(), btc.tick()));
    }
}

#[tokio::test]
#[ignore = "live"]
async fn btc_s_book_and_a_spot_pair_s_rest_on_both_sides() {
    for network in Network::ALL {
        let client = reader(network);
        let book = order_book_orders(&client, BTC, 10).await.unwrap();
        same_shape(&client, &recording(network, "orderBookOrders_1.json"));
        let (bid, ask) = book.touch();
        assert!(bid.unwrap() < ask.unwrap(), "{network}: {bid:?} {ask:?}");
        assert!(book.bids.len() <= 10 && book.asks.len() <= 10);
        let walk = book.walk(Side::Bid, book.asks[0].remaining_base_amount);
        assert_eq!((walk.complete, walk.worst), (true, ask), "{network}: the best ask fills its own size");

        let (pair, recorded) = spot_pair(network);
        let book = order_book_orders(&client, pair, 10).await.unwrap();
        same_shape(&client, &recording(network, recorded));
        if let (Some(bid), Some(ask)) = book.touch() {
            assert!(bid < ask, "{network}: {bid} {ask}");
        }
    }
}

/// Every recording of trades, on both venues: the venue leaves a trade's zero or false fields
/// out (`taker_fee`), so each holds some of them.
const TRADES: [&str; 4] =
    ["lighter/recentTrades_1.json", "lighter/trades_1.json", "robinhood/recentTrades.json", "robinhood/trades_1.json"];

/// What the docs' `Trade` lists that no recording caught, each written only when set: an
/// integrator's fees and their collector, an isolated position's margin before and after, whether
/// the maker's position flipped, and each side's realized PnL.
const TRADE_FIELDS_UNRECORDED: [&str; 11] = [
    "trades[].ask_account_pnl",
    "trades[].bid_account_pnl",
    "trades[].integrator_maker_fee",
    "trades[].integrator_maker_fee_collector_index",
    "trades[].integrator_taker_fee",
    "trades[].integrator_taker_fee_collector_index",
    "trades[].maker_allocated_margin_usdc_after",
    "trades[].maker_allocated_margin_usdc_before",
    "trades[].maker_position_sign_changed",
    "trades[].taker_allocated_margin_usdc_after",
    "trades[].taker_allocated_margin_usdc_before",
];

/// Holds trades against every recording of trades, `own` first: the live answer is kept as it.
fn same_trades(client: &Client<Tap>, own: &str) {
    let mut paths = vec![own];
    paths.extend(TRADES.iter().copied().filter(|path| *path != own));
    same_shape_as_any(client, &paths, &TRADE_FIELDS_UNRECORDED);
}

#[tokio::test]
#[ignore = "live"]
async fn the_tape_and_the_paged_history_read_newest_first() {
    for (network, tape) in [(Lighter, "recentTrades_1.json"), (Robinhood, "recentTrades.json")] {
        let client = reader(network);
        let prints = recent_trades(&client, BTC, 20).await.unwrap();
        same_trades(&client, &recording(network, tape));
        assert!(!prints.is_empty() && prints.iter().all(|print| print.market_id == BTC), "{network}");
        assert!(prints.windows(2).all(|pair| pair[0].timestamp >= pair[1].timestamp), "{network}");

        let query = TradeQuery::market(BTC, 5);
        let page = trades(&client, &query).await.unwrap();
        same_trades(&client, &recording(network, "trades_1.json"));
        assert_eq!(page.trades.len(), 5, "{network}");
        let next = trades(&client, &query.after(page.next().unwrap())).await.unwrap();
        same_trades(&client, &recording(network, "trades_1.json"));
        let (last, first) = (page.trades.last().unwrap(), next.trades.first().unwrap());
        assert!(first.timestamp <= last.timestamp && first.trade_id != last.trade_id, "{network}: the page goes on");

        let pool = trades(&client, &TradeQuery::pool(AccountIndex::LLP, 5)).await.unwrap();
        same_trades(&client, &recording(network, "trades_1.json"));
        assert!(pool.trades.iter().all(|fill| fill.side_of(AccountIndex::LLP).is_some()), "{network}: the LLP's own");
    }
}

#[tokio::test]
#[ignore = "live"]
async fn a_day_of_btc_candles_mark_candles_and_hourly_prices_read() {
    let now = clock();
    let window = candle_window(now - DAY_MS, now, Resolution::OneHour).unwrap();
    for (network, recorded) in [(Lighter, "candles_1.json"), (Robinhood, "candles.json")] {
        let client = reader(network);
        let bars = candles(&client, &CandleQuery::new(BTC, Resolution::OneHour, window)).await.unwrap();
        same_shape(&client, &recording(network, recorded));
        assert!(bars.len() >= 23, "{network}: {} candles", bars.len());
        assert!(bars.windows(2).all(|pair| pair[0].open_time + HOUR_MS == pair[1].open_time), "{network}");
        assert!(bars.iter().all(|candle| candle.bar().is_some_and(|bar| bar.low <= bar.high)), "{network}");

        let marks = mark_price_candles(&client, BTC, Resolution::OneHour, window).await.unwrap();
        same_shape(&client, &recording(network, "markPriceCandles_1.json"));
        assert!(marks.len() >= 23 && marks.iter().all(|mark| mark.samples > 0), "{network}");

        let charts = market_price_charts(&client, &[BTC]).await.unwrap();
        same_shape(&client, &recording(network, "marketPriceCharts.json"));
        assert_eq!(charts.iter().map(|chart| chart.market_id).collect::<Vec<_>>(), [BTC], "{network}");
        assert!(charts[0].prices.iter().all(|price| *price > Dec::ZERO), "{network}");
    }
}

#[tokio::test]
#[ignore = "live"]
async fn funding_reads_each_hour_s_settlement_and_every_venue_s_rate() {
    let now = clock();
    for network in Network::ALL {
        let client = reader(network);
        let settled = funding_history(&client, BTC, now - DAY_MS, now).await.unwrap();
        same_shape(&client, &recording(network, "fundings_1.json"));
        assert!(settled.len() >= 23, "{network}: {} hours", settled.len());
        assert!(settled.windows(2).all(|pair| pair[0].timestamp + 3_600 == pair[1].timestamp), "{network}");

        let rates = funding_rates(&client).await.unwrap();
        same_shape(&client, &recording(network, "funding-rates.json"));
        let btc = rates.iter().find(|rate| rate.exchange == Exchange::Lighter && rate.market_id == BTC);
        assert!(btc.is_some_and(|rate| rate.hourly().is_some()), "{network}: {btc:?}");
    }
}

#[tokio::test]
#[ignore = "live"]
async fn assets_tokens_and_oracles_read_and_a_synthetic_spot_names_its_carry() {
    for network in Network::ALL {
        let client = reader(network);
        let assets = asset_details(&client).await.unwrap();
        same_shape(&client, &recording(network, "assetDetails.json"));
        let collateral = network.collateral();
        assert!(assets.iter().any(|asset| asset.asset_id == collateral.asset), "{network}");
        let one = asset_detail(&client, AssetId::COLLATERAL).await.unwrap().unwrap();
        same_shape(&client, &recording(network, "assetDetails.json"));
        assert_eq!((one.symbol.as_str(), one.decimals), (collateral.symbol, collateral.decimals), "{network}");

        let tokens = token_list(&client).await.unwrap();
        same_shape(&client, &recording(network, "tokenlist.json"));
        assert!(tokens.iter().any(|token| token.symbol == "BTC"), "{network}");

        let oracles = price_oracle_info(&client, Some("BTC")).await.unwrap();
        same_shape(&client, &recording(network, "priceOracleInfo.json"));
        assert_eq!(oracles.markets.iter().map(|market| market.symbol.as_str()).collect::<Vec<_>>(), ["BTC"]);
    }

    let client = reader(Lighter);
    let us500 = synthetic_spot_info(&client, "US500").await.unwrap();
    same_shape(&client, "lighter/syntheticSpotInfo_US500.json");
    assert!(!us500.symbol.is_empty() && stat_value(&us500.bps_per_day).is_some(), "{us500:?}");
    let client = reader(Robinhood);
    let refused = synthetic_spot_info(&client, "SPY").await.unwrap_err();
    same_shape(&client, "robinhood/syntheticSpotInfo_SPY.json");
    assert_eq!(refused.code(), Some(codes::INVALID_PARAM), "{refused:?}");
}

#[tokio::test]
#[ignore = "live"]
async fn the_exchange_s_day_its_history_and_its_slippage_read() {
    for network in Network::ALL {
        let client = reader(network);
        let day = exchange_stats(&client).await.unwrap();
        same_shape(&client, &recording(network, "exchangeStats.json"));
        assert_eq!(usize::try_from(day.total).unwrap(), day.order_book_stats.len(), "{network}");
        assert!(stat_value(&day.daily_usd_volume).is_some_and(|volume| volume > Dec::ZERO), "{network}");

        let week = exchange_metrics(&client, Period::Week, MetricKind::Volume, None).await.unwrap();
        same_shape(&client, &recording(network, "exchangeMetrics_w.json"));
        assert!(!week.is_empty(), "{network}");
        assert!(week.windows(2).all(|pair| pair[0].timestamp + 86_400 == pair[1].timestamp), "a point a day");

        let slippage = execute_stats(&client, Period::Day).await.unwrap();
        same_shape(&client, &recording(network, "executeStats.json"));
        assert!(slippage.iter().any(|stat| !stat.slippage.is_empty()), "{network}");
    }

    let client = reader(Lighter);
    let refused = exchange_metrics(&client, Period::Day, MetricKind::Volume, None).await.unwrap_err();
    same_shape(&client, "lighter/exchangeMetrics.json");
    assert_eq!(refused.code(), Some(codes::INVALID_PARAM), "volume by the day is not offered: {refused:?}");
}

#[tokio::test]
#[ignore = "live"]
async fn fee_tiers_read_on_lighter_and_robinhood_chain_refuses_them() {
    let client = reader(Lighter);
    let all = fee_tiers(&client, FeeFilter::All).await.unwrap();
    same_shape(&client, "lighter/feeTiers.json");
    assert!(!all.user_tiers.is_empty() && !all.bo_tiers.is_empty() && !all.staking_tiers.is_empty());
    let binary = fee_tiers(&client, FeeFilter::BinaryOptions).await.unwrap();
    same_shape(&client, "lighter/feeTiers_bo.json");
    assert!(binary.user_tiers.is_empty() && !binary.bo_tiers.is_empty(), "{binary:?}");

    let refused = fee_tiers(&reader(Robinhood), FeeFilter::All).await.unwrap_err();
    assert!(is_forbidden(&refused), "{refused:?}");
}

#[tokio::test]
#[ignore = "live"]
async fn each_venue_names_its_pools_contract_l1_withdrawal_delay_and_news() {
    for network in Network::ALL {
        let client = reader(network);
        let config = system_config(&client).await.unwrap();
        same_shape(&client, &recording(network, "systemConfig.json"));
        let pools = (config.liquidity_pool(), config.staking_pool().map(AccountIndex::get));
        match network {
            Lighter => assert_eq!(pools, (Some(AccountIndex::LLP), Some(281_474_976_624_800))),
            Robinhood => assert_eq!(pools, (None, None), "2^48 - 1: none"),
        }

        assert_eq!(contract_address(&client).await, Ok(network.contract()), "{network}");
        same_shape(&client, &recording(network, "info.json"));
        let l1 = layer1_basic_info(&client).await.unwrap();
        same_shape(&client, &recording(network, "layer1BasicInfo.json"));
        assert_eq!(
            (l1.chain_id(), l1.contract("ZkLighterContract")),
            (Some(network.l1_chain_id()), Some(network.contract()))
        );
        assert_eq!(l1.contract("USDCContract"), Some(network.collateral().token), "{network}");

        assert!(withdrawal_delay(&client).await.unwrap() > 0, "{network}");
        same_shape(&client, &recording(network, "withdrawalDelay.json"));
        let news = announcements(&client).await.unwrap();
        same_shape(&client, &recording(network, "announcement.json"));
        assert!(!news.is_empty() && news.windows(2).all(|pair| pair[0].created_at >= pair[1].created_at));
    }
}

#[tokio::test]
#[ignore = "live"]
async fn binary_options_list_their_events_and_the_price_they_settle_on_lighter_only() {
    let client = reader(Lighter);
    let page = events(&client, &EventQuery::tradable(2).series("btc-updown-15m")).await.unwrap();
    same_shape(&client, "lighter/events.json");
    let event = page.events.first().unwrap();
    assert_eq!((event.tag.as_str(), event.metadata.underlying_market_id), ("btc-updown-15m", Some(BTC)));
    assert!(!event.order_books.is_empty() && event.start_at < event.end_at, "{event:?}");

    let now_s = clock() / 1_000;
    let page = up_down_candles(&client, "btc", "15m", UpDownResolution::ThirtySeconds, now_s - 600, now_s - 450, None)
        .await
        .unwrap();
    same_shape(&client, "lighter/upDownCandles.json");
    assert!(!page.candles.is_empty(), "{page:?}");
    assert!(page.candles.iter().all(|candle| candle.low <= candle.open.min(candle.close) && candle.high >= candle.low));

    let refused = events(&reader(Robinhood), &EventQuery::default()).await.unwrap_err();
    assert!(is_forbidden(&refused), "{refused:?}");
}
