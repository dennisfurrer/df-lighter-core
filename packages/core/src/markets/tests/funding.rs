//! BTC's settlements over a day and every venue's current rates, both networks, recorded on
//! 2026-10-25: units by name, the sign from the side that paid, and an hour's rate from the eight
//! hours' as the venue writes it.

use super::*;
use crate::test_support::{Recorded, block_on};

const LIGHTER_FUNDINGS: &str = include_str!("../../../fixtures/lighter/fundings_1.json");
const ROBINHOOD_FUNDINGS: &str = include_str!("../../../fixtures/robinhood/fundings_1.json");

/// 2026-10-24 09:00 UTC and the last hour the recordings cover.
const START_MS: u64 = 1_792_832_400_000;
const LAST_MS: u64 = 1_792_915_200_000;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn btc() -> MarketId {
    MarketId::of::<1>()
}

#[test]
fn a_day_of_settlements_reads_on_both_networks() {
    let transport = Recorded::answering(LIGHTER_FUNDINGS);
    let lighter =
        block_on(fundings(&transport, btc(), FundingResolution::Hour, 1_792_832_400, 1_792_918_800, 24)).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/fundings".to_owned(), false));
    let fields = [
        ("market_id", "1"),
        ("resolution", "1h"),
        ("start_timestamp", "1792832400"),
        ("end_timestamp", "1792918800"),
        ("count_back", "24"),
    ];
    assert_eq!(call.fields, fields.map(|(key, value)| (key, value.to_owned())));
    let first = &lighter[0];
    assert_eq!((lighter.len(), first.timestamp, first.at_ms()), (24, 1_792_832_400, START_MS));
    assert_eq!((first.value, first.rate, first.payer), (dec("0.9685512"), dec("0.0012"), Payer::Long));
    assert_eq!(first.fraction(), Some(dec("0.000012")), "0.0012% of the notional, longs paying");
    assert!(lighter.windows(2).all(|pair| pair[0].timestamp + 3_600 == pair[1].timestamp), "every hour, oldest first");

    let robinhood =
        block_on(fundings(&Recorded::answering(ROBINHOOD_FUNDINGS), btc(), FundingResolution::Hour, 0, 0, 24));
    let robinhood = robinhood.unwrap();
    assert_eq!(
        (robinhood.len(), robinhood[0].rate, robinhood[0].fraction()),
        (23, dec("0.0009"), Some(dec("0.000009")))
    );
    assert_eq!(fundings_call(btc(), FundingResolution::Day, 0, 1, 10_000).value("count_back"), Some("750"));
}

#[test]
fn a_settlement_shorts_paid_is_negative() {
    let row: Funding =
        serde_json::from_str(r#"{"timestamp":1792832400,"value":"0.4","rate":"0.0005","direction":"short"}"#).unwrap();
    assert_eq!((row.payer, row.fraction()), (Payer::Short, Some(dec("-0.000005"))));
    let odd: Funding = serde_json::from_str(r#"{"timestamp":1,"value":"0","rate":"0","direction":"both"}"#).unwrap();
    assert_eq!((odd.payer, odd.fraction()), (Payer::Other, None));
}

#[test]
fn a_range_is_asked_in_seconds_with_one_hour_to_spare_and_cut_to_the_range() {
    let transport = Recorded::answering(LIGHTER_FUNDINGS);
    let history = block_on(funding_history(&transport, btc(), START_MS + 1, LAST_MS)).unwrap();
    assert_eq!((history.len(), history[0].at_ms()), (23, START_MS + 3_600_000), "the first hour is before the range");
    let call = transport.only();
    assert_eq!((call.value("start_timestamp"), call.value("end_timestamp")), (Some("1792832400"), Some("1792915200")));
    assert_eq!(call.value("count_back"), Some("24"), "22 whole hours, plus two");
    assert_eq!(funding_count_back(START_MS, LAST_MS), 25);
    assert_eq!((funding_count_back(5, 5), funding_count_back(0, 10_000 * HOUR_MS)), (2, FUNDINGS_PER_CALL));
    assert_eq!(next_funding(START_MS), START_MS + HOUR_MS);
    assert_eq!(next_funding(START_MS + 1), START_MS + HOUR_MS);
}

#[test]
fn every_venue_s_rate_reads_and_lighter_s_hour_is_an_eighth_of_its_eight() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/funding-rates.json"));
    let lighter = block_on(funding_rates(&transport)).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/funding-rates");
    assert_eq!(lighter.len(), 735);
    let count = |rates: &[FundingRate], exchange| rates.iter().filter(|rate| rate.exchange == exchange).count();
    assert_eq!((count(&lighter, Exchange::Lighter), count(&lighter, Exchange::Hyperliquid)), (214, 134));
    let btc_rate = lighter.iter().find(|rate| rate.exchange == Exchange::Lighter && rate.market_id == btc()).unwrap();
    assert_eq!((btc_rate.symbol.as_str(), btc_rate.rate.as_str()), ("BTC", "0.00009599999999999999"));
    assert_eq!((btc_rate.eight_hour(), btc_rate.hourly()), (Some(dec("0.000096")), Some(dec("0.000012"))));
    let settled = Funding { timestamp: 0, value: Dec::ZERO, rate: dec("0.0012"), payer: Payer::Long };
    assert_eq!(btc_rate.hourly(), settled.fraction(), "the rate an hour settles at");
    let tiny = lighter.iter().find(|rate| rate.rate.as_str().contains('e')).unwrap();
    assert!(tiny.eight_hour().is_some(), "{} reads with its exponent", tiny.rate);

    let robinhood =
        block_on(funding_rates(&Recorded::answering(include_str!("../../../fixtures/robinhood/funding-rates.json"))));
    let robinhood = robinhood.unwrap();
    assert_eq!((robinhood.len(), count(&robinhood, Exchange::Lighter)), (204, 63));
    let btc_rate = robinhood.iter().find(|rate| rate.exchange == Exchange::Lighter && rate.market_id == btc()).unwrap();
    assert_eq!(btc_rate.hourly(), Some(dec("0.00001")));
}
