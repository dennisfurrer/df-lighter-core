//! Liquidations need a token, so the page here is shaped from the docs' `LiquidationInfos` schema,
//! not recorded.

use super::*;
use crate::test_support::{Recorded, block_on};

const PAGE: &str = r#"{"code":200,"liquidations":[{"id":51,"market_id":1,"type":"partial",
    "trade":{"price":"74210.5","size":"0.10000","taker_fee":"0","maker_fee":"0","transaction_time":1790000000123456,
    "bid_account_id":281474976710654,"ask_account_id":100001},
    "info":{"positions":[{"market_id":1,"symbol":"BTC","initial_margin_fraction":"5.00","sign":1,"position":"0.40000",
        "avg_entry_price":"80000.0","position_value":"29684.200000","unrealized_pnl":"-2315.800000","realized_pnl":"0",
        "liquidation_price":"74300.1","margin_mode":0,"allocated_margin":"0.000000"}],
      "risk_info_before":{"cross_risk_parameters":{"market_id":255,"collateral":"3000.000000",
        "total_account_value":"684.200000","initial_margin_req":"1484.210000","maintenance_margin_req":"890.526000",
        "close_out_margin_req":"593.684000","total_account_liquidation_threshold":"890.526000",
        "usdc_collateral_with_funding":"3000.000000","usdc_portfolio_value":"684.200000","total_portfolio_value":"684.200000"},
        "isolated_risk_parameters":[]},
      "risk_info_after":{"cross_risk_parameters":null,"isolated_risk_parameters":null},
      "mark_prices":{"1":74210.53},"assets":[],"asset_index_prices":{"3":"1.000000"}},
    "executed_at":1790000000123},{"id":52,"market_id":2,"type":"adl-v2",
    "trade":{"price":"1","size":"1","taker_fee":"","maker_fee":"","transaction_time":0,
    "bid_account_id":1,"ask_account_id":2},"info":null,"executed_at":0}],"next_cursor":"l2"}"#;

#[test]
fn a_liquidation_reads_with_the_account_as_it_stood() {
    let transport = Recorded::answering(PAGE);
    let account = AccountIndex::new(100_001).unwrap();
    let page = block_on(liquidations(&transport, account, None, None, 50)).unwrap();
    let first = &page.liquidations[0];
    assert_eq!((first.id, first.kind, first.market_id.get()), (51, LiquidationKind::Partial, 1));
    assert_eq!((first.trade.price.to_string(), first.trade.ask_account_id), ("74210.5".to_owned(), account));
    let state = first.info.as_ref().unwrap();
    assert_eq!(state.positions[0].liquidation_price.to_string(), "74300.1");
    let before = state.risk_info_before.as_ref().unwrap().cross_risk_parameters.as_ref().unwrap();
    assert_eq!(
        (before.market_id, before.maintenance_margin_req.to_string()),
        (None, "890.526".to_owned()),
        "255 is no market"
    );
    assert_eq!(state.mark_prices.get("1").map(Figure::as_str), Some("74210.53"));
    assert_eq!(state.risk_info_after.as_ref().unwrap().isolated_risk_parameters.len(), 0);
    let second = &page.liquidations[1];
    assert_eq!((second.kind, second.trade.taker_fee, second.info.as_ref()), (LiquidationKind::Other, None, None));
    assert_eq!(page.next_cursor.as_deref(), Some("l2"));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("liquidations", true));
    assert_eq!(call.fields, [("account_index", "100001".to_owned()), ("limit", "50".to_owned())]);
}
