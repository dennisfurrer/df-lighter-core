//! Decimals to the integers a transaction signs, exact or cut, never rounded up.

use super::*;

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn an_exact_amount_is_its_units_and_a_finer_one_is_refused() {
    assert_eq!(exact_units(d("25.5"), COLLATERAL_DECIMALS, u64::MAX), Ok(25_500_000));
    assert_eq!(exact_units(d("1"), LIT_DECIMALS, u64::MAX), Ok(100_000_000));
    assert_eq!(
        exact_units(d("0.0000001"), COLLATERAL_DECIMALS, u64::MAX),
        Err(BadUnits::TooFine { value: d("0.0000001"), places: 6 })
    );
    assert_eq!(cut_units(d("0.0000019"), 6, u64::MAX), Ok(1));
    assert_eq!(cut_units(d("1.9999999"), 6, u64::MAX), Ok(1_999_999), "toward zero, never up");
}

#[test]
fn nothing_and_too_much_are_refused() {
    for value in ["0", "-1", "0.0000001"] {
        assert!(cut_units(d(value), 6, u64::MAX).is_err(), "{value}");
    }
    assert_eq!(exact_units(d("2"), 0, 1), Err(BadUnits::TooLarge { value: d("2"), places: 0, most: 1 }));
    assert_eq!(from_units(25_500_000, 6), Some(d("25.5")));
    assert_eq!(from_units(-5, 2), Some(d("-0.05")));
}
