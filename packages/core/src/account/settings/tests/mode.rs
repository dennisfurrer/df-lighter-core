//! The trading mode and margin assets, held to lighter-go (`vectors/lighter_go.json`), and the
//! mode read leniently from an account.

use super::*;

#[cfg(feature = "sign")]
fn replays(tx: &impl Tx, name: &str) {
    use crate::test_support::{replay, vector, vector_envelope, vector_key};
    use crate::utils::sign::tx::sign_drawing;

    let case = vector(name);
    let (chain_id, envelope) = vector_envelope(&case);
    let signed = sign_drawing(tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
    let out = &case["out"];
    assert_eq!(u64::from(signed.tx_type().code()), out["tx_type"].as_u64().unwrap(), "{name}: type");
    assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
    assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
    assert_eq!(signed.l1_message(), None, "{name}: the API key alone signs it");
}

#[cfg(feature = "sign")]
#[test]
fn unified_mode_is_signed_as_lighter_go_signs_it() {
    let case = crate::test_support::vector("unified-trading-mode");
    let mode = TradingMode::from_code(case["fields"]["AccountTradingMode"].as_u64().unwrap()).unwrap();
    assert_eq!(mode, TradingMode::Unified);
    replays(&UpdateAccountConfig::new(mode), "unified-trading-mode");
}

#[cfg(feature = "sign")]
#[test]
fn an_asset_as_margin_is_signed_as_lighter_go_signs_it() {
    let case = crate::test_support::vector("asset-as-margin");
    let asset = AssetId::new(u16::try_from(case["fields"]["AssetIndex"].as_u64().unwrap()).unwrap()).unwrap();
    assert_eq!(case["fields"]["AssetMarginMode"], 1);
    replays(&UpdateAccountAssetConfig::enable(asset), "asset-as-margin");
}

#[test]
fn modes_and_margin_flags_have_lighter_go_s_numbers() {
    assert_eq!((TradingMode::Classic.code(), TradingMode::Unified.code()), (0, 1));
    assert_eq!((TradingMode::from_code(1), TradingMode::from_code(2)), (Some(TradingMode::Unified), None));
    assert_eq!(TradingMode::default(), TradingMode::Classic);
    assert_eq!(UpdateAccountAssetConfig::enable(AssetId::ETH).margin_code(), 1);
    assert_eq!(UpdateAccountAssetConfig::disable(AssetId::ETH).margin_code(), 0);
    let info = UpdateAccountAssetConfig::disable(AssetId::LIT).info_fields();
    assert_eq!(info, [("AssetIndex", Json::Number("2".into())), ("AssetMarginMode", Json::Number("0".into()))]);
}

#[test]
fn a_mode_this_build_does_not_know_reads_as_none() {
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(default, deserialize_with = "known_mode")]
        mode: Option<TradingMode>,
    }
    let read = |text: &str| serde_json::from_str::<Row>(text).unwrap().mode;
    assert_eq!(read(r#"{"mode":1}"#), Some(TradingMode::Unified));
    assert_eq!(read(r#"{"mode":7}"#), None);
    assert_eq!(read("{}"), None);
}
