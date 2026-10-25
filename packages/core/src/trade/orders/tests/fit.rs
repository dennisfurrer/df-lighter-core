//! Putting decimals on a grid: exactly or not at all, in the trader's favour, or as asked; and a
//! taker's worst price, never looser than asked.

use super::*;
use crate::trade::support::d;
use crate::utils::BadUnits;

const BTC: Grid = Grid::new(5, 1);

#[test]
fn exact_takes_what_is_on_the_grid_and_refuses_the_rest() {
    assert_eq!(Fit::Exact.size(BTC, d("0.01")), Ok(1_000));
    assert_eq!(Fit::Exact.price(BTC, d("65000.5"), Side::Bid), Ok(650_005));
    assert_eq!(Fit::Exact.trigger(BTC, d("65000.50")), Ok(650_005), "trailing zeros are no finer");
    let fine = Fit::Exact.size(BTC, d("0.000001"));
    assert_eq!(
        fine,
        Err(TradeError::Units { field: "BaseAmount", error: BadUnits::TooFine { value: d("0.000001"), places: 5 } })
    );
    assert!(matches!(Fit::Exact.price(BTC, d("0"), Side::Bid), Err(TradeError::Units { field: "Price", .. })));
    assert!(matches!(
        Fit::Exact.price(BTC, d("429496729.6"), Side::Bid),
        Err(TradeError::Units { field: "Price", .. })
    ));
    assert!(
        matches!(Fit::Exact.size(BTC, d("2814749767.10656")), Err(TradeError::Units { .. })),
        "past 2^48 - 1 units"
    );
}

#[test]
fn favour_rounds_for_the_trader_and_round_as_asked() {
    assert_eq!(Fit::Favour.size(BTC, d("0.0123456")), Ok(1_234));
    assert_eq!(Fit::Favour.price(BTC, d("100.05"), Side::Bid), Ok(1_000));
    assert_eq!(Fit::Favour.price(BTC, d("100.05"), Side::Ask), Ok(1_001));
    assert_eq!(Fit::Favour.trigger(BTC, d("100.05")), Ok(1_001));
    assert_eq!(Fit::Favour.trigger(BTC, d("100.04")), Ok(1_000));
    assert_eq!(Fit::Round(Round::Up).size(BTC, d("0.0123451")), Ok(1_235));
    assert_eq!(Fit::Round(Round::Down).price(BTC, d("100.09"), Side::Ask), Ok(1_000));
    assert_eq!(Fit::Round(Round::Nearest).price(BTC, d("100.05"), Side::Bid), Ok(1_001));
}

#[test]
fn a_quote_amount_is_sized_down_at_its_price() {
    assert_eq!(Fit::Exact.size_for(BTC, d("100"), d("65000")), Ok(153));
    assert_eq!(Fit::Round(Round::Up).size_for(BTC, d("100"), d("65000")), Ok(154));
    assert_eq!(Fit::Exact.size_for(BTC, d("100"), Dec::ZERO), Err(TradeError::NoPrice("a quote-sized order")));
}

#[test]
fn a_worst_price_moves_against_the_taker_then_back_toward_the_reference() {
    assert_eq!(worst_price(BTC, Side::Bid, d("65000"), 200), Ok(d("66300")));
    assert_eq!(worst_price(BTC, Side::Ask, d("65000"), 200), Ok(d("63700")));
    assert_eq!(worst_price(BTC, Side::Ask, d("65000.05"), 200), Ok(d("63700.1")), "63,700.049 up");
    assert_eq!(worst_price(BTC, Side::Bid, d("65000.05"), 200), Ok(d("66300")), "66,300.051 down");
    assert_eq!(worst_price(BTC, Side::Bid, d("65000"), 0), Ok(d("65000")));
    assert_eq!(worst_price(BTC, Side::Bid, d("65000"), 1_001), Err(TradeError::Slippage(1_001)));
    assert_eq!(worst_price(BTC, Side::Bid, d("-1"), 200), Err(TradeError::NoPrice("a reference")));
    assert_eq!(worst_price(BTC, Side::Ask, d("0.1"), 1_000), Ok(d("0.1")), "0.09 rounds back up to the reference");
}
