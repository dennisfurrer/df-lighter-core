//! Binary options on Lighter, recorded on 2026-10-25: two 15-minute Up/Down events and thirty
//! second candles of BTC's Chainlink price. Robinhood Chain answered both reads 403.

use super::*;
use crate::client::TransportError;
use crate::markets::books::MarketStatus;
use crate::test_support::{Recorded, block_on};

const EVENTS: &str = include_str!("../../../fixtures/lighter/events.json");

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn events_read_with_their_markets_grids_and_windows() {
    let transport = Recorded::answering(EVENTS);
    let page = block_on(events(&transport, &EventQuery::tradable(2).series("btc-updown-15m"))).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/events".to_owned(), false));
    let fields =
        [("active", "true"), ("limit", "2"), ("tag", "btc-updown-15m"), ("sort", "start_at"), ("order", "asc")];
    assert_eq!(call.fields, fields.map(|(key, value)| (key, value.to_owned())));
    assert_eq!((page.events.len(), page.next_cursor), (2, Some(1_112)));

    let btc = &page.events[0];
    assert_eq!((btc.event_id, btc.title.as_str(), btc.tag.as_str()), (1_111, "BTC Up or Down", "btc-updown-15m"));
    assert_eq!(
        (btc.event_kind, btc.recurrence_period_seconds, btc.creator_account_index.get()),
        (EventKind::Recurrent, 900, 761_897)
    );
    assert_eq!((btc.start_at, btc.end_at), (1_792_917_900_000, 1_792_918_800_000), "the observation window");
    let meta = &btc.metadata;
    assert_eq!(
        (meta.kind.as_deref(), meta.symbol.as_deref(), meta.duration.as_deref()),
        (Some("updown"), Some("btc"), Some("15m"))
    );
    assert_eq!((meta.market_id, meta.underlying_market_id), (Some(MarketId::of::<5212>()), Some(MarketId::of::<1>())));

    let market = &btc.order_books[0];
    assert_eq!(
        (market.symbol.as_str(), market.market_type, market.status),
        ("BTC-UPDOWN-15M-1792917900", Some(MarketKind::BinaryOptions), MarketStatus::Active)
    );
    assert_eq!((market.grid().size_decimals(), market.grid().price_decimals()), (4, 2));
    assert_eq!((market.base_asset_id, market.quote_asset_id), (AssetId::COLLATERAL, AssetId::COLLATERAL));
    assert_eq!((market.taker_fee, market.maker_fee, market.min_base_amount), (dec("1"), Dec::ZERO, dec("5")));
    assert_eq!((market.payout(), market.default_price, market.settled_at()), (Dec::ONE, 50, None));
    assert_eq!((market.outcome, market.open_interest), (Outcome::Undetermined, dec("20.93")));
    assert_eq!(market.start_timestamp, btc.start_at - 7_200_000, "trading opens two hours early");
    assert!(market.is_open(btc.start_at) && market.is_open(market.start_timestamp));
    assert!(!market.is_open(market.end_timestamp + 1) && !market.is_open(market.start_timestamp - 1));
    let stats = market.stats.as_ref().unwrap();
    assert_eq!((stats.mid_price, stats.best_bid_price, stats.best_ask_price), (dec("0.54"), dec("0.46"), dec("0.61")));
    assert_eq!((stats.daily_quote_token_volume.to_dec(), stats.status), (Some(dec("9.4185")), MarketStatus::Active));
    assert_eq!(page.events[1].order_books[0].stats.as_ref().map(|stats| stats.last_trade_price), Some(Dec::ZERO));
}

#[test]
fn a_settled_market_reads_its_outcome_and_its_price() {
    let text = EVENTS.replacen(r#""status":"active""#, r#""status":"settled""#, 1).replacen(
        r#""outcome":0,"#,
        r#""outcome":2,"#,
        1,
    );
    let text = text.replacen(r#""settlement_price":0,"#, r#""settlement_price":100,"#, 1);
    let page: EventPage = serde_json::from_str(&text).unwrap();
    let market = &page.events[0].order_books[0];
    assert_eq!(
        (market.status, market.outcome, market.settled_at()),
        (MarketStatus::Settled, Outcome::Yes, Some(Dec::ONE))
    );
    assert!(!market.is_open(page.events[0].start_at));
    for (code, outcome) in [(0, Outcome::Undetermined), (1, Outcome::No), (2, Outcome::Yes), (7, Outcome::Other(7))] {
        assert_eq!((Outcome::from_code(code), outcome.code()), (outcome, code));
    }
    let unknown: MarketStatus = serde_json::from_str(r#""paused""#).unwrap();
    assert_eq!(unknown, MarketStatus::Other);
}

#[test]
fn events_are_asked_by_ids_or_filters_and_robinhood_chain_has_none() {
    let by_market = events_call(&EventQuery::of_markets(&[MarketId::of::<5212>(), MarketId::of::<5213>()]));
    assert_eq!(by_market.fields, [("market_ids", "5212,5213".to_owned())]);
    let query = EventQuery {
        event_ids: vec![1_111],
        creator: Some(AccountIndex::of::<761_897>()),
        event_kind: Some(EventKind::OneOff),
        start_from: Some(1),
        start_before: Some(2),
        limit: Some(1_000),
        ..EventQuery::default()
    }
    .after(1_112);
    let fields = [
        ("event_ids", "1111"),
        ("creator_account_index", "761897"),
        ("cursor", "1112"),
        ("limit", "100"),
        ("event_kind", "one_off"),
        ("start_from", "1"),
        ("start_before", "2"),
    ];
    assert_eq!(events_call(&query).fields, fields.map(|(key, value)| (key, value.to_owned())));
    assert!(events_call(&EventQuery::default()).fields.is_empty(), "the venue's defaults: newest first");

    let transport = Recorded::failing(TransportError::Status { status: 403, body: String::new() });
    let refused = block_on(events(&transport, &EventQuery::default())).unwrap_err();
    assert_eq!(refused, ApiError::Transport(TransportError::Status { status: 403, body: String::new() }));
}

#[test]
fn the_underlying_s_candles_read_exactly_and_page_on() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/upDownCandles.json"));
    let page = block_on(up_down_candles(
        &transport,
        "btc",
        "15m",
        UpDownResolution::ThirtySeconds,
        1_792_917_840,
        1_792_917_990,
        None,
    ));
    let page = page.unwrap();
    let fields = [
        ("symbol", "btc"),
        ("duration", "15m"),
        ("resolution", "30s"),
        ("start_timestamp", "1792917840"),
        ("end_timestamp", "1792917990"),
    ];
    assert_eq!(transport.only().fields, fields.map(|(key, value)| (key, value.to_owned())));
    assert_eq!((page.candles.len(), page.next()), (5, Some(1_792_917_990)));
    let first = &page.candles[0];
    assert_eq!(
        (first.timestamp, first.resolution.as_str(), first.provider.as_str()),
        (1_792_917_840, "30s", "chainlink")
    );
    assert_eq!(first.open, dec("80394.831153821296623616"), "eighteen places, exact");
    assert!(page.candles.windows(2).all(|pair| pair[0].timestamp + 30 == pair[1].timestamp));
    assert_eq!(UpDownPage { candles: Vec::new(), next_cursor: 0 }.next(), None);
    let call = up_down_candles_call("eth", "5m", UpDownResolution::Second, 0, 1, Some(10_000));
    assert_eq!((call.value("resolution"), call.value("limit")), (Some("1s"), Some("3600")));
}
