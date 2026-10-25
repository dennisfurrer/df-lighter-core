//! `ModifyOrder`, held to lighter-go: a limit's size and price, a stop's trigger, a version that
//! must be passed; and built from decimals on the grid.

use super::*;
use crate::trade::support::{attributes, case, cases, d, int, signs_as_lighter_go};
use crate::utils::Round;

const BTC: Grid = Grid::new(5, 1);

/// A case's modify, exactly as its fields read.
fn modify_of(case: &serde_json::Value) -> ModifyOrder {
    let fields = &case["fields"];
    ModifyOrder {
        market: MarketId::new(int(fields, "MarketIndex")).unwrap(),
        order: OrderRef::from_index(int(fields, "Index")).unwrap(),
        base_amount: int(fields, "BaseAmount"),
        price: u32::try_from(int(fields, "Price")).unwrap(),
        trigger_price: u32::try_from(int(fields, "TriggerPrice")).unwrap(),
        attributes: attributes(case),
    }
}

#[test]
fn every_modify_lighter_go_signs_signs_the_same_here() {
    let mut names = Vec::new();
    for case in cases("ModifyOrder") {
        signs_as_lighter_go(&modify_of(&case), &case);
        names.push(case["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(names, ["modify-limit", "modify-trigger", "modify-with-version"]);
}

#[test]
fn a_modify_built_from_decimals_is_the_one_lighter_go_signed() {
    let btc = MarketId::of::<1>();
    let client = OrderRef::from_index(1_790_000_000_000).unwrap();
    let limit = ModifyIntent::new(btc, client, Side::Bid, d("0.02"), d("83400")).build(BTC);
    assert_eq!(limit, Ok(modify_of(&case("modify-limit"))));
    let stop = ModifyIntent::new(btc, OrderRef::from_index(11).unwrap(), Side::Ask, d("0.0002"), d("78000"))
        .trigger(d("79500"))
        .build(BTC);
    assert_eq!(stop, Ok(modify_of(&case("modify-trigger"))));
    let venue = OrderRef::from_index(562_953_635_421_737).unwrap();
    let versioned = ModifyIntent::new(btc, venue, Side::Bid, d("0.02"), d("83400")).build(BTC).unwrap().version(7);
    assert_eq!(versioned, modify_of(&case("modify-with-version")));
}

#[test]
fn a_modify_finer_than_the_grid_is_refused_unless_a_fit_is_given() {
    let btc = MarketId::of::<1>();
    let order = OrderRef::from_index(11).unwrap();
    let fine = ModifyIntent::new(btc, order, Side::Ask, d("0.000201"), d("78000.05"));
    assert!(matches!(fine.build(BTC), Err(TradeError::Units { field: "BaseAmount", .. })));
    let placed = fine.fit(Fit::Favour).build(BTC).unwrap();
    assert_eq!((placed.base_amount, placed.price), (20, 780_001), "a sell's price up");
    let down = fine.fit(Fit::Round(Round::Down)).trigger(d("79500.05")).build(BTC).unwrap();
    assert_eq!((down.price, down.trigger_price), (780_000, 795_000));
}

#[test]
fn a_modify_is_refused_by_lighter_go_s_rules() {
    let limit = modify_of(&case("modify-limit"));
    assert_eq!(ModifyOrder { price: 0, ..limit }.check().unwrap_err().field, "Price");
    assert_eq!(ModifyOrder { base_amount: 1 << 48, ..limit }.check().unwrap_err().field, "BaseAmount");
    ModifyOrder { base_amount: 0, ..limit }.check().unwrap();
    let too_new = limit.version(1 << 48);
    assert_eq!(too_new.check().unwrap_err().field, "L2TxAttributes");
    let with_integrator =
        limit.with(Attributes::NONE.integrator(crate::utils::AccountIndex::of::<713_888>(), 0, 0)).version(2);
    assert_eq!(with_integrator.attributes().entries(), [(1, 713_888), (8, 2)]);
}
