//! One connection's subscriptions, book chains and transactions: what is wanted is sent on every
//! socket, once each, with the token where it is taken; a book that stops chaining is
//! resubscribed; a ping is answered; a transaction waits for its answer by id and is never sent
//! again.

use super::*;
use crate::live::fixtures::{answer, lighter, robinhood};
use crate::live::{BadSubscription, PING, send_tx};
use crate::utils::AccountIndex;

fn market(id: u64) -> MarketId {
    MarketId::new(id).unwrap()
}

fn book(id: u64) -> Subscription {
    Subscription::OrderBook { market: market(id) }
}

fn orders() -> Subscription {
    Subscription::AccountAllOrders { account: AccountIndex::new(10).unwrap() }
}

/// A session names its network's socket, or its read-only one. Subscriptions wait while it is
/// closed and all go out in order on open, on every socket: the venue keeps nothing for a new
/// one. While open a new one goes out at once, and one already wanted does not (the venue answers
/// `Already Subscribed`).
#[test]
fn what_is_wanted_goes_out_on_every_socket_once_each() {
    for network in Network::ALL {
        let session = Session::new(network);
        assert_eq!((session.network(), session.url()), (network, network.ws_url()));
        assert_eq!(Session::new(network).read_only().url(), network.ws_readonly_url());
        assert!(!session.is_open());
    }
    let mut session = Session::new(Network::Lighter);
    assert_eq!(session.subscribe(Subscription::Height), Ok(None));
    assert_eq!(session.subscribe(book(1)), Ok(None));
    assert_eq!(session.subscribe(Subscription::Height), Ok(None), "wanted once");
    let all = vec![subscribe(&Subscription::Height, None), subscribe(&book(1), None)];
    assert_eq!(session.opened(1_000), all);
    assert!(session.is_open());
    let _ = session.closed(2_000, 0);
    assert!(!session.is_open());
    assert_eq!(session.opened(3_000), all);

    assert_eq!(session.subscribe(book(0)), Ok(Some(subscribe(&book(0), None))));
    assert_eq!(session.subscribe(book(0)), Ok(None));
    assert_eq!(session.unsubscribe(&book(1)), Some(unsubscribe(&book(1))));
    assert_eq!(session.unsubscribe(&book(1)), None, "no longer wanted");
    assert_eq!(session.wanted(), [Subscription::Height, book(0)]);
    let _ = session.closed(10_000, 0);
    assert_eq!(session.unsubscribe(&Subscription::Height), None, "nothing to send while closed");
    assert_eq!(session.opened(20_000), [subscribe(&book(0), None)], "and it is not asked for again on open");
}

/// A private channel goes out with the token, on every socket, the newest one set; without a
/// token it is refused before anything goes out. The token never shows in the session's debug.
#[test]
fn a_private_channel_carries_the_newest_token() {
    let mut session = Session::new(Network::Lighter);
    assert_eq!(session.subscribe(orders()), Err(Refused::NoToken("account_all_orders/10".to_owned())));
    session.set_auth(Some("first".to_owned()));
    assert_eq!(session.subscribe(orders()), Ok(None));
    assert_eq!(session.opened(0), [subscribe(&orders(), Some("first"))]);
    let _ = session.closed(1, 0);
    let mut session = session.with_auth("second");
    assert_eq!(session.opened(2), [subscribe(&orders(), Some("second"))]);
    assert!(!format!("{session:?}").contains("second"));
    assert!(session.resubscribe(&orders())[1].contains(r#""auth":"second""#));
}

/// Binary options are Lighter's, live points Robinhood Chain's.
#[test]
fn a_channel_the_network_does_not_serve_is_refused() {
    let mut session = Session::new(Network::Robinhood);
    let refused = session.subscribe(Subscription::BoMarketStats { market: None });
    assert!(matches!(refused, Err(Refused::Bad(BadSubscription::OtherNetwork { .. }))));
    assert!(session.wanted().is_empty());
}

#[test]
fn a_session_holds_no_more_than_its_limits() {
    let limits = Limits { subscriptions: 2, accounts: 1, transactions: 1 };
    let mut session = Session::new(Network::Lighter).with_limits(limits);
    let account = |index| AccountIndex::new(index).unwrap();
    assert_eq!(session.subscribe(Subscription::AccountAll { account: account(1) }), Ok(None));
    assert_eq!(session.subscribe(Subscription::UserStats { account: account(2) }), Err(Refused::Accounts { limit: 1 }));
    assert_eq!(session.subscribe(Subscription::UserStats { account: account(1) }), Ok(None), "the same account");
    assert_eq!(session.subscribe(Subscription::Height), Err(Refused::Subscriptions { limit: 2 }));
    assert_eq!(session.accounts(), 1);
}

/// The recorded book chains: each update begins at the nonce before it. One that does not comes
/// back as a gap, and the session owes the venue an unsubscribe and a subscribe, once; the
/// updates until the fresh snapshot pass unchecked, and the snapshot starts the chain again.
#[test]
fn a_book_that_stops_chaining_is_resubscribed_once() {
    let mut session = Session::new(Network::Lighter);
    session.subscribe(book(1)).unwrap();
    let _ = session.opened(0);
    let snapshot = lighter("subscribed/order_book", "order_book:1");
    assert!(matches!(session.received(1, snapshot), Ok(Frame::Snapshot(_))));
    for (at, update) in
        crate::live::fixtures::all(Network::Lighter, "update/order_book", "order_book:1").into_iter().enumerate()
    {
        assert!(matches!(session.received(2 + at as u64, update), Ok(Frame::Update(_))), "{at}: it chains");
    }
    assert!(session.owed().is_empty());

    let lost =
        lighter("update/order_book", "order_book:1").replace(r#""begin_nonce":25652761941"#, r#""begin_nonce":1"#);
    let expected = 25_652_762_093;
    assert_eq!(session.received(10, &lost), Ok(Frame::Gap(Gap { market: market(1), expected, got: 1 })));
    assert_eq!(session.owed(), [unsubscribe(&book(1)), subscribe(&book(1), None)]);
    assert!(matches!(session.received(11, &lost), Ok(Frame::Update(_))), "awaiting the snapshot: unchecked");
    assert!(session.owed().is_empty(), "owed once");
    assert!(matches!(session.received(12, snapshot), Ok(Frame::Snapshot(_))));
    assert!(matches!(session.received(13, &lost), Ok(Frame::Gap(_))), "the chain starts again from the snapshot");
}

/// A book the session does not want is checked all the same, and nothing is owed for it.
#[test]
fn a_book_not_wanted_is_checked_but_not_resubscribed() {
    let mut session = Session::new(Network::Robinhood);
    let _ = session.opened(0);
    assert!(session.received(1, robinhood("subscribed/order_book", "")).is_ok());
    let lost = robinhood("update/order_book", "").replace(r#""begin_nonce":3931852961"#, r#""begin_nonce":2"#);
    assert!(matches!(session.received(2, &lost), Ok(Frame::Gap(_))));
    assert!(session.owed().is_empty());
}

#[test]
fn the_venue_s_ping_is_owed_a_pong() {
    let mut session = Session::new(Network::Lighter);
    let _ = session.opened(0);
    assert_eq!(session.received(1, r#"{"type":"ping"}"#), Ok(Frame::Ping));
    assert_eq!(session.owed(), [PONG]);
    assert_eq!(session.received(2, r#"{"type":"pong"}"#), Ok(Frame::Pong));
    assert!(session.owed().is_empty());
    assert_ne!(PING, PONG);
    let _ = session.received(3, answer("connected"));
    assert!(session.owed().is_empty());
}

/// A transaction goes out only on an open socket that takes them, gets the next id, and waits
/// for an answer naming it; a drop reports those unanswered and never sends them again.
#[test]
fn transactions_wait_for_their_answer_by_id_and_a_dropped_socket_reports_them_unanswered() {
    let info = r#"{"AccountIndex":10,"Nonce":1}"#;
    let mut session = Session::new(Network::Lighter);
    assert_eq!(session.send_tx_info(14, info), Err(Refused::Closed), "never held to send later");
    let _ = session.opened(0);
    let (one, text) = session.send_tx_info(14, info).unwrap();
    assert_eq!((one, text), (1, send_tx("1", 14, info).unwrap()));
    let (two, _) = session.send_tx_batch_infos(&[(14, info.to_owned()), (15, info.to_owned())]).unwrap();
    let (three, _) = session.send_tx_info(15, info).unwrap();
    assert_eq!(session.waiting(), [1, 2, 3]);
    assert_eq!(two, 2);
    let taken = r#"{"code":200,"id":"1","tx_hash":"ab"}"#;
    assert!(matches!(session.received(10, taken), Ok(Frame::Sent { .. })));
    let refused = r#"{"error":{"code":21104,"message":"invalid nonce"},"id":"2"}"#;
    assert!(matches!(session.received(11, refused), Ok(Frame::Error(_))));
    assert_eq!(session.waiting(), [three]);
    // Unknown, not failed: it may have landed, so it is reported and never resent.
    assert_eq!(session.closed(20, 0).unanswered, [3]);
    assert!(session.waiting().is_empty());
    let _ = session.opened(30);
    assert_eq!(session.send_tx_info(14, info).map(|(id, _)| id), Ok(4), "an id is never reused");
    assert_eq!(session.send_tx_info(14, "not json").map(|(id, _)| id).map_err(|_| ()), Err(()));
    assert_eq!(session.send_tx_batch_infos(&[]), Err(Refused::Batch(0)));
    let many = vec![(14, info.to_owned()); 51];
    assert_eq!(session.send_tx_batch_infos(&many), Err(Refused::Batch(51)));
}

#[test]
fn the_read_only_socket_takes_no_transactions() {
    let mut session = Session::new(Network::Lighter).read_only();
    let _ = session.opened(0);
    assert!(session.is_read_only());
    assert_eq!(session.send_tx_info(14, "{}"), Err(Refused::ReadOnly));
    assert_eq!(session.send_tx_batch_infos(&[(14, "{}".to_owned())]), Err(Refused::ReadOnly));
}

/// A signed transaction goes as signed: its type's number and `tx_info` with its keys in order.
#[cfg(feature = "sign")]
#[test]
fn a_signed_transaction_goes_as_signed() {
    use crate::account::ChangePubKey;
    use crate::test_support::{replay, vector, vector_envelope, vector_key};
    use crate::utils::sign::{PublicKey, sign_drawing};

    let case = vector("register-key-a");
    let (chain_id, envelope) = vector_envelope(&case);
    let tx = ChangePubKey::new(PublicKey::from_hex(case["fields"]["PubKey"].as_str().unwrap()).unwrap());
    let signed = sign_drawing(&tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
    let mut session = Session::new(Network::Lighter);
    let _ = session.opened(0);
    let (id, text) = session.send_tx(&signed).unwrap();
    assert_eq!(text, send_tx(&id.to_string(), 8, &signed.tx_info()).unwrap());
    assert!(text.contains(r#""tx_type":8,"tx_info":{"AccountIndex""#), "{text}");
    let (_, batch) = session.send_tx_batch(std::slice::from_ref(&signed)).unwrap();
    assert!(batch.contains(r#""tx_types":"[8]""#), "{batch}");
}
