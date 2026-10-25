//! `UpdateMargin`, held to lighter-go: add, remove, and an amount past 2^32 millionths.

use super::*;
use crate::trade::support::{case, cases, d, int, signs_as_lighter_go};
use crate::utils::BadUnits;

/// A case's update, exactly as its fields read.
fn update_of(case: &serde_json::Value) -> UpdateMargin {
    let fields = &case["fields"];
    let direction = match int(fields, "Direction") {
        1 => MarginDirection::Add,
        _ => MarginDirection::Remove,
    };
    UpdateMargin {
        market: MarketId::new(int(fields, "MarketIndex")).unwrap(),
        amount: int(fields, "USDCAmount"),
        direction,
        attributes: Attributes::NONE,
    }
}

#[test]
fn every_margin_update_lighter_go_signs_signs_the_same_here() {
    let mut names = Vec::new();
    for case in cases("UpdateMargin") {
        signs_as_lighter_go(&update_of(&case), &case);
        names.push(case["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(names, ["margin-add", "margin-remove", "margin-above-2^32"]);
}

#[test]
fn collateral_is_counted_in_millionths_exactly() {
    let eth = MarketId::of::<0>();
    assert_eq!(UpdateMargin::add(eth, d("25")), Ok(update_of(&case("margin-add"))));
    assert_eq!(UpdateMargin::remove(eth, d("5")), Ok(update_of(&case("margin-remove"))));
    assert_eq!(UpdateMargin::add(eth, d("12345678.901234")), Ok(update_of(&case("margin-above-2^32"))));
    let fine = UpdateMargin::add(eth, d("1.0000001"));
    assert_eq!(
        fine,
        Err(TradeError::Units { field: "USDCAmount", error: BadUnits::TooFine { value: d("1.0000001"), places: 6 } })
    );
    assert!(matches!(UpdateMargin::remove(eth, Dec::ZERO), Err(TradeError::Units { .. })));
}

#[test]
fn an_update_is_refused_by_lighter_go_s_rules() {
    let add = update_of(&case("margin-add"));
    assert_eq!(UpdateMargin { amount: 0, ..add }.check().unwrap_err().field, "USDCAmount");
    assert_eq!(UpdateMargin { amount: 1 << 60, ..add }.check().unwrap_err().field, "USDCAmount");
    UpdateMargin { amount: (1 << 60) - 1, ..add }.check().unwrap();
}
