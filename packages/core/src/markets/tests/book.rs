//! Books read on both networks on 2026-10-25: BTC and ETH/USDC on Lighter at ten orders a side,
//! BTC at ten and AAPL/USDG at five on Robinhood Chain. The walks' figures are worked by hand from
//! the same rows.

use super::*;
use crate::test_support::{Recorded, block_on};

const LIGHTER_BTC: &str = include_str!("../../../fixtures/lighter/orderBookOrders_1.json");
const LIGHTER_ETH_USDC: &str = include_str!("../../../fixtures/lighter/orderBookOrders_2048.json");
const ROBINHOOD_BTC: &str = include_str!("../../../fixtures/robinhood/orderBookOrders_1.json");
const ROBINHOOD_AAPL: &str = include_str!("../../../fixtures/robinhood/orderBookOrders_2049.json");

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn id(value: u64) -> MarketId {
    MarketId::new(value).unwrap()
}

fn book(text: &str, market: u64, limit: u32) -> Book {
    let transport = Recorded::answering(text);
    let book = block_on(order_book_orders(&transport, id(market), limit)).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/orderBookOrders".to_owned(), false));
    assert_eq!(call.fields, [("market_id", market.to_string()), ("limit", limit.to_string())]);
    book
}

#[test]
fn a_book_reads_its_orders_best_first_with_their_owners() {
    let btc = book(LIGHTER_BTC, 1, 10);
    assert_eq!((btc.limit, btc.bids.len(), btc.asks.len()), (10, 10, 10));
    let first = &btc.asks[0];
    assert_eq!(
        (first.order().map(OrderIndex::get), first.owner_account_index.get()),
        (Some(562_953_635_421_737), 281_474_976_509_475)
    );
    assert_eq!(
        (first.price, first.remaining_base_amount, first.order_expiry),
        (dec("80470.7"), dec("0.0313"), 1_795_337_261_203)
    );
    assert!(btc.asks.windows(2).all(|pair| pair[0].price <= pair[1].price), "asks lowest first");
    assert!(btc.bids.windows(2).all(|pair| pair[0].price >= pair[1].price), "bids highest first");

    let eth = book(LIGHTER_ETH_USDC, 2048, 10);
    let llp = &eth.asks[0];
    assert_eq!((llp.owner_account_index, llp.filled()), (AccountIndex::LLP, dec("0.3003")), "partly filled");
    assert_eq!(llp.order().map(OrderIndex::get), Some(576_742_227_478_928_076), "(2048 + 1) << 48 and up");

    let robinhood = book(ROBINHOOD_BTC, 1, 10);
    assert_eq!(robinhood.touch(), (Some(dec("80161.4")), Some(dec("80170.3"))));
    assert_eq!(robinhood.levels(Side::Bid)[0], Level { price: dec("80161.4"), size: dec("0.0449"), orders: 2 });
}

#[test]
fn the_touch_mid_and_spread_are_exact() {
    let btc = book(LIGHTER_BTC, 1, 10);
    assert_eq!((btc.best_bid(), btc.best_ask()), (Some(dec("80470.6")), Some(dec("80470.7"))));
    assert_eq!((btc.mid(), btc.spread()), (Some(dec("80470.65")), Some(dec("0.1"))));
    let aapl = book(ROBINHOOD_AAPL, 2049, 5);
    assert_eq!((aapl.mid(), aapl.spread()), (Some(dec("346.475")), Some(dec("1.25"))));
    let empty = Book::default();
    assert_eq!((empty.touch(), empty.mid(), empty.spread()), ((None, None), None, None));
}

#[test]
fn depth_sums_each_price_and_a_cut_side_s_last_level_is_not_whole() {
    let btc = book(LIGHTER_BTC, 1, 10);
    let asks = btc.levels(Side::Ask);
    assert_eq!(asks.len(), 8);
    assert_eq!(asks[1], Level { price: dec("80470.9"), size: dec("0.14656"), orders: 2 });
    assert_eq!(asks[2], Level { price: dec("80471"), size: dec("0.04685"), orders: 2 });
    assert!(btc.is_cut(Side::Ask) && btc.is_cut(Side::Bid), "ten asked, ten listed");
    assert_eq!((btc.edge(Side::Ask), btc.edge(Side::Bid)), (Some(dec("80472.3")), Some(dec("80465.3"))));
    let whole = btc.whole_levels(Side::Ask, 20);
    assert_eq!((whole.len(), whole.last().map(|level| level.price)), (7, Some(dec("80472.2"))));
    assert_eq!(btc.whole_levels(Side::Bid, 3).len(), 3);
    assert_eq!(btc.size_to(Side::Ask, dec("80471")), dec("0.22471"));
    assert_eq!(btc.size_to(Side::Bid, dec("80470.4")), dec("0.0006"));

    let longer = book(LIGHTER_BTC, 1, 250);
    assert!(!longer.is_cut(Side::Ask), "ten of 250: the whole side");
    assert_eq!((longer.edge(Side::Ask), longer.whole_levels(Side::Ask, 20).len()), (None, 8));
    assert!(!Book { limit: 0, ..longer }.is_cut(Side::Bid), "an unknown limit cuts nothing");
}

#[test]
fn a_taker_walks_the_other_side_to_an_average_and_a_worst_price() {
    let btc = book(LIGHTER_BTC, 1, 10);
    let buy = btc.walk(Side::Bid, dec("0.2"));
    assert_eq!((buy.filled, buy.notional, buy.worst), (dec("0.2"), dec("16094.175954"), Some(dec("80471"))));
    assert_eq!((buy.average, buy.complete), (Some(dec("80470.87977")), true));

    let sell = btc.walk(Side::Ask, dec("1"));
    assert_eq!((sell.filled, sell.worst, sell.average), (dec("1"), Some(dec("80466.2")), Some(dec("80466.20568"))));

    let too_big = btc.walk(Side::Bid, dec("5"));
    assert_eq!((too_big.filled, too_big.complete, too_big.worst), (dec("1.06058"), false, Some(dec("80472.3"))));
    assert_eq!(too_big.average, Some(dec("80471.486489468026928662")), "eighteen places, rounded up for a buy");

    let at_touch = btc.walk(Side::Bid, dec("0.0313"));
    assert_eq!((at_touch.worst, at_touch.average), (Some(dec("80470.7")), Some(dec("80470.7"))));
    let nothing = Book::default().walk(Side::Ask, dec("1"));
    assert_eq!((nothing.filled, nothing.average, nothing.worst, nothing.complete), (Dec::ZERO, None, None, false));
    assert!(btc.walk(Side::Bid, Dec::ZERO).complete);
}

#[test]
fn a_trade_names_the_orders_it_filled_and_their_owners() {
    let page: TradeList = serde_json::from_str(include_str!("../../../fixtures/lighter/recentTrades_1.json")).unwrap();
    let filled = orders_filled(&page.trades);
    assert_eq!(filled.len(), 40, "an ask and a bid each");
    let trade = &page.trades[0];
    assert_eq!(
        filled[0],
        FilledOrder { order: trade.ask_order().unwrap(), owner: trade.ask_account_id, at_ms: trade.timestamp }
    );
    assert_eq!(filled[1].owner.get(), 513_030);
}

#[derive(serde::Deserialize)]
struct TradeList {
    trades: Vec<Trade>,
}

#[test]
fn a_book_is_asked_for_at_most_250_orders_a_side() {
    assert_eq!(order_book_orders_call(id(1), 1_000).value("limit"), Some("250"));
    assert_eq!(order_book_orders_call(id(1), 0).value("limit"), Some("1"));
    let transport = Recorded::answering(LIGHTER_BTC);
    assert_eq!(block_on(order_book_orders(&transport, id(1), 1_000)).unwrap().limit, MOST_BOOK_ORDERS);
}
