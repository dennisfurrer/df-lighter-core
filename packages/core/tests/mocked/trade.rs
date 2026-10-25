//! Trading as atlas trades: a market's details and book read, an order built on its grid from
//! decimals, and every transaction checked by the venue as it is taken.

use df_lighter_core::markets::{self, MarketFilter};
use df_lighter_core::trade::{self, MarginMode, OrderIntent, Side, Size};
use df_lighter_core::utils::MarketId;

use crate::client::trader;
use crate::support::clock;
use crate::support::venue::Venue;

fn market() -> Venue {
    Venue::lighter()
        .read("orderBookDetails", "lighter/orderBookDetails.json")
        .read("orderBookOrders", "lighter/orderBookOrders_1.json")
}

/// A $20 buy of BTC at the book's best ask, bounded 2% past it, sized on BTC's grid.
#[tokio::test]
async fn a_market_buy_is_built_from_the_book_and_taken() {
    let client = trader(market());
    let perps = markets::order_book_details(&client, MarketFilter::Perps).await.unwrap();
    let btc = perps.by_symbol("BTC").unwrap();
    let book = markets::order_book_orders(&client, btc.market_id, 50).await.unwrap();
    let ask = book.best_ask().unwrap();
    assert_eq!(ask.to_string(), "80470.7");

    let order = OrderIntent::market(btc.market_id, Side::Bid, Size::Quote("20".parse().unwrap()), ask)
        .build(btc.grid(), clock())
        .unwrap();
    trade::place(&client, &order).await.unwrap();
    let taken = client.http().taken();
    let placed = &taken[0];
    assert_eq!((placed.tx_type, placed.number("MarketIndex"), placed.number("IsAsk")), (14, 1, 0));
    assert_eq!(placed.number("BaseAmount"), 24, "$20 at 80470.7 is 0.000248 BTC, cut to 0.00024");
    assert_eq!(placed.number("Price"), 820_801, "80470.7 and 2%, rounded back toward the book: 82080.1");
    assert_eq!((placed.number("Type"), placed.number("TimeInForce"), placed.number("OrderExpiry")), (1, 0, 0));
}

/// Perps' cancel-all leaves spot alone: one cancel-all per market, as one batch.
#[tokio::test]
async fn cancelling_every_perp_order_is_one_batch_a_market() {
    let client = trader(Venue::lighter());
    let markets = [MarketId::new(0).unwrap(), MarketId::new(1).unwrap(), MarketId::new(1).unwrap()];
    let sent = trade::cancel_all_in_markets(&client, &markets).await.unwrap();
    assert_eq!(sent.tx_hashes.len(), 2, "each market once");
    let taken = client.http().taken();
    let cancelled: Vec<Option<u64>> = taken
        .iter()
        .map(|tx| {
            tx.info.get("L2TxAttributes").and_then(|attributes| attributes.get("5")).and_then(|market| market.as_u64())
        })
        .collect();
    assert_eq!(cancelled, [Some(0), Some(1)]);
}

#[tokio::test]
async fn leverage_and_isolated_margin_are_set_and_taken() {
    let client = trader(Venue::lighter());
    let btc = MarketId::new(1).unwrap();
    trade::set_leverage(&client, btc, 10, MarginMode::Isolated).await.unwrap();
    trade::add_margin(&client, btc, "25.5".parse().unwrap()).await.unwrap();
    trade::remove_margin(&client, btc, "5000.000001".parse().unwrap()).await.unwrap();
    let taken = client.http().taken();
    assert_eq!(taken.iter().map(|tx| tx.tx_type).collect::<Vec<_>>(), [20, 29, 29]);
    assert_eq!((taken[0].number("InitialMarginFraction"), taken[0].number("MarginMode")), (1_000, 1));
    assert_eq!((taken[1].number("USDCAmount"), taken[1].number("Direction")), (25_500_000, 1));
    assert_eq!((taken[2].number("USDCAmount"), taken[2].number("Direction")), (5_000_000_001, 0));
}
