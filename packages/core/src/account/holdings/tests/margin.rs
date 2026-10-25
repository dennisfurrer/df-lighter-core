//! Leverage, margin and prices from the positions both venues recorded, and the fraction's edges.

use super::*;
use crate::account::holdings::account::AccountPage;

fn account(text: &str) -> Account {
    let page: AccountPage = serde_json::from_str(text).unwrap();
    page.accounts.into_iter().next().unwrap()
}

fn market(id: u64) -> MarketId {
    MarketId::new(id).unwrap()
}

#[test]
fn a_percent_fraction_is_basis_points_and_floors_back_to_the_leverage_set() {
    for (percent, bps, leverage) in [
        ("2.63", 263, 38),
        ("4.34", 434, 23),
        ("33.33", 3333, 3),
        ("8.33", 833, 12),
        ("5.00", 500, 20),
        ("100", 10_000, 1),
    ] {
        let bps_read = imf_bps(Dec::parse(percent).unwrap());
        assert_eq!((bps_read, bps_read.and_then(leverage_of)), (Some(bps), Some(leverage)), "{percent}");
    }
    assert_eq!(imf_bps(Dec::ZERO), None);
    assert_eq!(imf_bps(Dec::parse("-5").unwrap()), None);
    assert_eq!(imf_bps(Dec::parse("100.01").unwrap()), None, "more than the whole");
    assert_eq!(imf_bps(Dec::parse("0.004").unwrap()), None, "rounds to nothing");
    assert_eq!(leverage_of(0), None);
}

#[test]
fn a_flat_row_still_carries_the_leverage_and_margin_mode_set() {
    let account = account(include_str!("../../../../fixtures/lighter/account_10.json"));
    let eth = account.setting(market(0)).unwrap();
    assert_eq!((eth.leverage, eth.imf_bps, eth.margin_mode), (38, 263, MarginMode::Cross));
    let btc = account.setting(market(1)).unwrap();
    assert_eq!((btc.leverage, btc.margin_mode), (23, MarginMode::Isolated));
    assert_eq!(account.setting(market(9_999)), None, "a market never set has no row");
    let flat = account.position(market(1)).unwrap();
    assert_eq!((flat.entry_price(), flat.liquidation()), (None, None));
    assert_eq!(flat.margin(), Some(Dec::ZERO), "isolated: what is allocated, nothing");
}

#[test]
fn a_cross_position_s_margin_is_its_notional_times_the_fraction_to_six_places() {
    let account = account(include_str!("../../../../fixtures/robinhood/account_llp.json"));
    for (id, leverage, margin, entry) in [
        (0, 50, "1953.140245", "2343.84"),
        (1, 50, "3354.02186", "80499.1"),
        (2, 20, "2001.029954", "76.218"),
        (3, 25, "1073.452728", "101.433"),
        (4, 10, "820.942152", "1194.95"),
    ] {
        let position = account.open_position(market(id)).unwrap();
        assert_eq!(position.leverage(), Some(leverage), "{id}");
        assert_eq!(position.margin().map(|margin| margin.to_string()).as_deref(), Some(margin), "{id}");
        assert_eq!(position.entry_price().map(|price| price.to_string()).as_deref(), Some(entry), "{id}");
        assert!(position.notional().is_positive());
    }
}

#[test]
fn an_isolated_position_s_margin_is_what_was_allocated() {
    let text = r#"{"market_id":1,"symbol":"BTC","initial_margin_fraction":"10.00","sign":1,"position":"0.5",
        "avg_entry_price":"80000","position_value":"40100","unrealized_pnl":"100","realized_pnl":"0",
        "liquidation_price":"72500.5","margin_mode":1,"allocated_margin":"4210.123456"}"#;
    let position: Position = serde_json::from_str(text).unwrap();
    assert_eq!(position.margin().map(|margin| margin.to_string()).as_deref(), Some("4210.123456"));
    assert_eq!(position.liquidation().map(|price| price.to_string()).as_deref(), Some("72500.5"));
    let setting = position.setting().unwrap();
    assert_eq!((setting.market, setting.leverage, setting.margin_mode), (market(1), 10, MarginMode::Isolated));
}
