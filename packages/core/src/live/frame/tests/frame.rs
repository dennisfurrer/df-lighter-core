//! What a frame says of itself: its data, whether it is a whole, the transaction it answers.

use super::*;
use crate::live::fixtures::lighter;
use crate::live::{Height, decode};

#[test]
fn a_frame_says_whether_it_is_a_whole_and_what_it_answers() {
    let snapshot = decode(lighter("subscribed/height", "")).unwrap();
    assert!(snapshot.is_snapshot());
    assert_eq!(snapshot.data(), Some(&Data::Height(Height { height: 365_159_557, timestamp: 1_792_918_165_000 })));
    let update = decode(lighter("update/height", "")).unwrap();
    assert!(!update.is_snapshot() && update.data().is_some());
    assert_eq!((Frame::Pong.data(), Frame::Pong.answers()), (None, None));
}

#[test]
fn a_gap_says_where_the_book_was_and_where_the_update_began() {
    let gap = Gap { market: MarketId::new(1).unwrap(), expected: 7, got: 9 };
    assert_eq!(
        gap.to_string(),
        "market 1: an order book update began at nonce 9 where the book was at 7; a frame was lost"
    );
}
