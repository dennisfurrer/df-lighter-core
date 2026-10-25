//! Trades as both venues wrote them, recorded: every field read exactly, and each side named.

use serde_json::Value;

use super::*;

fn trades(text: &str) -> Vec<Trade> {
    let page: Value = serde_json::from_str(text).unwrap();
    serde_json::from_value(page["trades"].clone()).unwrap()
}

#[test]
fn recorded_trades_read_on_both_venues() {
    let lighter = trades(include_str!("../../../fixtures/lighter/recentTrades_1.json"));
    assert_eq!(lighter.len(), 20);
    let first = &lighter[0];
    assert_eq!((first.trade_id, first.kind, first.market_id.get()), (36_388_615_593, TradeKind::Trade, 1));
    assert_eq!((first.market_symbol.as_str(), first.market_kind), ("BTC", Some(MarketKind::Perps)));
    assert_eq!((first.size.to_string(), first.price.to_string()), ("0.03727".to_owned(), "80466.1".to_owned()));
    assert_eq!(first.usd_amount.to_string(), "2998.971547");
    assert_eq!((first.ask_id, first.ask_client_id), (562_953_635_421_852, 17_928_831_250_668));
    assert_eq!((first.timestamp, first.transaction_time), (1_792_918_063_399, 1_792_918_063_880_922));
    assert_eq!((first.maker_fee, first.taker_fee), (Some(32), None));
    assert_eq!(first.taker_position_size_before.map(|size| size.to_string()).as_deref(), Some("0.03727"));
    assert!(lighter.iter().all(|trade| trade.tx_hash.len() == 80));

    let page = trades(include_str!("../../../fixtures/lighter/trades_1.json"));
    assert_eq!(page.len(), 5);
    let robinhood = trades(include_str!("../../../fixtures/robinhood/recentTrades.json"));
    assert_eq!((robinhood[0].market_id.get(), robinhood[0].taker_fee, robinhood[1].taker_fee), (21, None, Some(245)));
    assert_eq!(robinhood[0].bid_account_id.get(), 281_474_976_710_654, "the LLP took the other side");
}

#[test]
fn the_maker_is_named_and_the_taker_is_the_other_side() {
    let trade = &trades(include_str!("../../../fixtures/lighter/recentTrades_1.json"))[0];
    assert!(!trade.is_maker_ask);
    assert_eq!((trade.maker_side(), trade.taker_side()), (Side::Bid, Side::Ask));
    assert_eq!((trade.maker_account().get(), trade.taker_account().get()), (513_030, 731_664));
    assert_eq!(trade.side_of(trade.ask_account_id), Some(Side::Ask));
    assert_eq!(trade.side_of(AccountIndex::new(1).unwrap()), None);
    assert_eq!(trade.ask_order().map(OrderIndex::get), Some(562_953_635_421_852));
}

#[test]
fn an_unknown_trade_kind_still_reads() {
    let text = r#"{"trade_id":1,"tx_hash":"","type":"something-new","market_id":0,"size":"1","price":"2",
        "usd_amount":"2","ask_id":0,"bid_id":0,"ask_account_id":1,"bid_account_id":2,"is_maker_ask":true,
        "timestamp":5,"market_kind":"martian"}"#;
    let trade: Trade = serde_json::from_str(text).unwrap();
    assert_eq!((trade.kind, trade.market_kind, trade.ask_order()), (TradeKind::Other, None, None));
}

fn market(value: u64) -> MarketId {
    MarketId::new(value).unwrap()
}

#[test]
fn the_tape_is_read_on_both_networks_newest_first() {
    use crate::test_support::{Recorded, block_on};

    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/recentTrades_1.json"));
    let tape = block_on(recent_trades(&transport, market(1), 20)).unwrap();
    assert_eq!(tape.len(), 20);
    assert!(tape.windows(2).all(|pair| pair[0].timestamp >= pair[1].timestamp), "newest first");
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/recentTrades".to_owned(), false));
    assert_eq!(call.fields, [("market_id", "1".to_owned()), ("limit", "20".to_owned())]);

    let transport = Recorded::answering(include_str!("../../../fixtures/robinhood/recentTrades.json"));
    let tape = block_on(recent_trades(&transport, market(21), 5)).unwrap();
    assert_eq!((tape.len(), tape[0].market_symbol.as_str(), tape[0].trade_id), (5, "USAR", 2_048_050_397));
    assert_eq!(recent_trades_call(market(1), 500).value("limit"), Some("100"), "at most a hundred");
}

#[test]
fn a_market_s_history_pages_on_by_its_cursor_on_both_networks() {
    use crate::test_support::{Recorded, block_on};

    let query = TradeQuery::market(market(1), 5);
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/trades_1.json"));
    let page = block_on(super::trades(&transport, &query)).unwrap();
    assert_eq!((page.trades.len(), page.next()), (5, Some("eyJpbmRleCI6MzYzODg2MjcxNDJ9")));
    assert_eq!((page.trades[0].trade_id, page.trades[0].maker_fee), (36_388_627_146, Some(28)));
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/trades".to_owned(), false), "a market's history is public");
    let fields = [("market_id", "1"), ("sort_by", "timestamp"), ("limit", "5")];
    assert_eq!(call.fields, fields.map(|(key, value)| (key, value.to_owned())));

    let robinhood = Recorded::answering(include_str!("../../../fixtures/robinhood/trades_1.json"));
    let page = block_on(super::trades(&robinhood, &query)).unwrap();
    assert_eq!((page.trades[0].trade_id, page.trades[0].taker_fee), (2_049_192_294, Some(350)));
    let next = query.after(page.next().unwrap());
    assert_eq!(trades_call(&next).value("cursor"), Some("eyJpbmRleCI6MjA0OTE5MTQzOX0="));

    let last = TradePage { trades: Vec::new(), next_cursor: Some("abc".to_owned()) };
    assert_eq!(last.next(), None, "an empty page is the last");
}

#[test]
fn an_account_s_trades_carry_its_token_and_a_pool_s_do_not() {
    let account = AccountIndex::new(10).unwrap();
    let own = TradeQuery::account(account, 100)
        .on(market(2048))
        .of_kind(MarketKind::Spot)
        .of_order(OrderIndex::new(576_742_227_478_928_076).unwrap())
        .as_role(Role::Maker)
        .only(TradeKind::Liquidation)
        .sorted_by(SortBy::BlockHeight)
        .after("c");
    let call = trades_call(&own);
    assert!(call.auth, "a master account's or a sub-account's own");
    let fields = [
        ("market_id", "2048"),
        ("market_type", "spot"),
        ("account_index", "10"),
        ("order_index", "576742227478928076"),
        ("sort_by", "block_height"),
        ("cursor", "c"),
        ("role", "maker"),
        ("type", "liquidation"),
        ("limit", "100"),
    ];
    assert_eq!(call.fields, fields.map(|(key, value)| (key, value.to_owned())));
    let pool = trades_call(&TradeQuery::pool(AccountIndex::LLP, 1_000));
    assert!(!pool.auth, "a public pool's history is public");
    assert_eq!(pool.value("account_index"), Some("281474976710654"));
    assert_eq!(pool.value("limit"), Some("100"));
    assert_eq!(
        trades_call(&TradeQuery::market(market(1), 5).of_kind(MarketKind::Perps)).value("market_type"),
        Some("perp")
    );
}
