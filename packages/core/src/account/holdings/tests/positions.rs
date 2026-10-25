//! Positions, balances and pool shares, from a recorded account and from one made to the venue's
//! shape.

use serde_json::Value;

use super::*;

fn account() -> Value {
    let page: Value = serde_json::from_str(include_str!("../../../../fixtures/lighter/account_10.json")).unwrap();
    page["accounts"][0].clone()
}

#[test]
fn a_recorded_account_s_holdings_read() {
    let account = account();
    let positions: Vec<Position> = serde_json::from_value(account["positions"].clone()).unwrap();
    assert_eq!(positions.len(), 48);
    assert!(positions.iter().all(|position| !position.is_open() && position.side().is_none()));
    assert_eq!((positions[1].symbol.as_str(), positions[1].margin_mode), ("BTC", MarginMode::Isolated));
    assert_eq!(positions[0].initial_margin_fraction.to_string(), "2.63");

    let assets: Vec<Asset> = serde_json::from_value(account["assets"].clone()).unwrap();
    let usdc = &assets[1];
    assert_eq!((usdc.symbol.as_str(), usdc.asset_id.get()), ("USDC", 3));
    assert_eq!(
        (usdc.balance.to_string(), usdc.margin_balance.to_string()),
        ("1.997570461".to_owned(), "1945.003244157056".to_owned())
    );
    assert!(!usdc.backs_perps());
    assert_eq!(assets[0].free().to_string(), "524.21838586");

    let shares: Vec<PoolShare> = serde_json::from_value(account["shares"].clone()).unwrap();
    assert_eq!((shares[1].pool.get(), shares[1].shares), (281_474_976_710_654, 1), "the LLP");
    assert_eq!(shares[0].principal_amount.to_string(), "0.00965");
}

#[test]
fn an_open_short_is_negative_and_an_ask() {
    let text = r#"{"market_id":1,"symbol":"BTC","initial_margin_fraction":"5.00","open_order_count":2,
        "pending_order_count":0,"position_tied_order_count":1,"sign":-1,"position":"0.25000",
        "avg_entry_price":"80000.0","position_value":"20100.000000","unrealized_pnl":"-100.000000",
        "realized_pnl":"0.000000","liquidation_price":"95000.5","total_funding_paid_out":"1.5",
        "margin_mode":0,"margin_set_flag":0,"allocated_margin":"0.000000"}"#;
    let position: Position = serde_json::from_str(text).unwrap();
    assert!(position.is_open());
    assert_eq!((position.signed_size().to_string(), position.side()), ("-0.25".to_owned(), Some(Side::Ask)));
    assert_eq!((position.margin_mode, position.open_order_count), (MarginMode::Cross, 2));
    assert_eq!(position.total_funding_paid_out.map(|paid| paid.to_string()).as_deref(), Some("1.5"));
}
