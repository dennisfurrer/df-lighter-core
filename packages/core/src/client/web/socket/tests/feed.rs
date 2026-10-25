//! Run as wasm in Node (`just web`), the feed on Node's own `WebSocket`, against a websocket
//! server on 127.0.0.1 started from the test (`server.js`). Its frames were recorded from
//! Lighter's socket on 2026-10-25 (`fixtures/lighter/stream.jsonl`); a transaction's answer is the
//! shape lighter-ts-sdk notes as verified live, with the id the transaction went out with.

use std::{
    future::poll_fn,
    pin::{Pin, pin},
    task::Poll,
};

use crate::client::Network;
use crate::live::fixtures::lighter;
use crate::live::{Data, Frame, Gap, PING, PONG, Refused, Session, Subscription, Timing, subscribe, unsubscribe};
use crate::utils::MarketId;
use js_sys::Promise;
use wasm_bindgen::{JsCast, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

use super::*;

#[wasm_bindgen(module = "/fixtures/web/server.js")]
extern "C" {
    type Server;
    #[wasm_bindgen(static_method_of = Server)]
    fn listen() -> Promise;
    #[wasm_bindgen(method)]
    fn url(this: &Server) -> String;
    #[wasm_bindgen(method)]
    fn accept(this: &Server, ms: u32) -> Promise;
    #[wasm_bindgen(method)]
    fn close(this: &Server);

    type Peer;
    #[wasm_bindgen(method)]
    fn text(this: &Peer, ms: u32) -> Promise;
    #[wasm_bindgen(method)]
    fn send(this: &Peer, text: &str);
    #[wasm_bindgen(method)]
    fn close(this: &Peer);
    #[wasm_bindgen(method)]
    fn ended(this: &Peer, ms: u32) -> Promise;

    type Deadline;
    #[wasm_bindgen(constructor)]
    fn new(ms: u32) -> Deadline;
    #[wasm_bindgen(method, getter)]
    fn promise(this: &Deadline) -> Promise;
    #[wasm_bindgen(method)]
    fn cancel(this: &Deadline);
}

const BOOK: &str = r#"{"channel":"order_book:1","order_book":{"asks":[{"price":"101","size":"1"}],"bids":[{"price":"100","size":"1"}],"nonce":7,"begin_nonce":0},"timestamp":1,"type":"subscribed/order_book"}"#;
const LOST: &str = r#"{"channel":"order_book:1","order_book":{"asks":[],"bids":[{"price":"100","size":"2"}],"nonce":12,"begin_nonce":9},"timestamp":2,"type":"update/order_book"}"#;

/// Quick enough for a test to see every rule fire.
const FAST: Timing =
    Timing { ping_after_ms: 150, silent_ms: 400, steady_ms: 10_000, first_retry_ms: 20, last_retry_ms: 40 };

/// As quick to dial again, but no ping in the time a test watches for nothing.
const QUIET: Timing = Timing { ping_after_ms: 10_000, silent_ms: 20_000, ..FAST };

const WAIT: u32 = 5_000;

fn book() -> Subscription {
    Subscription::OrderBook { market: MarketId::new(1).unwrap() }
}

async fn listen() -> Server {
    JsFuture::from(Server::listen()).await.unwrap().unchecked_into()
}

async fn accept(server: &Server) -> Peer {
    JsFuture::from(server.accept(WAIT)).await.unwrap().unchecked_into()
}

async fn text(peer: &Peer) -> String {
    JsFuture::from(peer.text(WAIT)).await.unwrap().as_string().unwrap()
}

/// `future`'s output, or `None` when `ms` passes first.
async fn within<T>(ms: u32, future: impl Future<Output = T>) -> Option<T> {
    let deadline = Deadline::new(ms);
    let mut future = pin!(future);
    let mut timeout = JsFuture::from(deadline.promise());
    let out = poll_fn(|cx| match future.as_mut().poll(cx) {
        Poll::Ready(out) => Poll::Ready(Some(out)),
        Poll::Pending => match Pin::new(&mut timeout).poll(cx) {
            Poll::Ready(_) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        },
    })
    .await;
    deadline.cancel();
    out
}

async fn event(feed: &mut Feed) -> Event {
    within(WAIT, feed.next()).await.expect("an event in time").expect("a running feed")
}

async fn delivered(feed: &mut Feed) -> (Event, Option<String>) {
    within(WAIT, feed.next_with_text()).await.expect("an event in time").expect("a running feed")
}

#[wasm_bindgen_test]
async fn it_subscribes_on_open_and_hands_back_typed_frames() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter));
    feed.subscribe(Subscription::Height).await.unwrap();
    let peer = accept(&server).await;
    assert_eq!(text(&peer).await, subscribe(&Subscription::Height, None));
    peer.send(lighter("subscribed/height", ""));
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Snapshot(Data::Height(height)) => assert_eq!(height.height, 365_159_557),
            other => panic!("expected the height, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
    feed.close();
    server.close();
}

/// What a feed reads, it can also give as sent, for a program that reads some channels itself;
/// an open or a close has no text.
#[wasm_bindgen_test]
async fn each_frame_comes_with_the_text_it_arrived_as_when_asked() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter));
    let peer = accept(&server).await;
    assert_eq!(delivered(&mut feed).await, (Event::Opened, None));
    peer.send(lighter("subscribed/height", ""));
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Frame(frame) if frame.is_snapshot()));
    assert_eq!(text.as_deref(), Some(lighter("subscribed/height", "")));
    peer.send("not json");
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Unreadable(_)));
    assert_eq!(text.as_deref(), Some("not json"), "what did not read too");
    peer.close();
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Closed { .. }));
    assert_eq!(text, None);
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_dropped_socket_is_dialled_again_and_everything_resubscribed() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter).with_timing(QUIET));
    feed.subscribe(Subscription::Height).await.unwrap();
    feed.subscribe(book()).await.unwrap();
    let first = accept(&server).await;
    assert_eq!(text(&first).await, subscribe(&Subscription::Height, None));
    assert_eq!(text(&first).await, subscribe(&book(), None));
    first.close();
    let second = accept(&server).await;
    assert_eq!(text(&second).await, subscribe(&Subscription::Height, None));
    assert_eq!(text(&second).await, subscribe(&book(), None));
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Closed { retry, .. } => {
            assert!((20..=25).contains(&retry.after_ms), "the first delay and a quarter: {}", retry.after_ms);
            assert!(retry.unanswered.is_empty());
        }
        other => panic!("the drop is reported before the next open, got {other:?}"),
    }
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    server.close();
}

/// The venue closes a socket it has had no frame from for two minutes, however busy: the ping
/// goes out on the clock. The venue's own ping is answered at once.
#[wasm_bindgen_test]
async fn a_socket_is_pinged_on_the_clock_and_answers_the_venue_s_ping() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter).with_timing(FAST));
    feed.subscribe(Subscription::Height).await.unwrap();
    let peer = accept(&server).await;
    text(&peer).await;
    assert_eq!(text(&peer).await, PING);
    peer.send(r#"{"type":"pong"}"#);
    assert_eq!(event(&mut feed).await, Event::Opened);
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Pong)));
    peer.send(r#"{"type":"ping"}"#);
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Ping)));
    let mut answered = text(&peer).await;
    if answered == PING {
        answered = text(&peer).await;
    }
    assert_eq!(answered, PONG);
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_socket_nothing_arrives_on_is_given_up_and_dialled_again() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter).with_timing(FAST));
    let silent = accept(&server).await;
    assert_eq!(text(&silent).await, PING, "pinged first");
    let _again = accept(&server).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Closed { reason, .. } => assert_eq!(reason, "nothing arrived for too long"),
        other => panic!("expected the silence, got {other:?}"),
    }
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    server.close();
}

/// A book update that does not follow is handed back as a gap, and the feed asks the venue for a
/// fresh snapshot at once.
#[wasm_bindgen_test]
async fn a_book_gap_is_resubscribed_at_once() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter).with_timing(QUIET));
    feed.subscribe(book()).await.unwrap();
    let peer = accept(&server).await;
    assert_eq!(text(&peer).await, subscribe(&book(), None));
    assert_eq!(event(&mut feed).await, Event::Opened);
    peer.send(BOOK);
    assert!(matches!(event(&mut feed).await, Event::Frame(frame) if frame.is_snapshot()));
    peer.send(LOST);
    let gap = Gap { market: MarketId::new(1).unwrap(), expected: 7, got: 9 };
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Gap(gap))));
    assert_eq!(text(&peer).await, unsubscribe(&book()));
    assert_eq!(text(&peer).await, subscribe(&book(), None));
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_frame_that_does_not_read_leaves_the_socket_open() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter));
    let peer = accept(&server).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    peer.send("not json");
    assert!(matches!(event(&mut feed).await, Event::Unreadable(_)));
    peer.send(r#"{"type":"pong"}"#);
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Pong)));
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_transaction_goes_out_with_its_id_and_its_answer_comes_back() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter));
    let peer = accept(&server).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    let info = r#"{"AccountIndex":10,"Nonce":1}"#;
    let id = feed.send_tx_info(14, info.to_owned()).await.unwrap();
    assert_eq!(text(&peer).await, crate::live::send_tx(&id.to_string(), 14, info).unwrap());
    peer.send(&format!(r#"{{"code":200,"id":"{id}","tx_hash":"b5b63eaa","predicted_execution_time_ms":1}}"#));
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Sent { id: answered, sent } => {
                assert_eq!((answered, sent.tx_hash.as_str()), (id.to_string(), "b5b63eaa"));
            }
            other => panic!("expected the answer, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn an_unanswered_transaction_is_reported_on_close_and_never_sent_again() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter).with_timing(QUIET));
    feed.subscribe(Subscription::Height).await.unwrap();
    let first = accept(&server).await;
    text(&first).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    let id = feed.send_tx_info(14, r#"{"Nonce":1}"#.to_owned()).await.unwrap();
    text(&first).await;
    first.close();
    match event(&mut feed).await {
        Event::Closed { retry, .. } => assert_eq!(retry.unanswered, vec![id]),
        other => panic!("expected the close, got {other:?}"),
    }
    let second = accept(&server).await;
    assert_eq!(text(&second).await, subscribe(&Subscription::Height, None));
    assert!(JsFuture::from(second.text(300)).await.is_err(), "the transaction is not sent again");
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_socket_that_cannot_open_refuses_transactions_and_is_dialled_again_later_each_time() {
    let server = listen().await;
    let url = server.url();
    server.close();
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter).with_timing(FAST));
    assert_eq!(feed.send_tx_info(14, "{}".to_owned()).await, Err(FeedError::Refused(Refused::Closed)));
    let mut delays = Vec::new();
    for _ in 0..2 {
        match event(&mut feed).await {
            Event::Closed { reason, retry } => {
                assert!(!reason.is_empty());
                assert!(retry.unanswered.is_empty());
                delays.push(retry.after_ms);
            }
            other => panic!("expected the failed dial, got {other:?}"),
        }
    }
    assert!((20..=25).contains(&delays[0]), "the first delay and a quarter: {delays:?}");
    assert!((40..=50).contains(&delays[1]), "doubled: {delays:?}");
    feed.close();
}

#[wasm_bindgen_test]
async fn a_closed_feed_closes_its_socket_and_dials_no_more() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new(Network::Lighter).with_timing(QUIET));
    let peer = accept(&server).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    JsFuture::from(peer.ended(WAIT)).await.expect("the socket ends");
    assert!(JsFuture::from(server.accept(300)).await.is_err(), "no dial after the close");
    server.close();
}
