//! A day of hourly BTC candles on both networks (trades and mark), and the day's hourly prices,
//! recorded on 2026-10-25; the windows a long range is read in.

use super::*;
use crate::test_support::{Recorded, block_on};

const LIGHTER_CANDLES: &str = include_str!("../../../fixtures/lighter/candles_1.json");
const ROBINHOOD_CANDLES: &str = include_str!("../../../fixtures/robinhood/candles.json");

/// The day both networks' history fixtures cover: 24 hours from 2026-10-24 09:00 UTC.
const START_MS: u64 = 1_792_832_400_000;
const LAST_OPEN_MS: u64 = 1_792_915_200_000;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn btc() -> MarketId {
    MarketId::of::<1>()
}

fn day() -> CandleWindow {
    CandleWindow { start_ms: START_MS, end_ms: LAST_OPEN_MS + HOUR_MS, count: 24 }
}

#[test]
fn a_day_of_candles_reads_on_both_networks_as_bars() {
    let query = CandleQuery::new(btc(), Resolution::OneHour, day());
    let transport = Recorded::answering(LIGHTER_CANDLES);
    let lighter = block_on(candles(&transport, &query)).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/candles".to_owned(), false));
    let fields = [
        ("market_id", "1"),
        ("resolution", "1h"),
        ("start_timestamp", "1792832400000"),
        ("end_timestamp", "1792918800000"),
        ("count_back", "24"),
    ];
    assert_eq!(call.fields, fields.map(|(key, value)| (key, value.to_owned())));
    assert_eq!((lighter.len(), lighter[0].open_time, lighter[23].open_time), (24, START_MS, LAST_OPEN_MS));
    assert!(lighter.windows(2).all(|pair| pair[0].open_time + HOUR_MS == pair[1].open_time), "back to back");
    let first = &lighter[0];
    assert_eq!(first.volume.as_ref().map(Figure::as_str), Some("349.8602000000589"), "the float as written");
    assert_eq!(first.last_trade_id, 36_297_823_734);
    let bar = first.bar().unwrap();
    assert_eq!(
        (bar.open, bar.high, bar.low, bar.close),
        (dec("80687"), dec("80689.4"), dec("80410.7"), dec("80598.6"))
    );
    assert_eq!((bar.volume, bar.quote_volume), (dec("349.8602"), dec("28183816.304")));

    let robinhood = block_on(candles(&Recorded::answering(ROBINHOOD_CANDLES), &query)).unwrap();
    let bar = robinhood[0].bar().unwrap();
    assert_eq!((robinhood.len(), bar.open, bar.close, bar.volume), (24, dec("80666"), dec("80586.6"), dec("39.95728")));
}

#[test]
fn a_figure_left_out_was_zero_and_a_bar_without_a_close_is_none() {
    let candle: Candle = serde_json::from_str(r#"{"t":1792832400000,"c":80598.6}"#).unwrap();
    let bar = candle.bar().unwrap();
    assert_eq!(
        (bar.open, bar.high, bar.low, bar.close),
        (dec("80598.6"), dec("80598.6"), dec("80598.6"), dec("80598.6"))
    );
    assert_eq!((bar.volume, bar.quote_volume, candle.last_trade_id), (Dec::ZERO, Dec::ZERO, 0));
    let open_only: Candle = serde_json::from_str(r#"{"t":1792832400000,"o":2,"c":3}"#).unwrap();
    let bar = open_only.bar().unwrap();
    assert_eq!((bar.high, bar.low), (dec("3"), dec("2")));
    let none: Candle = serde_json::from_str(r#"{"t":1792832400000,"o":2,"v":1}"#).unwrap();
    assert_eq!(none.bar(), None);
}

#[test]
fn the_switches_are_sent_only_when_set() {
    let query = CandleQuery::new(btc(), Resolution::FiveMinutes, day()).stamped_at_end().with_mark_price();
    let call = candles_call(&query);
    assert_eq!((call.value("set_timestamp_to_end"), call.value("merge_mark_price")), (Some("true"), Some("true")));
    let plain = candles_call(&CandleQuery::new(btc(), Resolution::OneDay, day()));
    assert_eq!((plain.value("set_timestamp_to_end"), plain.value("resolution")), (None, Some("1d")));
    let many = CandleWindow { count: 2_000, ..day() };
    assert_eq!(candles_call(&CandleQuery::new(btc(), Resolution::OneHour, many)).value("count_back"), Some("500"));
}

#[test]
fn mark_price_candles_read_on_both_networks() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/markPriceCandles_1.json"));
    let lighter = block_on(mark_price_candles(&transport, btc(), Resolution::OneHour, day())).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/markPriceCandles");
    let first = &lighter[0];
    assert_eq!((lighter.len(), first.open_time, first.samples), (24, START_MS, 524_525));
    assert_eq!((first.open.as_str(), first.close.as_str()), ("80684.2", "80597.6"));
    let transport = Recorded::answering(include_str!("../../../fixtures/robinhood/markPriceCandles_1.json"));
    let robinhood = block_on(mark_price_candles(&transport, btc(), Resolution::OneHour, day())).unwrap();
    assert_eq!(
        (robinhood.len(), robinhood[0].high.to_dec(), robinhood[0].samples),
        (24, Some(dec("80679.8")), 216_311)
    );
}

#[test]
fn the_day_s_hourly_prices_read_for_the_markets_asked() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/marketPriceCharts.json"));
    let lighter = block_on(market_price_charts(&transport, &[btc()])).unwrap();
    assert_eq!(transport.only().fields, [("market_ids", "1".to_owned())]);
    assert_eq!((lighter.len(), lighter[0].prices.len(), lighter[0].prices[0]), (1, 24, dec("80597.6")));

    let apple = MarketId::of::<2049>();
    let transport = Recorded::answering(include_str!("../../../fixtures/robinhood/marketPriceCharts.json"));
    let robinhood = block_on(market_price_charts(&transport, &[btc(), apple])).unwrap();
    assert_eq!(
        transport.only().fields,
        [("market_ids", "1".to_owned()), ("market_ids", "2049".to_owned())],
        "once each"
    );
    assert_eq!((robinhood[1].market_id, robinhood[1].prices[23]), (apple, dec("347.82")), "a spot pair's index");
    assert!(market_price_charts_call(&[]).fields.is_empty(), "none asks for every market");
}

#[test]
fn every_resolution_reads_and_writes_as_the_venue_spells_it() {
    let table = [
        ("1m", MINUTE_MS),
        ("5m", 5 * MINUTE_MS),
        ("15m", 15 * MINUTE_MS),
        ("30m", 30 * MINUTE_MS),
        ("1h", HOUR_MS),
        ("4h", 4 * HOUR_MS),
        ("12h", 12 * HOUR_MS),
        ("1d", DAY_MS),
    ];
    assert_eq!(Resolution::ALL.map(Resolution::wire), table.map(|(wire, _)| wire), "finest first");
    for (resolution, (wire, ms)) in Resolution::ALL.into_iter().zip(table) {
        assert_eq!((resolution.ms(), resolution.to_string()), (ms, wire.to_owned()));
        assert_eq!(wire.parse::<Resolution>(), Ok(resolution));
        assert_eq!(serde_json::from_str::<Resolution>(&format!("\"{wire}\"")).unwrap(), resolution);
    }
    for refused in ["3m", "1H", "2h", "1w", ""] {
        assert_eq!(refused.parse::<Resolution>(), Err(UnknownResolution(refused.to_owned())), "{refused:?}");
    }
}

#[test]
fn a_long_range_is_read_back_from_its_end_in_windows_of_500() {
    let step = HOUR_MS;
    let end = LAST_OPEN_MS + 30 * MINUTE_MS;
    assert_eq!(
        candle_window(START_MS, end, Resolution::OneHour),
        Some(CandleWindow { start_ms: START_MS, end_ms: end, count: 24 })
    );
    let long = candle_window(START_MS - 1_000 * step, end, Resolution::OneHour).unwrap();
    assert_eq!((long.count, long.start_ms), (500, LAST_OPEN_MS - 499 * step), "the newest 500, on whole hours");
    assert_eq!(candle_window(end, START_MS, Resolution::OneHour), None);
    assert_eq!(candle_window(START_MS, START_MS, Resolution::OneHour).map(|window| window.count), Some(1));
    assert_eq!(candle_window(START_MS + 1, START_MS + step - 1, Resolution::OneHour), None, "no candle opens inside");

    let page: Vec<Candle> = serde_json::from_str::<serde_json::Value>(LIGHTER_CANDLES)
        .map(|answer| serde_json::from_value(answer["c"].clone()).unwrap())
        .unwrap();
    let window = candle_window(START_MS, end, Resolution::OneHour).unwrap();
    assert!(reaches_start(&page, window, Resolution::OneHour));
    assert!(!reaches_start(&page[1..], window, Resolution::OneHour), "short of its count: history starts inside");
    assert!(!reaches_start(&page, long, Resolution::OneHour));

    let merged = merge_candles(START_MS + step, LAST_OPEN_MS, vec![page[12..].to_vec(), page[..13].to_vec()]);
    assert_eq!((merged.len(), merged[0].open_time, merged[22].open_time), (23, START_MS + step, LAST_OPEN_MS));
    assert!(merged.windows(2).all(|pair| pair[0].open_time < pair[1].open_time), "oldest first, each once");
}

#[test]
fn a_range_its_first_page_covers_is_one_read() {
    let transport = Recorded::answering(LIGHTER_CANDLES);
    let history = block_on(candle_history(&transport, btc(), Resolution::OneHour, START_MS, LAST_OPEN_MS, 4)).unwrap();
    assert_eq!((history.len(), transport.sent().len()), (24, 1));
    let longer = Recorded::answering(LIGHTER_CANDLES);
    let history =
        block_on(candle_history(&longer, btc(), Resolution::OneHour, START_MS - 600 * HOUR_MS, LAST_OPEN_MS, 4));
    assert_eq!((history.unwrap().len(), longer.sent().len()), (24, 1), "a short page is where history begins");
    assert_eq!(longer.only().value("count_back"), Some("500"));
}

/// Answers each read with the next page in turn and keeps each call.
struct Pages {
    pages: std::cell::RefCell<std::collections::VecDeque<String>>,
    sent: std::cell::RefCell<Vec<Call>>,
}

impl Transport for Pages {
    fn call(&self, call: Call) -> impl std::future::Future<Output = Result<String, crate::client::TransportError>> {
        self.sent.borrow_mut().push(call);
        std::future::ready(Ok(self
            .pages
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| r#"{"code":200,"c":[]}"#.to_owned())))
    }
}

/// `count` hourly candles opening from `first_ms`, as the venue writes them.
fn page(first_ms: u64, count: u64) -> String {
    let candles: Vec<String> = (0..count).map(|at| format!(r#"{{"t":{},"c":1.5}}"#, first_ms + at * HOUR_MS)).collect();
    format!(r#"{{"code":200,"r":"1h","c":[{}]}}"#, candles.join(","))
}

#[test]
fn a_long_range_pages_back_until_the_market_s_history_begins() {
    let end = START_MS + 1_499 * HOUR_MS;
    let listed = START_MS + 400 * HOUR_MS;
    let pages = Pages {
        pages: std::cell::RefCell::new(
            [page(end - 499 * HOUR_MS, 500), page(end - 999 * HOUR_MS, 500), page(listed, 100)].into_iter().collect(),
        ),
        sent: std::cell::RefCell::default(),
    };
    let history = block_on(candle_history(&pages, btc(), Resolution::OneHour, START_MS, end, 4)).unwrap();
    assert_eq!((history.len(), history[0].open_time, history[1_099].open_time), (1_100, listed, end));
    let sent = pages.sent.borrow().clone();
    assert_eq!(sent.len(), 3, "the third page fell short: no fourth read");
    assert_eq!(sent[0].value("start_timestamp"), Some((end - 499 * HOUR_MS).to_string().as_str()));
    assert_eq!(
        sent[1].value("end_timestamp"),
        Some((end - 499 * HOUR_MS - 1).to_string().as_str()),
        "a millisecond before"
    );
    assert_eq!(sent[2].value("count_back"), Some("500"));
    let history = block_on(candle_history(&pages, btc(), Resolution::OneHour, START_MS, end, 0)).unwrap();
    assert!(history.is_empty(), "no reads, nothing");
}
