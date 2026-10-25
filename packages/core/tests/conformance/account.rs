//! How an account is set up, built from the Go fields lighter-go was handed: a sub-account
//! opened, unified trading mode, an asset counted as margin, and an integrator approved, revoked
//! and approved on Robinhood Chain, each with the text the owner signs and the `tx_info` sent with
//! that signature.

use df_lighter_core::account::{
    ApproveIntegrator, CreateSubAccount, TradingMode, UpdateAccountAssetConfig, UpdateAccountConfig,
};

use crate::vector::every;

const CREATE_SUB_ACCOUNT: [&str; 1] = ["create-sub-account"];
const UPDATE_ACCOUNT_CONFIG: [&str; 1] = ["unified-trading-mode"];
const UPDATE_ACCOUNT_ASSET_CONFIG: [&str; 1] = ["asset-as-margin"];
const APPROVE_INTEGRATOR: [&str; 3] = ["approve-integrator", "revoke-integrator", "robinhood-approve-integrator"];

/// The transaction cases replayed here.
pub const CASES: [&[&str]; 4] =
    [&CREATE_SUB_ACCOUNT, &UPDATE_ACCOUNT_CONFIG, &UPDATE_ACCOUNT_ASSET_CONFIG, &APPROVE_INTEGRATOR];

#[test]
fn a_sub_account_is_opened_as_lighter_go_signs_it() {
    every("CreateSubAccount", &CREATE_SUB_ACCOUNT, &[], |case| {
        assert!(case.fields().is_empty(), "it carries nothing of its own");
        Ok(CreateSubAccount)
    });
}

#[test]
fn unified_trading_mode_is_set_as_lighter_go_signs_it() {
    every("UpdateAccountConfig", &UPDATE_ACCOUNT_CONFIG, &[], |case| {
        let mode = TradingMode::from_code(case.fields().int("AccountTradingMode")).unwrap();
        Ok(UpdateAccountConfig::new(mode))
    });
}

#[test]
fn an_asset_counts_as_margin_as_lighter_go_signs_it() {
    every("UpdateAccountAssetConfig", &UPDATE_ACCOUNT_ASSET_CONFIG, &[], |case| {
        let fields = case.fields();
        Ok(UpdateAccountAssetConfig { asset: fields.asset("AssetIndex")?, margin: fields.flag("AssetMarginMode") })
    });
}

#[test]
fn every_integrator_approval_and_revocation_signs_as_lighter_go_signs_it_with_the_owner_s_signature() {
    every("ApproveIntegrator", &APPROVE_INTEGRATOR, &[], |case| {
        let fields = case.fields();
        Ok(ApproveIntegrator {
            integrator: fields.account("IntegratorAccountIndex")?,
            max_perps_taker_fee: fields.narrow("MaxPerpsTakerFee"),
            max_perps_maker_fee: fields.narrow("MaxPerpsMakerFee"),
            max_spot_taker_fee: fields.narrow("MaxSpotTakerFee"),
            max_spot_maker_fee: fields.narrow("MaxSpotMakerFee"),
            expiry: fields.int("ApprovalExpiry"),
        })
    });
}
