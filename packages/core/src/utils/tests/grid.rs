//! A market's grid: sizes cut toward zero, prices rounded the trader's way, integers exact.

use super::*;

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

/// BTC on Lighter: five size decimals, one price decimal.
const BTC: Grid = Grid::new(5, 1);

#[test]
fn sizes_cut_and_prices_round_as_asked() {
    assert_eq!(BTC.size(d("0.123456789")), Some(d("0.12345")));
    assert_eq!(BTC.size(d("0.000009")), None, "nothing left");
    assert_eq!(BTC.price(d("65000.05"), Round::Up), Some(d("65000.1")));
    assert_eq!(BTC.price(d("65000.05"), Round::Down), Some(d("65000")));
    assert_eq!((BTC.size_step(), BTC.tick()), (d("0.00001"), d("0.1")));
}

#[test]
fn a_figure_on_the_grid_is_its_integer_and_one_off_it_is_refused() {
    assert_eq!(BTC.base_amount(d("0.01")), Ok(1000));
    assert_eq!(BTC.price_units(d("83453")), Ok(834_530));
    assert!(BTC.base_amount(d("0.000001")).is_err());
    assert!(BTC.price_units(d("83453.05")).is_err());
    assert!(Grid::new(0, 0).price_units(d("4294967296")).is_err(), "a price is a uint32");
    assert!(Grid::new(0, 0).base_amount(d("281474976710656")).is_err(), "a size is below 2^48");
    assert_eq!((BTC.size_of(1000), BTC.price_of(834_530)), (d("0.01"), d("83453")));
}
