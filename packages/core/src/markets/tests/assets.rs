//! Assets, tokens, a synthetic spot and the oracles on both networks, recorded on 2026-10-25.

use super::*;
use crate::client::Network;
use crate::client::http::codes;
use crate::test_support::{Recorded, block_on};

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn every_asset_reads_with_both_its_decimals_on_both_networks() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/assetDetails.json"));
    let lighter = block_on(asset_details(&transport)).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.fields.len(), call.auth), ("/api/v1/assetDetails".to_owned(), 0, false));
    assert_eq!(lighter.len(), 13);
    let eth = &lighter[0];
    assert_eq!((eth.asset_id, eth.symbol.as_str(), eth.decimals, eth.l1_decimals), (AssetId::ETH, "ETH", 8, 18));
    assert_eq!((eth.margin_mode, eth.is_margin(), eth.l1_address.is_zero()), (AssetMarginMode::Enabled, true, true));
    assert_eq!(
        (eth.loan_to_value, eth.liquidation_threshold, eth.user_supply_cap),
        (dec("0.8"), dec("0.9"), dec("2000"))
    );
    assert_eq!((eth.index_price, eth.min_withdrawal_amount, eth.is_plain()), (dec("2339.635813"), dec("0.001"), true));
    let usdc = lighter.iter().find(|asset| asset.asset_id == AssetId::COLLATERAL).unwrap();
    assert_eq!(
        (usdc.symbol.as_str(), usdc.l1_address, usdc.decimals),
        ("USDC", Network::Lighter.collateral().token, 6)
    );
    let qqq = lighter.iter().find(|asset| asset.symbol == "rhQQQ").unwrap();
    assert_eq!((qqq.margin_mode, qqq.is_plain()), (AssetMarginMode::PricedOnly, false), "a fund that accrues");

    let robinhood =
        block_on(asset_details(&Recorded::answering(include_str!("../../../fixtures/robinhood/assetDetails.json"))));
    let robinhood = robinhood.unwrap();
    assert_eq!(robinhood.len(), 28);
    let usdg = robinhood.iter().find(|asset| asset.asset_id == AssetId::COLLATERAL).unwrap();
    assert_eq!(
        (usdg.symbol.as_str(), usdg.l1_address, usdg.loan_to_value),
        ("USDG", Network::Robinhood.collateral().token, Dec::ONE)
    );
    let spy = robinhood.iter().find(|asset| asset.symbol == "SPY").unwrap();
    assert_eq!(
        (spy.asset_id.get(), spy.decimals, spy.is_margin(), spy.multiplier),
        (20, 6, true, dec("1.001717991187472003"))
    );
    assert_eq!(robinhood.iter().filter(|asset| asset.is_margin()).count(), 12, "USDG, ETH and ten stocks and funds");
}

#[test]
fn one_asset_is_asked_by_its_id() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/assetDetails.json"));
    let lit = block_on(asset_detail(&transport, AssetId::LIT)).unwrap().unwrap();
    assert_eq!((lit.symbol.as_str(), lit.margin_mode), ("LIT", AssetMarginMode::Disabled));
    assert_eq!(transport.only().fields, [("asset_id", "2".to_owned())]);
    assert_eq!(block_on(asset_detail(&transport, AssetId::of::<60>())).unwrap(), None);
}

#[test]
fn the_token_list_reads_on_both_networks() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/tokenlist.json"));
    let lighter = block_on(token_list(&transport)).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/tokenlist");
    assert_eq!(lighter.len(), 264);
    let first = &lighter[0];
    assert_eq!((first.symbol.as_str(), first.market, first.asset_type), ("0G", TokenMarket::Perps, AssetClass::Crypto));
    assert_eq!(
        (first.gecko_id.as_str(), first.categories.as_slice()),
        ("zero-gravity", ["LAYER_1".to_owned()].as_slice())
    );
    let pepe = lighter.iter().find(|token| token.symbol == "kPEPE").unwrap();
    assert_eq!(pepe.backend_symbol.as_deref(), Some("1000PEPE"), "the market's own symbol");
    assert_eq!(lighter.iter().filter(|token| token.market == TokenMarket::Spot).count(), 18);
    assert_eq!(lighter.iter().filter(|token| token.asset_type == AssetClass::Rwa).count(), 115);

    let robinhood =
        block_on(token_list(&Recorded::answering(include_str!("../../../fixtures/robinhood/tokenlist.json"))));
    let robinhood = robinhood.unwrap();
    assert_eq!(robinhood.len(), 92);
    assert_eq!(robinhood.iter().filter(|token| token.asset_type == AssetClass::Rwa).count(), 69, "mostly stocks");
    assert!(robinhood.iter().all(|token| token.backend_symbol.is_none()));
}

#[test]
fn a_perp_priced_off_futures_has_a_synthetic_spot_and_another_is_refused() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/syntheticSpotInfo_US500.json"));
    let us500 = block_on(synthetic_spot_info(&transport, "US500")).unwrap();
    assert_eq!(transport.only().fields, [("symbol", "US500".to_owned())]);
    assert_eq!((us500.symbol.as_str(), us500.source.as_str()), ("EMU6", "pythlazer"));
    assert_eq!(
        (us500.bps_per_day.as_str(), us500.expiry_time_ms, us500.spot_close_ms),
        ("1.9237607992149872", 1_789_761_600_000, 1_789_588_800_000)
    );
    let transport = Recorded::answering(include_str!("../../../fixtures/robinhood/syntheticSpotInfo_SPY.json"));
    let refused = block_on(synthetic_spot_info(&transport, "SPY")).unwrap_err();
    assert_eq!(refused.code(), Some(codes::INVALID_PARAM));
}

#[test]
fn every_market_s_oracles_read_on_both_networks() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/priceOracleInfo.json"));
    let lighter = block_on(price_oracle_info(&transport, Some("BTC"))).unwrap();
    assert_eq!(transport.only().fields, [("symbol", "BTC".to_owned())]);
    let btc = &lighter.markets[0];
    assert_eq!((lighter.computed_at_ms, btc.symbol.as_str(), btc.pre_market), (1_792_918_090_803, "BTC", false));
    assert_eq!(
        (btc.index.source.as_str(), btc.index.using_median, btc.index.median_sources.len()),
        ("median", true, 5)
    );
    assert_eq!(
        (btc.mark.median_sources[0].as_str(), btc.mark.price_cap_bps.to_dec()),
        ("chainlink / usdt", Some(Dec::ZERO))
    );

    let robinhood = block_on(price_oracle_info(
        &Recorded::answering(include_str!("../../../fixtures/robinhood/priceOracleInfo.json")),
        None,
    ));
    let robinhood = robinhood.unwrap();
    assert_eq!((robinhood.markets[0].pre_market, robinhood.markets[0].mark.median_sources.len()), (false, 3));
    assert!(price_oracle_info_call(None).fields.is_empty());
}
