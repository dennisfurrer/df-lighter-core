//! Points on Robinhood Chain: the weekly leaderboard recorded (empty that week), entries shaped
//! from the docs' `Leaderboard`, and live points (token needed) from `LivePointsTotal`.

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn the_recorded_weekly_board_is_empty_and_entries_read_as_written() {
    let transport = Recorded::answering(include_str!("../../../../fixtures/robinhood/leaderboard_weekly.json"));
    assert_eq!(block_on(points_leaderboard(&transport, &Board::Weekly, None)), Ok(Vec::new()));
    assert_eq!((transport.only().path, transport.only().fields), ("leaderboard", vec![("type", "weekly".to_owned())]));

    let transport = Recorded::answering(
        r#"{"entries":[{"l1_address":"0x2Ba553d9F990a3B66b03b2dC0D030dfC1c061036","points":1520.75,"entry":3,
        "entryId":17,"metadata":""}]}"#,
    );
    let owner: Address = "0x2Ba553d9F990a3B66b03b2dC0D030dfC1c061036".parse().unwrap();
    let board = Board::Competition("spring".to_owned());
    let entries = block_on(points_leaderboard(&transport, &board, Some(owner))).unwrap();
    assert_eq!((entries[0].points.as_str(), entries[0].entry_id), ("1520.75", 17));
    assert_eq!(
        transport.only().fields,
        [
            ("type", "competition".to_owned()),
            ("l1_address", "0x2ba553d9f990a3b66b03b2dc0d030dfc1c061036".to_owned()),
            ("competition_id", "spring".to_owned()),
        ]
    );
}

#[test]
fn live_points_are_the_account_s_own() {
    let transport = Recorded::answering(r#"{"code":200,"total_live_points":42.125}"#);
    let points = block_on(live_points(&transport, AccountIndex::new(100_002).unwrap())).unwrap();
    assert_eq!(points.as_str(), "42.125");
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/livePoints/total".to_owned(), true));
    assert_eq!(Board::AllTime.name(), "all");
}
