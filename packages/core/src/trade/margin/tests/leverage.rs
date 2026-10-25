//! `UpdateLeverage`, held to lighter-go: cross at 10x, isolated at 3x, a zero fraction refused;
//! leverage as a fraction, and the market's ceiling and isolated-only rule.

use super::*;
use crate::trade::support::{case, cases, int, signed, signs_as_lighter_go};

/// A case's update, exactly as its fields read.
fn update_of(case: &serde_json::Value) -> UpdateLeverage {
    let fields = &case["fields"];
    UpdateLeverage {
        market: MarketId::new(int(fields, "MarketIndex")).unwrap(),
        initial_margin_fraction: u16::try_from(int(fields, "InitialMarginFraction")).unwrap(),
        mode: MarginMode::from_code(u8::try_from(int(fields, "MarginMode")).unwrap()).unwrap(),
        attributes: Attributes::NONE,
    }
}

#[test]
fn every_leverage_lighter_go_signs_signs_the_same_here_and_a_zero_fraction_is_refused() {
    let mut names = Vec::new();
    for case in cases("UpdateLeverage").iter().filter(|case| signed(case)) {
        signs_as_lighter_go(&update_of(case), case);
        names.push(case["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(names, ["leverage-cross-10x", "leverage-isolated-3x"]);
    let zero = update_of(&case("refused-leverage-zero"));
    assert_eq!(zero.check().unwrap_err().field, "InitialMarginFraction");
    let whole = UpdateLeverage { initial_margin_fraction: 10_001, ..zero };
    assert_eq!(whole.check().unwrap_err().field, "InitialMarginFraction");
}

#[test]
fn leverage_is_10000_over_it_rounded_down() {
    assert_eq!(
        UpdateLeverage::new(MarketId::of::<1>(), 10, MarginMode::Cross),
        Ok(update_of(&case("leverage-cross-10x")))
    );
    let three = UpdateLeverage::new(MarketId::of::<0>(), 3, MarginMode::Isolated);
    assert_eq!(three, Ok(update_of(&case("leverage-isolated-3x"))));
    assert_eq!(three.map(|update| update.leverage()), Ok(3));
    assert_eq!(leverage_fraction(1), Some(10_000));
    assert_eq!(leverage_fraction(10_000), Some(1));
    assert_eq!(leverage_fraction(10_001), None);
    assert_eq!(UpdateLeverage::new(MarketId::of::<1>(), 0, MarginMode::Cross), Err(TradeError::NoLeverage));
    assert_eq!(
        UpdateLeverage::fraction(MarketId::of::<1>(), 500, MarginMode::Cross).map(|update| update.leverage()),
        Ok(20)
    );
    assert!(matches!(UpdateLeverage::fraction(MarketId::of::<1>(), 0, MarginMode::Cross), Err(TradeError::Bad(_))));
}

#[test]
fn a_market_caps_the_leverage_and_an_isolated_only_one_refuses_cross() {
    let btc = MarginLimits { min_initial_margin_fraction: 200, isolated_only: false };
    assert_eq!(btc.max_leverage(), 50);
    let fifty = UpdateLeverage::new(MarketId::of::<1>(), 50, MarginMode::Cross).unwrap();
    assert_eq!(fifty.within(btc), Ok(fifty));
    let more = UpdateLeverage::new(MarketId::of::<1>(), 51, MarginMode::Cross).unwrap();
    assert_eq!(more.within(btc), Err(TradeError::Leverage { asked: 51, most: 50 }));
    let isolated_only = MarginLimits { min_initial_margin_fraction: 1_000, isolated_only: true };
    let cross = UpdateLeverage::new(MarketId::of::<9>(), 5, MarginMode::Cross).unwrap();
    assert_eq!(cross.within(isolated_only), Err(TradeError::IsolatedOnly), "refused, never switched");
    let isolated = UpdateLeverage { mode: MarginMode::Isolated, ..cross };
    assert_eq!(isolated.within(isolated_only), Ok(isolated));
    assert_eq!(MarginLimits { min_initial_margin_fraction: 3_333, isolated_only: false }.max_leverage(), 3);
}
