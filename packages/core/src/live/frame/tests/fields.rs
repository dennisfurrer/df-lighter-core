//! Fields the venue writes more than one way read the same.

use serde::Deserialize;

use super::*;
use crate::utils::MarketId;

#[derive(Deserialize)]
struct Blank {
    #[serde(default, deserialize_with = "blank")]
    price: Option<Dec>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Row {
    market_id: MarketId,
}

#[derive(Deserialize)]
struct Rows {
    #[serde(default, deserialize_with = "rows")]
    rows: Vec<Row>,
}

#[derive(Deserialize)]
struct Balances {
    #[serde(deserialize_with = "assets")]
    assets: Vec<Asset>,
}

fn price(text: &str) -> Option<Dec> {
    serde_json::from_str::<Blank>(text).unwrap().price
}

fn markets(text: &str) -> Vec<u64> {
    serde_json::from_str::<Rows>(text).unwrap().rows.into_iter().map(|row| row.market_id.get()).collect()
}

#[test]
fn a_blank_decimal_is_none() {
    assert_eq!(price(r#"{"price":"2338.44"}"#), Dec::parse("2338.44").ok());
    assert_eq!(price(r#"{"price":7}"#), Some(Dec::from_int(7)));
    for none in [r#"{"price":""}"#, r#"{"price":null}"#, "{}"] {
        assert_eq!(price(none), None, "{none}");
    }
    assert!(serde_json::from_str::<Blank>(r#"{"price":1.5}"#).is_err(), "a float is refused");
}

/// A list, a map by market of rows or of lists, one row, or nothing: rows in the order written.
#[test]
fn rows_read_however_they_are_nested() {
    assert_eq!(markets(r#"{"rows":[{"market_id":1},{"market_id":0}]}"#), [1, 0]);
    assert_eq!(markets(r#"{"rows":{"10":{"market_id":10},"2":{"market_id":2}}}"#), [10, 2], "as written");
    assert_eq!(markets(r#"{"rows":{"1":[{"market_id":1},{"market_id":1}],"0":[]}}"#), [1, 1]);
    assert_eq!(markets(r#"{"rows":{"market_id":5}}"#), [5], "one row");
    for none in [r#"{"rows":null}"#, r#"{"rows":[]}"#, r#"{"rows":{}}"#, "{}"] {
        assert!(markets(none).is_empty(), "{none}");
    }
    assert!(serde_json::from_str::<Rows>(r#"{"rows":[{"market_id":"x"}]}"#).is_err());
}

/// The docs' `account_all_assets` example leaves the margin fields out: they read as zero and
/// empty, as the venue omits them.
#[test]
fn a_balance_missing_its_margin_fields_reads() {
    let balances: Balances = serde_json::from_str(
        r#"{"assets":{"1":{"symbol":"ETH","asset_id":1,"balance":"7.1072","locked_balance":"0.0000"}}}"#,
    )
    .unwrap();
    let eth = &balances.assets[0];
    assert_eq!(
        (eth.asset_id, eth.balance, eth.margin_balance, eth.multiplier),
        (AssetId::ETH, Dec::parse("7.1072").unwrap(), Dec::ZERO, Dec::ZERO)
    );
    assert_eq!(eth.margin_mode, "");
}
