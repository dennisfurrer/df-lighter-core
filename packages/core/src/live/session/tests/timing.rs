//! The session's clock: a ping every thirty seconds however busy the socket, a dead socket after
//! sixty of nothing, and reconnect delays that double, jitter, and start over after a socket that
//! held.

use super::*;
use crate::client::Network;
use crate::live::fixtures::lighter;
use crate::live::{Frame, PING};

fn open(at: u64) -> Session {
    let mut session = Session::new(Network::Lighter);
    let _ = session.opened(at);
    session
}

/// The venue closes a socket it has had no frame from for two minutes, however much it sends:
/// a busy book does not keep it. So the ping goes out every thirty seconds, busy or quiet, once
/// each time; `due` says when a tick next has work.
#[test]
fn a_ping_goes_out_every_thirty_seconds_busy_or_quiet() {
    let mut session = Session::new(Network::Lighter);
    assert_eq!((session.due(), session.tick(0)), (None, Tick::Wait), "closed");
    let _ = session.opened(1_000);
    assert_eq!(session.due(), Some(31_000));
    assert_eq!(session.tick(30_999), Tick::Wait);
    assert_eq!(session.tick(31_000), Tick::Ping);
    assert_eq!(session.tick(31_500), Tick::Wait, "one ping, not one a tick");
    assert_eq!(session.received(31_200, r#"{"type":"pong"}"#), Ok(Frame::Pong));
    assert_eq!(session.due(), Some(61_000));
    assert_eq!(PING, r#"{"type":"ping"}"#);

    let mut session = open(0);
    let mut pings = 0;
    for at in (1_000..120_000).step_by(1_000) {
        assert!(session.received(at, lighter("update/height", "")).is_ok());
        if session.tick(at) == Tick::Ping {
            pings += 1;
        }
    }
    assert_eq!(pings, 3, "at 30, 60 and 90 seconds, though a frame came every second");
}

#[test]
fn sixty_seconds_of_nothing_pong_included_is_a_dead_socket() {
    let mut session = open(0);
    assert_eq!(session.tick(30_000), Tick::Ping);
    assert_eq!(session.tick(59_999), Tick::Wait);
    assert_eq!(session.tick(60_000), Tick::Reconnect);
    assert_eq!(session.tick(90_000), Tick::Reconnect, "until the socket is closed");
    let _ = session.closed(90_001, 0);
    assert_eq!(session.tick(200_000), Tick::Wait, "a closed socket waits for its reconnect");

    // Anything that arrives counts, even what does not read.
    let mut session = open(0);
    assert!(session.received(50_000, "Websocket connection established.").is_err());
    assert_eq!(session.tick(59_999), Tick::Ping);
    assert_eq!(session.tick(89_998), Tick::Wait);
    assert_eq!(session.tick(110_000), Tick::Reconnect, "sixty seconds after it arrived");
}

/// 250 ms doubling to 20 s, plus up to a quarter more, and never a last attempt. A socket that
/// held thirty seconds and heard something starts the delays over; one that heard nothing, or
/// dropped sooner, keeps backing off.
#[test]
fn reconnects_back_off_and_start_over_after_a_socket_that_held() {
    let mut session = Session::new(Network::Lighter);
    let delays: Vec<u64> = (0..9).map(|_| session.closed(0, 0).after_ms).collect();
    assert_eq!(delays, [250, 500, 1_000, 2_000, 4_000, 8_000, 16_000, 20_000, 20_000]);
    assert_eq!(session.closed(0, u64::MAX).after_ms, 20_000 + u64::MAX % 5_001, "up to a quarter more");

    let _ = session.opened(0);
    assert_eq!(session.closed(40_000, 0).after_ms, 20_000, "heard nothing: no proof it worked");
    let _ = session.opened(50_000);
    let _ = session.received(51_000, r#"{"type":"pong"}"#);
    assert_eq!(session.closed(60_000, 0).after_ms, 20_000, "dropped before thirty seconds");
    let _ = session.opened(70_000);
    let _ = session.received(71_000, r#"{"type":"pong"}"#);
    assert_eq!(session.closed(100_000, 0).after_ms, 250, "held: start over");
    let timing = Timing { first_retry_ms: 10, ..Timing::default() };
    assert_eq!(Session::new(Network::Lighter).with_timing(timing).closed(0, 0).after_ms, 10);
}
