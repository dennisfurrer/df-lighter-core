//! Figures read from bare JSON floats and decimal strings, every digit kept.

use serde::Deserialize;

use super::*;

#[derive(Debug, Deserialize)]
struct Pool {
    annual_percentage_yield: Figure,
    #[serde(default)]
    sharpe_ratio: Option<Figure>,
}

/// Live, the LLP on zkLighter, 2026-10-25: a float an f64 rounds on the way through, read as written.
/// An exponent is applied exactly; past eighteen places it is the nearest at eighteen.
#[test]
fn a_number_keeps_every_digit_it_was_written_with() {
    let pool: Pool =
        serde_json::from_str(r#"{"annual_percentage_yield":30.733724772746875,"sharpe_ratio":0.0}"#).unwrap();
    assert_eq!(pool.annual_percentage_yield.as_str(), "30.733724772746875");
    assert_eq!(pool.annual_percentage_yield.to_string(), "30.733724772746875");
    assert_eq!(pool.sharpe_ratio.and_then(|figure| figure.to_dec()), Some(Dec::ZERO));
    let none: Pool = serde_json::from_str(r#"{"annual_percentage_yield":"0.05","sharpe_ratio":null}"#).unwrap();
    assert_eq!(none.annual_percentage_yield.to_dec(), Dec::new(5, 2), "a decimal string reads too");
    assert_eq!(none.sharpe_ratio, None);

    for (text, value) in [
        ("0.037569521048262375", Dec::new(37_569_521_048_262_375, 18)),
        ("1.376149999999998", Dec::new(1_376_149_999_999_998, 15)),
        ("1e-7", Dec::new(1, 7)),
        ("2.5E3", Some(Dec::from_int(2500))),
        ("-4.2e-1", Dec::new(-42, 2)),
        ("1e-30", Some(Dec::ZERO)),
        ("41085245.621082", Dec::new(41_085_245_621_082, 6)),
    ] {
        assert_eq!(serde_json::from_str::<Figure>(text).unwrap().to_dec(), value, "{text}");
    }
}

#[test]
fn anything_but_a_number_is_refused() {
    for bad in ["true", "\"abc\"", "{}", "[1]", "null", "\"\""] {
        assert!(serde_json::from_str::<Figure>(bad).is_err(), "{bad}");
    }
}

/// Past eighteen places a figure is the nearest at eighteen, an exponent moving the point first.
#[test]
fn past_eighteen_places_a_figure_is_the_nearest_at_eighteen() {
    let dec = |text: &str| serde_json::from_str::<Figure>(text).unwrap().to_dec().map(|value| value.to_string());
    assert_eq!(dec("0.00009599999999999999").as_deref(), Some("0.000096"));
    assert_eq!(dec("-0.00009599999999999999").as_deref(), Some("-0.000096"));
    assert_eq!(dec("0.1234567890123456784").as_deref(), Some("0.123456789012345678"));
    assert_eq!(dec("1.5e-18").as_deref(), Some("0.000000000000000002"));
    assert_eq!(dec("1.4e-18").as_deref(), Some("0.000000000000000001"));
    assert_eq!(dec("4e-19").as_deref(), Some("0"));
    assert_eq!(dec("2.5e3").as_deref(), Some("2500"));
    assert_eq!(dec("\"1e2\"").as_deref(), Some("100"));
}
