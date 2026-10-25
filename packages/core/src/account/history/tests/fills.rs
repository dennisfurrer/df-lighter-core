//! Fills from recorded trades (`trades_1.json`, a public page) read as each side's, and the
//! account's own trades asked with the token (the venue refuses them without one, so a page with a
//! reducing fill is shaped from the docs' `Trade` schema). Paging stops where it should.

use super::*;
use crate::test_support::{Recorded, block_on};

const PAGE: &str = include_str!("../../../../fixtures/lighter/trades_1.json");

fn page() -> TradePage {
    serde_json::from_str(PAGE).unwrap()
}

fn index(account: u64) -> AccountIndex {
    AccountIndex::new(account).unwrap()
}

#[test]
fn a_recorded_trade_is_the_maker_s_fill_on_one_side_and_the_taker_s_on_the_other() {
    let trade = &page().trades[0];
    let maker = Fill::of(trade, index(702_384)).unwrap();
    assert_eq!((maker.side, maker.is_maker, maker.fee_rate), (Side::Bid, true, 28));
    assert_eq!(maker.fee.to_string(), "0.010748", "383.840469 at 0.0028%");
    assert_eq!(maker.realized_pnl, Some(Dec::ZERO), "a long buying adds");
    assert_eq!(
        (maker.order_index(), maker.client_order_index().map(ClientOrderIndex::get)),
        (844_421_215_082_639, Some(105_497_301_242_118))
    );
    let taker = Fill::of(trade, index(750_486)).unwrap();
    assert_eq!((taker.side, taker.is_maker, taker.fee_rate, taker.fee), (Side::Ask, false, 0, Dec::ZERO));
    assert_eq!(taker.realized_pnl, Some(Dec::ZERO), "a short selling adds");
    assert_eq!((taker.market().get(), taker.timestamp_ms(), taker.is_liquidation()), (1, 1_792_918_079_391, false));
    assert_eq!((taker.price().to_string(), taker.size().to_string()), ("80469.7".to_owned(), "0.00477".to_owned()));
    assert_eq!(Fill::of(trade, index(1)), None, "not the account's");
}

#[test]
fn a_fill_against_the_position_realizes_against_its_average_entry() {
    // The documented shape: a long of 2 entered at 100 (entry quote 200), sold 3 at 110 by a taker.
    let text = r#"{"trade_id":1,"tx_hash":"00","type":"trade","market_id":1,"size":"3","price":"110",
        "usd_amount":"330","ask_id":562949953421313,"bid_id":562949953421314,"ask_account_id":7,"bid_account_id":8,
        "is_maker_ask":false,"timestamp":1790000000000,"taker_fee":280,"taker_position_size_before":"2",
        "taker_entry_quote_before":"200","maker_position_size_before":"-1","maker_entry_quote_before":"120",
        "ask_account_pnl":"20.000000"}"#;
    let trade: Trade = serde_json::from_str(text).unwrap();
    let seller = Fill::of(&trade, index(7)).unwrap();
    assert_eq!(seller.realized_pnl.map(|pnl| pnl.to_string()).as_deref(), Some("20"), "2 closed at 110 against 100");
    assert_eq!(
        (seller.fee.to_string(), seller.venue_pnl().map(|pnl| pnl.to_string())),
        ("0.0924".to_owned(), Some("20".to_owned()))
    );
    let buyer = Fill::of(&trade, index(8)).unwrap();
    assert_eq!(
        buyer.realized_pnl.map(|pnl| pnl.to_string()).as_deref(),
        Some("10"),
        "a short of 1 at 120 bought back at 110"
    );
    assert_eq!((buyer.is_maker, buyer.fee), (true, Dec::ZERO));

    assert_eq!(realized(Side::Ask, Dec::ONE, Dec::ONE, None, Some(Dec::ONE)), None, "no position before, no figure");
    let third = Dec::parse("0.000001").unwrap();
    let odd = realized(Side::Ask, Dec::from_int(1), Dec::from_int(1), Some(Dec::from_int(3)), Some(third));
    assert_eq!(odd.map(|pnl| pnl.to_string()).as_deref(), Some("1"), "rounded once, to six places");
    assert_eq!(fee(Dec::parse("1000").unwrap(), 10_000).to_string(), "10", "the 1% liquidation fee");
}

#[test]
fn the_account_s_trades_are_asked_with_the_token_and_every_filter() {
    let transport = Recorded::answering(PAGE);
    let query = TradeQuery {
        market: Some(MarketId::new(1).unwrap()),
        kinds: vec![MarketKind::Perps],
        role: Some(Role::Taker),
        kind: Some(TradeKind::Liquidation),
        side: Some(Side::Bid),
        limit: 0,
        ..TradeQuery::new(index(702_384))
    };
    let first = block_on(account_trades(&transport, &query)).unwrap();
    assert_eq!((first.trades.len(), first.next()), (5, Some("eyJpbmRleCI6MzYzODg2MjcxNDJ9")));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("trades", true));
    assert_eq!(
        call.fields,
        [
            ("account_index", "702384".to_owned()),
            ("market_id", "1".to_owned()),
            ("market_type", "perp".to_owned()),
            ("sort_by", "timestamp".to_owned()),
            ("ask_filter", "0".to_owned()),
            ("role", "taker".to_owned()),
            ("type", "liquidation".to_owned()),
            ("limit", "1".to_owned()),
        ]
    );
    assert_eq!(
        account_trades_call(&query.after(&first).unwrap()).value("cursor"),
        Some("eyJpbmRleCI6MzYzODg2MjcxNDJ9")
    );
}

#[test]
fn fills_since_reads_pages_until_one_reaches_back_past_the_start() {
    // Every page answers the same five trades, all at 1792918079391; two are the account's.
    let account = index(702_384);
    let transport = Recorded::answering(PAGE);
    let all = block_on(fills_since(&transport, account, &[], 0, 3)).unwrap();
    assert_eq!(transport.sent().len(), 3, "never past the pages asked");
    assert_eq!(all.len(), 3 * page().trades.iter().filter(|trade| trade.side_of(account).is_some()).count());

    assert_eq!(all.len(), 6);

    let transport = Recorded::answering(PAGE);
    let later = block_on(fills_since(&transport, account, &[MarketKind::Perps], 1_792_918_079_392, 5)).unwrap();
    assert_eq!((transport.sent().len(), later.len()), (1, 0), "the first page already reached back past it");
    assert_eq!(transport.sent()[0].value("market_type"), Some("perp"));
}
