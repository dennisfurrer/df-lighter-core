//! The exchange's figures on both networks, recorded on 2026-10-25: the day's stats, a week of
//! volume, a day's slippage (Robinhood Chain's cut to two days of six rows), the fee tiers, and
//! floats read as the decimals meant.

use super::*;
use crate::client::TransportError;
use crate::client::http::codes;
use crate::test_support::{Recorded, block_on};

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn figure(text: &str) -> Figure {
    serde_json::from_str(text).unwrap()
}

#[test]
fn a_float_reads_as_the_decimal_the_venue_meant() {
    let cases = [
        ("349.8602000000589", "349.8602"),
        ("0.00009599999999999999", "0.000096"),
        ("268132.56497300003", "268132.564973"),
        ("-0.15943204968820798", "-0.159432049688"),
        ("6.968e-7", "0.0000006968"),
        ("80687", "80687"),
        ("0", "0"),
        ("\"0.000000\"", "0"),
        ("-0.0000000000000000004", "0"),
        ("-0.0000000000000000006", "-0.000000000000000001"),
    ];
    for (written, meant) in cases {
        assert_eq!(stat_value(&figure(written)), Some(dec(meant)), "{written}");
    }
}

#[test]
fn the_day_s_stats_read_on_both_networks() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/exchangeStats.json"));
    let lighter = block_on(exchange_stats(&transport)).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/exchangeStats");
    assert_eq!((lighter.total, lighter.order_book_stats.len(), lighter.daily_trades_count), (440, 440, 1_792_299));
    assert_eq!(stat_value(&lighter.daily_usd_volume), Some(dec("1450887162.28")));
    let updown = lighter.order_book_stats.iter().filter(|stats| stats.symbol.contains("-UPDOWN-")).count();
    assert_eq!(updown, 194, "binary options among them");
    let btc = lighter.order_book_stats.iter().find(|stats| stats.symbol == "BTC").unwrap();
    assert_eq!((btc.daily_trades_count, btc.last_trade_price.to_dec()), (692_270, Some(dec("80458.8"))));

    let robinhood =
        block_on(exchange_stats(&Recorded::answering(include_str!("../../../fixtures/robinhood/exchangeStats.json"))));
    let robinhood = robinhood.unwrap();
    assert_eq!((robinhood.total, robinhood.daily_trades_count), (90, 1_175_940));
    assert_eq!(stat_value(&robinhood.daily_usd_volume), Some(dec("467030682.133")));
}

#[test]
fn a_metric_s_history_reads_and_a_period_it_does_not_suit_is_refused() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/exchangeMetrics_w.json"));
    let week = block_on(exchange_metrics(&transport, Period::Week, MetricKind::Volume, None)).unwrap();
    assert_eq!(transport.only().fields, [("period", "w".to_owned()), ("kind", "volume".to_owned())]);
    assert_eq!((week.len(), week[0].timestamp, week[0].data.to_dec()), (7, 1_792_281_600, Some(dec("1683084872.8"))));
    assert!(week.windows(2).all(|pair| pair[0].timestamp + 86_400 == pair[1].timestamp), "a point a day");

    let transport = Recorded::answering(include_str!("../../../fixtures/robinhood/exchangeMetrics_w.json"));
    let week = block_on(exchange_metrics(&transport, Period::Week, MetricKind::Volume, None)).unwrap();
    assert_eq!((week.len(), week[0].data.as_str()), (7, "479444711.81"));

    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/exchangeMetrics.json"));
    let refused = block_on(exchange_metrics(&transport, Period::Day, MetricKind::Volume, None)).unwrap_err();
    assert_eq!(refused.code(), Some(codes::INVALID_PARAM));

    let call = exchange_metrics_call(Period::All, MetricKind::OpenInterest, Some("BTC"));
    let fields = [("period", "all"), ("kind", "open_interest"), ("filter", "byMarket"), ("value", "BTC")];
    assert_eq!(call.fields, fields.map(|(key, value)| (key, value.to_owned())));
}

#[test]
fn slippage_against_other_venues_reads_on_both_networks() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/executeStats.json"));
    let lighter = block_on(execute_stats(&transport, Period::Day)).unwrap();
    assert_eq!(transport.only().fields, [("period", "d".to_owned())]);
    assert_eq!((lighter.len(), lighter[0].timestamp, lighter[0].slippage.len()), (9, 1_792_886_400, 200));
    let ours = &lighter[0].slippage[4];
    assert_eq!(
        (ours.exchange.as_str(), ours.market.as_str(), ours.size_usd, ours.data_count),
        ("lighter", "NATGAS", 10_000, 60)
    );
    assert_eq!(ours.avg_slippage.to_dec(), Some(dec("5.4567")));

    let robinhood = block_on(execute_stats(
        &Recorded::answering(include_str!("../../../fixtures/robinhood/executeStats.json")),
        Period::Day,
    ));
    let robinhood = robinhood.unwrap();
    assert_eq!((robinhood.len(), robinhood[0].slippage[4].market.as_str()), (2, "XAG"));
}

#[test]
fn the_fee_tiers_read_on_lighter_and_robinhood_chain_has_none() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/feeTiers.json"));
    let all = block_on(fee_tiers(&transport, FeeFilter::All)).unwrap();
    assert_eq!(transport.only().fields, [("filter", "all".to_owned())]);
    assert_eq!((all.user_tiers.len(), all.bo_tiers.len(), all.staking_tiers.len()), (10, 8, 7));
    let premium = &all.user_tiers[2];
    assert_eq!(
        (premium.name.as_str(), premium.level, premium.maker_fee_ticks, premium.taker_fee_ticks),
        ("premium", 4, 40, 280)
    );
    assert_eq!((all.user_tiers[9].required_lit_stakes, all.bo_tiers[7].fee_rate.to_dec()), (500_000, Some(dec("0.6"))));

    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/feeTiers_bo.json"));
    let binary = block_on(fee_tiers(&transport, FeeFilter::BinaryOptions)).unwrap();
    assert_eq!(transport.only().value("filter"), Some("bo"));
    assert_eq!(
        (binary.user_tiers.len(), binary.bo_tiers.len(), binary.staking_tiers[0].fee_rate.as_str()),
        (0, 8, "0.975")
    );

    let transport = Recorded::failing(TransportError::Status { status: 403, body: String::new() });
    let refused = block_on(fee_tiers(&transport, FeeFilter::Perps)).unwrap_err();
    assert_eq!(refused, ApiError::Transport(TransportError::Status { status: 403, body: String::new() }));
    assert_eq!((FeeFilter::Perps.wire(), FeeFilter::Spot.wire()), ("perps", "spot"));
}
