//! A book kept from the recorded snapshot and its updates, its nonce chain held, as atlas keeps
//! one from its own socket.

use super::*;
use crate::client::Network;
use crate::live::decode;
use crate::live::fixtures::{all, lighter, robinhood};

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn btc() -> MarketId {
    MarketId::new(1).unwrap()
}

fn kept(network: Network) -> Book {
    let mut book = Book::new(btc());
    for text in std::iter::once(crate::live::fixtures::first(network, "subscribed/order_book", "order_book:1"))
        .chain(all(network, "update/order_book", "order_book:1"))
    {
        assert_eq!(book.apply(&decode(text).unwrap()), Ok(true), "{network}");
    }
    book
}

/// Each update moves the levels it names to their new size, zero removing one; the best bid stays
/// below the best offer throughout.
#[test]
fn a_book_follows_its_snapshot_and_updates() {
    let mut book = Book::new(btc());
    assert!(!book.is_ready());
    assert_eq!(book.apply(&decode(lighter("update/order_book", "")).unwrap()), Ok(false), "no snapshot yet");
    assert_eq!(book.apply(&decode(lighter("subscribed/order_book", "")).unwrap()), Ok(true));
    assert_eq!((book.is_ready(), book.nonce(), book.depth()), (true, Some(25_652_761_941), (1543, 1658)));
    assert_eq!(book.best_bid(), Some(Level { price: dec("80441.6"), size: dec("0.0002") }));
    let first = lighter("update/order_book", "");
    assert_eq!(book.apply(&decode(first).unwrap()), Ok(true));
    assert_eq!(book.best_ask(), Some(Level { price: dec("80441.7"), size: dec("4.35786") }));
    assert!(book.bids().all(|level| level.price != dec("80435.6")), "a size of zero removes the level");
    assert_eq!((book.nonce(), book.timestamp()), (Some(25_652_762_001), 1_792_918_164_524));

    let book = kept(Network::Lighter);
    assert_eq!(book.nonce(), Some(25_652_762_093));
    assert!(book.best_bid().unwrap().price < book.best_ask().unwrap().price);
    assert_eq!(book.spread(), book.best_ask().unwrap().price.checked_sub(book.best_bid().unwrap().price));
    let robinhood = kept(Network::Robinhood);
    assert!(robinhood.best_bid().unwrap().price < robinhood.best_ask().unwrap().price);
}

/// An update that does not begin where the book is: the book empties and says where it was,
/// until the next snapshot. A session's gap does the same.
#[test]
fn a_book_that_misses_an_update_empties_until_its_next_snapshot() {
    let mut book = Book::new(btc());
    let snapshot = decode(lighter("subscribed/order_book", "")).unwrap();
    book.apply(&snapshot).unwrap();
    let lost = lighter("update/order_book", "").replace(r#""begin_nonce":25652761941"#, r#""begin_nonce":5"#);
    assert_eq!(book.apply(&decode(&lost).unwrap()), Err(Gap { market: btc(), expected: 25_652_761_941, got: 5 }));
    assert_eq!((book.is_ready(), book.depth(), book.mid()), (false, (0, 0), None));
    assert_eq!(book.apply(&decode(lighter("update/order_book", "")).unwrap()), Ok(false), "waiting");
    book.apply(&snapshot).unwrap();
    assert!(book.is_ready());
    assert_eq!(book.apply(&Frame::Gap(Gap { market: btc(), expected: 1, got: 2 })), Ok(true));
    assert!(!book.is_ready());
}

/// Another market's frames pass by; prices compare by value, so a level written two ways is
/// one.
#[test]
fn a_book_keeps_its_own_market_and_one_level_a_price() {
    let mut book = Book::new(MarketId::new(0).unwrap());
    assert_eq!(book.apply(&decode(robinhood("subscribed/order_book", "")).unwrap()), Ok(false));
    let mut book = Book::new(btc());
    let snapshot = r#"{"channel":"order_book:1","order_book":{"asks":[{"price":"101.0","size":"1"}],
        "bids":[{"price":"100","size":"2"},{"price":"99","size":"3"}],"nonce":1,"begin_nonce":0},"type":"subscribed/order_book"}"#;
    let update = r#"{"channel":"order_book:1","order_book":{"asks":[{"price":"101.00","size":"4"}],
        "bids":[{"price":"100.0","size":"0"}],"nonce":2,"begin_nonce":1},"type":"update/order_book"}"#;
    book.apply(&decode(snapshot).unwrap()).unwrap();
    book.apply(&decode(update).unwrap()).unwrap();
    assert_eq!(book.depth(), (1, 1));
    assert_eq!(book.best_ask(), Some(Level { price: dec("101"), size: dec("4") }));
    assert_eq!((book.mid(), book.spread()), (Some(dec("100")), Some(dec("2"))));
    let (bids, asks) = book.top(DEFAULT_DEPTH);
    assert_eq!((bids.len(), asks.len()), (1, 1));
}

#[test]
fn the_top_of_a_book_is_its_best_levels_each_side() {
    let book = kept(Network::Lighter);
    let (bids, asks) = book.top(DEFAULT_DEPTH);
    assert_eq!((bids.len(), asks.len(), DEFAULT_DEPTH), (20, 20, 20));
    assert!(bids.windows(2).all(|pair| pair[0].price > pair[1].price), "bids highest first");
    assert!(asks.windows(2).all(|pair| pair[0].price < pair[1].price), "asks lowest first");
    assert_eq!(Some(bids[0]), book.best_bid());
}
