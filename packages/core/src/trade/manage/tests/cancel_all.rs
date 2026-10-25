//! `CancelAllOrders`, held to lighter-go: now, in one market, scheduled, aborted, and a market on a
//! scheduled one refused.

use super::*;
use crate::trade::support::{attributes, case, cases, int, signed, signs_as_lighter_go};

const NOW: u64 = 1_790_000_000_000;

/// A case's cancel-all, exactly as its fields and attributes read.
fn cancel_all(case: &serde_json::Value) -> CancelAllOrders {
    let fields = &case["fields"];
    let when = match int(fields, "TimeInForce") {
        0 => CancelAllTime::Now,
        1 => CancelAllTime::At(int(fields, "Time")),
        _ => CancelAllTime::Abort,
    };
    CancelAllOrders { when, attributes: attributes(case) }
}

#[test]
fn every_cancel_all_lighter_go_signs_signs_the_same_here() {
    let mut names = Vec::new();
    for case in cases("CancelAllOrders").iter().filter(|case| signed(case)) {
        signs_as_lighter_go(&cancel_all(case), case);
        names.push(case["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(names, ["cancel-all-now", "cancel-all-one-market", "cancel-all-scheduled", "cancel-all-abort"]);
}

#[test]
fn the_constructors_build_what_lighter_go_signed() {
    assert_eq!(CancelAllOrders::now(), cancel_all(&case("cancel-all-now")));
    assert_eq!(CancelAllOrders::in_market(MarketId::of::<1>()), cancel_all(&case("cancel-all-one-market")));
    assert_eq!(CancelAllOrders::scheduled(NOW + 900_000, NOW), Ok(cancel_all(&case("cancel-all-scheduled"))));
    assert_eq!(CancelAllOrders::abort(), cancel_all(&case("cancel-all-abort")));
}

#[test]
fn a_cancel_all_limited_to_a_market_runs_now_only() {
    let refused = case("refused-cancel-one-market-scheduled");
    assert!(!signed(&refused));
    let scheduled = cancel_all(&refused);
    assert_eq!(scheduled.check().unwrap_err().field, "TimeInForce");
    let aborting = CancelAllOrders::abort().with(Attributes::NONE.cancel_market(MarketId::of::<3>()));
    assert!(aborting.check().is_err());
    assert_eq!(CancelAllOrders::in_market(MarketId::of::<7>()).market(), Some(MarketId::of::<7>()));
    assert_eq!(CancelAllOrders::now().market(), None);
    assert_eq!(CancelAllOrders::at(0).check().unwrap_err().field, "Time");
}

#[test]
fn a_scheduled_cancel_all_is_5_minutes_to_15_days_ahead() {
    assert!(CancelAllOrders::scheduled(NOW + CANCEL_ALL_MOST_MS, NOW).is_ok());
    for at in [NOW + CANCEL_ALL_LEAST_MS - 1, NOW + CANCEL_ALL_MOST_MS + 1, NOW - 1] {
        assert!(matches!(CancelAllOrders::scheduled(at, NOW), Err(TradeError::Expiry { .. })), "{at}");
    }
}

#[test]
fn one_kind_of_market_is_cancelled_one_market_at_a_time_each_once() {
    let markets = [MarketId::of::<1>(), MarketId::of::<0>(), MarketId::of::<1>(), MarketId::of::<24>()];
    let cancels = CancelAllOrders::in_markets(&markets);
    let limited: Vec<Option<MarketId>> = cancels.iter().map(CancelAllOrders::market).collect();
    assert_eq!(limited, [Some(MarketId::of::<1>()), Some(MarketId::of::<0>()), Some(MarketId::of::<24>())]);
    assert!(cancels.iter().all(|cancel| cancel.when == CancelAllTime::Now));
    assert!(CancelAllOrders::in_markets(&[]).is_empty());
}
