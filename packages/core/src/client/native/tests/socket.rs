//! Against a websocket server on 127.0.0.1. Its frames were recorded from Lighter's socket on
//! 2026-10-25 (`fixtures/lighter/stream.jsonl`); a transaction's answer is the shape
//! lighter-ts-sdk notes as verified live, with the id the transaction went out with.

use std::time::Duration;

use crate::client::Network;
use crate::live::{Data, Frame, Gap, PONG, Refused, Session, Subscription, Timing, subscribe, unsubscribe};
use crate::utils::MarketId;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

use super::*;

const HEIGHT: &str = r#"{"channel":"height","height":365159557,"timestamp":1792918165000,"type":"subscribed/height"}"#;
const BOOK: &str = r#"{"channel":"order_book:1","order_book":{"asks":[{"price":"101","size":"1"}],"bids":[{"price":"100","size":"1"}],"nonce":7,"begin_nonce":0},"timestamp":1,"type":"subscribed/order_book"}"#;
const LOST: &str = r#"{"channel":"order_book:1","order_book":{"asks":[],"bids":[{"price":"100","size":"2"}],"nonce":12,"begin_nonce":9},"timestamp":2,"type":"update/order_book"}"#;

/// Quick enough for a test to see every rule fire.
const FAST: Timing =
    Timing { ping_after_ms: 150, silent_ms: 400, steady_ms: 10_000, first_retry_ms: 20, last_retry_ms: 40 };

const WAIT: Duration = Duration::from_secs(5);

fn book() -> Subscription {
    Subscription::OrderBook { market: MarketId::new(1).unwrap() }
}

async fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    (listener, url)
}

async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (stream, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
    accept_async(stream).await.unwrap()
}

async fn text(socket: &mut WebSocketStream<TcpStream>) -> String {
    loop {
        match timeout(WAIT, socket.next()).await.unwrap().unwrap().unwrap() {
            Message::Text(text) => return text.to_string(),
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("expected text, got {other:?}"),
        }
    }
}

async fn event(feed: &mut Feed) -> Event {
    timeout(WAIT, feed.next()).await.unwrap().unwrap()
}

async fn delivered(feed: &mut Feed) -> (Event, Option<String>) {
    timeout(WAIT, feed.next_with_text()).await.unwrap().unwrap()
}

#[tokio::test]
async fn it_subscribes_on_open_and_hands_back_typed_frames() {
    let (listener, url) = listen().await;
    let feed_session = Session::new(Network::Lighter);
    let mut feed = Feed::start_at(url, feed_session);
    feed.subscribe(Subscription::Height).await.unwrap();
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, subscribe(&Subscription::Height, None));
    server.send(Message::text(HEIGHT)).await.unwrap();
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Snapshot(Data::Height(height)) => assert_eq!(height.height, 365_159_557),
            other => panic!("expected the height, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
}

/// What a feed reads, it can also give as sent, for a program that reads some channels itself;
/// an open or a close has no text.
#[tokio::test]
async fn each_frame_comes_with_the_text_it_arrived_as_when_asked() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter));
    let mut server = accept(&listener).await;
    server.send(Message::text(HEIGHT)).await.unwrap();
    server.send(Message::text("not json")).await.unwrap();
    assert_eq!(delivered(&mut feed).await, (Event::Opened, None));
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Frame(frame) if frame.is_snapshot()));
    assert_eq!(text.as_deref(), Some(HEIGHT));
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Unreadable(_)));
    assert_eq!(text.as_deref(), Some("not json"), "what did not read too");
    drop(server);
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Closed { .. }));
    assert_eq!(text, None);
}

#[tokio::test]
async fn a_dropped_socket_is_dialled_again_and_everything_resubscribed() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter).with_timing(FAST));
    feed.subscribe(Subscription::Height).await.unwrap();
    feed.subscribe(book()).await.unwrap();
    let mut first = accept(&listener).await;
    assert_eq!(text(&mut first).await, subscribe(&Subscription::Height, None));
    assert_eq!(text(&mut first).await, subscribe(&book(), None));
    first.close(None).await.unwrap();
    drop(first);
    let mut second = accept(&listener).await;
    assert_eq!(text(&mut second).await, subscribe(&Subscription::Height, None));
    assert_eq!(text(&mut second).await, subscribe(&book(), None));
    let mut saw_closed = false;
    while let Ok(Some(event)) = timeout(Duration::from_millis(200), feed.next()).await {
        saw_closed |= matches!(event, Event::Closed { .. });
    }
    assert!(saw_closed, "the drop is reported before the next open");
}

/// The venue closes a socket it has had no frame from for two minutes, however busy: the ping
/// goes out on the clock.
#[tokio::test]
async fn a_socket_is_pinged_on_the_clock() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter).with_timing(FAST));
    feed.subscribe(Subscription::Height).await.unwrap();
    let mut server = accept(&listener).await;
    text(&mut server).await;
    assert_eq!(text(&mut server).await, PING);
    server.send(Message::text(r#"{"type":"pong"}"#)).await.unwrap();
    assert_eq!(event(&mut feed).await, Event::Opened);
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Pong)));
}

#[tokio::test]
async fn the_venue_s_ping_is_answered_at_once() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter));
    let mut server = accept(&listener).await;
    server.send(Message::text(r#"{"type":"ping"}"#)).await.unwrap();
    assert_eq!(text(&mut server).await, PONG);
    assert_eq!(event(&mut feed).await, Event::Opened);
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Ping)));
}

/// A book update that does not follow is handed back as a gap, and the feed asks the venue for a
/// fresh snapshot at once.
#[tokio::test]
async fn a_book_gap_is_resubscribed_at_once() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter));
    feed.subscribe(book()).await.unwrap();
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, subscribe(&book(), None));
    server.send(Message::text(BOOK)).await.unwrap();
    server.send(Message::text(LOST)).await.unwrap();
    assert_eq!(text(&mut server).await, unsubscribe(&book()));
    assert_eq!(text(&mut server).await, subscribe(&book(), None));
    assert_eq!(event(&mut feed).await, Event::Opened);
    assert!(matches!(event(&mut feed).await, Event::Frame(frame) if frame.is_snapshot()));
    let gap = Gap { market: MarketId::new(1).unwrap(), expected: 7, got: 9 };
    assert_eq!(event(&mut feed).await, Event::Frame(Box::new(Frame::Gap(gap))));
}

#[tokio::test]
async fn a_socket_nothing_arrives_on_is_given_up_and_dialled_again() {
    let (listener, url) = listen().await;
    let _feed = Feed::start_at(url, Session::new(Network::Lighter).with_timing(FAST));
    let silent = accept(&listener).await;
    let again = timeout(WAIT, listener.accept()).await;
    assert!(again.is_ok(), "a second connection after the silence");
    drop(silent);
}

#[tokio::test]
async fn a_transaction_goes_out_with_its_id_and_its_answer_comes_back() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter));
    let mut server = accept(&listener).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    let info = r#"{"AccountIndex":10,"Nonce":1}"#;
    let id = feed.send_tx_info(14, info.to_owned()).await.unwrap();
    assert_eq!(text(&mut server).await, crate::live::send_tx(&id.to_string(), 14, info).unwrap());
    let answer = format!(r#"{{"code":200,"id":"{id}","tx_hash":"b5b63eaa","predicted_execution_time_ms":1}}"#);
    server.send(Message::text(answer)).await.unwrap();
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Sent { id: answered, sent } => {
                assert_eq!((answered, sent.tx_hash.as_str()), (id.to_string(), "b5b63eaa"));
            }
            other => panic!("expected the answer, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
}

#[tokio::test]
async fn a_transaction_is_refused_while_the_socket_is_closed_or_read_only() {
    let (listener, url) = listen().await;
    drop(listener);
    let feed = Feed::start_at(url, Session::new(Network::Lighter).with_timing(FAST));
    assert_eq!(feed.send_tx_info(14, "{}".to_owned()).await, Err(FeedError::Refused(Refused::Closed)));
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new(Network::Lighter).read_only());
    let _server = accept(&listener).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    assert_eq!(feed.send_tx_info(14, "{}".to_owned()).await, Err(FeedError::Refused(Refused::ReadOnly)));
}

#[tokio::test]
async fn a_private_channel_without_a_token_is_refused_and_one_set_later_goes_out() {
    let (listener, url) = listen().await;
    let feed = Feed::start_at(url, Session::new(Network::Lighter));
    let orders = Subscription::AccountAllOrders { account: crate::utils::AccountIndex::new(10).unwrap() };
    assert!(matches!(feed.subscribe(orders.clone()).await, Err(FeedError::Refused(Refused::NoToken(_)))));
    feed.set_auth(Some("token".to_owned())).unwrap();
    feed.subscribe(orders.clone()).await.unwrap();
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, subscribe(&orders, Some("token")));
}

#[tokio::test]
async fn a_closed_feed_refuses_everything_after() {
    let (_listener, url) = listen().await;
    let feed = Feed::start_at(url, Session::new(Network::Lighter));
    let commands = feed.commands.clone();
    feed.close();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(commands.is_closed(), "the driver stopped");
}
