//! PnL and funding: account 10's daily chart and the day's leaderboard, recorded on Lighter; the
//! venue's refusal of `positionFunding` without a token, recorded; and a funding page shaped from
//! the docs' `PositionFundings`.

use super::*;
use crate::test_support::{Recorded, block_on};

fn ten() -> AccountIndex {
    AccountIndex::new(10).unwrap()
}

#[test]
fn a_recorded_chart_reads_every_figure_as_written() {
    let transport = Recorded::answering(include_str!("../../../../fixtures/lighter/pnl_10.json"));
    let query = PnlQuery {
        account: ten(),
        resolution: Resolution::Day,
        from_ms: 1_790_000_000_000,
        to_ms: 1_792_900_000_000,
        count: 3,
        ignore_transfers: true,
    };
    let chart = block_on(pnl(&transport, &query)).unwrap();
    assert_eq!((chart.resolution.as_str(), chart.pnl.len()), ("1d", 35));
    let first = &chart.pnl[0];
    assert_eq!(first.timestamp, 1_790_035_200);
    assert_eq!(first.trade_pnl.as_ref().map(Figure::as_str), Some("-11394.109243"));
    assert_eq!(first.staking_inflow.as_ref().map(Figure::as_str), Some("215.4081"));
    assert_eq!(first.pool_total_shares.as_ref().map(Figure::as_str), Some("0"));
    assert_eq!(first.staked_lit, None, "absent is none");
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("pnl", true));
    assert_eq!(
        call.fields,
        [
            ("by", "index".to_owned()),
            ("value", "10".to_owned()),
            ("resolution", "1d".to_owned()),
            ("start_timestamp", "1790000000000".to_owned()),
            ("end_timestamp", "1792900000000".to_owned()),
            ("count_back", "3".to_owned()),
            ("ignore_transfers", "true".to_owned()),
        ]
    );
}

#[test]
fn funding_is_refused_without_a_token_and_reads_with_one() {
    let refused = Recorded::answering(include_str!("../../../../fixtures/lighter/positionFunding_no_token.json"));
    let error = block_on(position_funding(&refused, &FundingQuery::new(ten()))).unwrap_err();
    assert_eq!(error.code(), Some(20_001));

    let transport = Recorded::answering(
        r#"{"code":200,"position_fundings":[{"timestamp":1790035200,"market_id":1,"funding_id":881,
        "change":"-0.412300","discount":"0","rate":"0.0012","position_size":"0.50000","position_side":"long"}],
        "next_cursor":"f2"}"#,
    );
    let query = FundingQuery {
        markets: vec![MarketId::new(1).unwrap(), MarketId::new(2).unwrap()],
        side: Some(Side::Ask),
        from: Some(1_790_000_000_000),
        ..FundingQuery::new(ten())
    };
    let page = block_on(position_funding(&transport, &query)).unwrap();
    let paid = &page.position_fundings[0];
    assert_eq!((paid.change.to_string(), paid.side(), paid.funding_id), ("-0.4123".to_owned(), Some(Side::Bid), 881));
    assert_eq!(page.next_cursor.as_deref(), Some("f2"));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("positionFunding", true));
    assert_eq!(
        call.fields,
        [
            ("account_index", "10".to_owned()),
            ("limit", "100".to_owned()),
            ("side", "short".to_owned()),
            ("start_timestamp", "1790000000000".to_owned()),
            ("market_ids", "1,2".to_owned()),
        ]
    );
}

#[test]
fn the_recorded_leaderboard_reads_its_floats_as_written() {
    let transport = Recorded::answering(include_str!("../../../../fixtures/lighter/pnlLeaderboard_24h.json"));
    let board = block_on(pnl_leaderboard(&transport, Window::Day, RankBy::Pnl, 3, 0)).unwrap();
    let first = &board.entries[0];
    assert_eq!((first.rank, first.pnl.as_str(), first.roi.as_str()), (1, "944202.55171", "3.441318855461209"));
    assert_eq!(first.l1_address.to_hex(), "0x2ba553d9f990a3b66b03b2dc0d030dfc1c061036");
    assert_eq!(board.entries.len(), 3);
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("pnlLeaderboard", false));
    assert_eq!(
        call.fields,
        [
            ("time_window", "24h".to_owned()),
            ("sort_by", "pnl".to_owned()),
            ("sort_dir", "desc".to_owned()),
            ("limit", "3".to_owned()),
            ("offset", "0".to_owned()),
        ]
    );
    assert_eq!(
        pnl_leaderboard_call(Window::All, RankBy::AccountValue, true, 200, 7, Some("0x2b")).value("limit"),
        Some("100")
    );
}
