//! Each enum's number is lighter-go's constant and its word the venue's.

use super::*;

#[test]
fn every_order_kind_is_its_number_and_its_word() {
    let codes: Vec<u8> = OrderKind::ALL.iter().map(|kind| kind.code()).collect();
    assert_eq!(codes, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    for kind in OrderKind::ALL {
        assert_eq!(OrderKind::from_code(kind.code()), Some(kind));
        assert_eq!(OrderKind::from_name(kind.name()), Some(kind));
        let read: OrderKind = serde_json::from_str(&format!("\"{}\"", kind.name())).unwrap();
        assert_eq!(read, kind);
    }
    assert_eq!(OrderKind::from_code(9), None);
    assert!(OrderKind::Twap.is_placeable() && !OrderKind::TwapSub.is_placeable());
    assert!(OrderKind::TakeProfitLimit.is_trigger() && !OrderKind::Market.is_trigger());
}

#[test]
fn time_in_force_margin_and_market_kinds_read_as_the_venue_writes_them() {
    assert_eq!(TimeInForce::ALL.map(TimeInForce::code), [0, 1, 2]);
    assert_eq!(TimeInForce::from_name("good-till-time"), Some(TimeInForce::GoodTillTime));
    assert_eq!(serde_json::from_str::<MarginMode>("1").unwrap(), MarginMode::Isolated);
    assert!(serde_json::from_str::<MarginMode>("2").is_err());
    for (word, kind) in [("perps", MarketKind::Perps), ("perp", MarketKind::Perps), ("bo", MarketKind::BinaryOptions)] {
        assert_eq!(MarketKind::from_name(word), Some(kind), "{word}");
    }
    assert_eq!((MarketKind::Perps.filter(), MarketKind::Spot.code()), ("perp", 1));
}

#[test]
fn a_side_is_its_is_ask() {
    assert_eq!((Side::from_is_ask(true), Side::from_is_ask(false)), (Side::Ask, Side::Bid));
    assert_eq!((Side::Bid.opposite(), Side::Ask.sign(), Side::Ask.is_ask()), (Side::Ask, -1, true));
}

#[test]
fn an_unknown_word_reads_as_none_where_it_is_read_leniently() {
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(deserialize_with = "known")]
        tif: Option<TimeInForce>,
    }
    let read = |text: &str| serde_json::from_str::<Row>(text).unwrap().tif;
    assert_eq!(read(r#"{"tif":"post-only"}"#), Some(TimeInForce::PostOnly));
    assert_eq!(read(r#"{"tif":"Unknown"}"#), None);
    assert_eq!(read(r#"{"tif":null}"#), None);
}
