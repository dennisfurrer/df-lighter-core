//! Both venues' recordings folded into a store: books, tickers, figures, tapes, the height and an
//! account, each stamped when told.

use super::*;
use crate::client::Network;
use crate::live::fixtures::{ORDER, every, lighter};
use crate::live::{Gap, decode};

fn market(id: u64) -> MarketId {
    MarketId::new(id).unwrap()
}

fn store_of(network: Network) -> Store {
    let mut store = Store::new();
    for (at, text) in every(network).into_iter().enumerate() {
        let frame = decode(text).unwrap();
        let _ = store.apply(1_000 + at as u64, &frame);
    }
    store
}

#[test]
fn a_venue_s_frames_fold_into_what_a_view_reads() {
    let store = store_of(Network::Lighter);
    let book = store.book(market(1)).unwrap();
    assert!(book.is_ready());
    assert_eq!(book.nonce(), Some(25_652_762_093));
    assert_eq!(store.mid(market(1)), book.mid());
    assert_eq!(store.ticker(market(1)).map(|ticker| ticker.nonce), Some(25_652_762_006));
    assert_eq!(store.market_stats(market(1)).map(|stats| stats.symbol.as_str()), Some("BTC"));
    assert!(store.spot_stats(market(2048)).is_some());
    assert!(store.option_stats(market(5210)).is_some());
    assert_eq!(store.height().map(|height| height.height), Some(365_159_576));
    let tape = store.tape(market(1)).unwrap();
    assert_eq!(tape.len(), TAPE.min(106));
    assert!(tape.windows(2).all(|pair| pair[0].timestamp >= pair[1].timestamp), "newest first");
    let account = store.account(AccountIndex::new(10).unwrap()).unwrap();
    assert_eq!(account.positions().map(|positions| positions.len()), Some(48));
    assert_eq!(account.open_positions().count(), 0);
    assert_eq!(account.assets().map(|assets| assets.len()), Some(2));
    assert_eq!(account.stats().map(|stats| stats.stats.collateral), Dec::parse("1945.003244").ok());
    assert_eq!(account.volumes().map(|volumes| volumes.total.as_str()), Some("2935377.596694"));

    let store = store_of(Network::Robinhood);
    assert!(store.book(market(1)).unwrap().is_ready());
    assert!(store.market_stats(market(10)).is_some(), "AAPL, from the `all` channel");
    assert!(store.mid(market(2049)).is_some(), "a spot market's mid from its figures");
}

/// The same frame told again changes nothing but its stamp; an update to a book not held is not
/// kept; a gap empties the book.
#[test]
fn a_frame_told_again_is_the_same_and_a_gap_empties_the_book() {
    let mut store = Store::new();
    let height = decode(lighter("subscribed/height", "")).unwrap();
    assert_eq!(store.apply(1, &height), Folded::Changed);
    assert_eq!(store.apply(2, &height), Folded::Same);
    assert_eq!(store.height().map(|held| (held.heard, held.changed)), Some((2, 1)));
    assert_eq!(store.apply(3, &decode(lighter("update/order_book", "")).unwrap()), Folded::Dropped);
    assert_eq!(store.apply(4, &decode(lighter("subscribed/order_book", "")).unwrap()), Folded::Changed);
    assert_eq!(store.apply(5, &Frame::Gap(Gap { market: market(1), expected: 1, got: 2 })), Folded::Changed);
    assert!(!store.book(market(1)).unwrap().is_ready());
    store.forget(&Subscription::OrderBook { market: market(1) });
    assert!(store.book(market(1)).is_none());
    assert_eq!(store.apply(6, &Frame::Pong), Folded::Dropped);
}

/// Orders: a snapshot replaces what it covers, an update merges, and an order no longer live
/// leaves. One market's snapshot leaves the others' orders be.
#[test]
fn an_account_s_live_orders_follow_its_channels() {
    let mut store = Store::new();
    let account = AccountIndex::new(10).unwrap();
    let order = |index: u64, market: u64, status: &str| {
        ORDER
            .replace("281476612587355", &index.to_string())
            .replace(r#""market_index": 1"#, &format!(r#""market_index": {market}"#))
            .replace(r#""status": "open""#, &format!(r#""status": "{status}""#))
    };
    let all = |kind: &str, orders: &[String]| {
        decode(&format!(
            r#"{{"channel":"account_all_orders:10","orders":[{}],"type":"{kind}/account_all_orders"}}"#,
            orders.join(",")
        ))
        .unwrap()
    };
    let held = |store: &Store| -> Vec<u64> {
        store
            .account(account)
            .and_then(Account::orders)
            .map(|orders| orders.keys().copied().collect())
            .unwrap_or_default()
    };
    assert_eq!(
        store.apply(1, &all("update", &[order(281_474_976_710_657, 0, "open")])),
        Folded::Dropped,
        "no snapshot yet"
    );
    let first = order(281_474_976_710_657, 0, "open");
    let second = order(562_949_953_421_313, 1, "open");
    assert_eq!(store.apply(2, &all("subscribed", &[first.clone(), second.clone()])), Folded::Changed);
    assert_eq!(held(&store), [281_474_976_710_657, 562_949_953_421_313]);
    assert_eq!(store.apply(3, &all("update", &[order(281_474_976_710_657, 0, "filled")])), Folded::Changed);
    assert_eq!(held(&store), [562_949_953_421_313], "filled: no longer live");
    assert_eq!(store.apply(4, &all("update", std::slice::from_ref(&second))), Folded::Same);

    let one = |orders: &[String]| {
        decode(&format!(r#"{{"account":10,"channel":"account_orders:0","nonce":1,"orders":{{"0":[{}]}},"type":"subscribed/account_orders"}}"#, orders.join(",")))
            .unwrap()
    };
    assert_eq!(store.apply(5, &one(&[first])), Folded::Changed);
    assert_eq!(held(&store), [281_474_976_710_657, 562_949_953_421_313], "market 0's snapshot leaves market 1's");
    assert_eq!(store.apply(6, &one(&[])), Folded::Changed);
    assert_eq!(held(&store), [562_949_953_421_313]);
}

#[test]
fn an_account_s_positions_merge_and_its_trades_are_kept_once() {
    let mut store = Store::new();
    let account = AccountIndex::new(10).unwrap();
    assert_eq!(store.apply(1, &decode(lighter("subscribed/account_all", "")).unwrap()), Folded::Changed);
    let update = lighter("subscribed/account_all", "")
        .replace("subscribed/account_all", "update/account_all")
        .replace(r#""position":"0.00000""#, r#""position":"0.10000""#);
    assert_eq!(store.apply(2, &decode(&update).unwrap()), Folded::Changed);
    let held = store.account(account).unwrap();
    assert_eq!(held.open_positions().map(|position| position.market_id).collect::<Vec<_>>(), [market(1)]);
    let trades = decode(lighter("subscribed/account_all_trades", "")).unwrap();
    assert_eq!(store.apply(3, &trades), Folded::Changed, "the volumes");
    assert_eq!(store.account(account).unwrap().trades().map(|trades| trades.len()), Some(0));
}
