//! Earning, built from the Go fields lighter-go was handed: a public pool opened and frozen with
//! new terms, LLP shares minted and burnt, LIT staked and unstaked.

use df_lighter_core::earn::{
    BurnShares, CreatePublicPool, MintShares, PoolStatus, StakeAssets, UnstakeAssets, UpdatePublicPool,
};

use crate::vector::every;

const CREATE_PUBLIC_POOL: [&str; 1] = ["create-public-pool"];
const UPDATE_PUBLIC_POOL: [&str; 1] = ["update-public-pool"];
const MINT_SHARES: [&str; 1] = ["mint-llp-shares"];
const BURN_SHARES: [&str; 1] = ["burn-llp-shares"];
const STAKE_ASSETS: [&str; 1] = ["stake-lit"];
const UNSTAKE_ASSETS: [&str; 1] = ["unstake-lit"];

/// The transaction cases replayed here.
pub const CASES: [&[&str]; 6] =
    [&CREATE_PUBLIC_POOL, &UPDATE_PUBLIC_POOL, &MINT_SHARES, &BURN_SHARES, &STAKE_ASSETS, &UNSTAKE_ASSETS];

#[test]
fn a_pool_is_opened_and_its_terms_updated_as_lighter_go_signs_them() {
    every("CreatePublicPool", &CREATE_PUBLIC_POOL, &[], |case| {
        let fields = case.fields();
        Ok(CreatePublicPool {
            operator_fee: fields.narrow("OperatorFee"),
            initial_total_shares: fields.int("InitialTotalShares"),
            min_operator_share_rate: fields.narrow("MinOperatorShareRate"),
        })
    });
    every("UpdatePublicPool", &UPDATE_PUBLIC_POOL, &[], |case| {
        let fields = case.fields();
        Ok(UpdatePublicPool {
            pool: fields.account("PublicPoolIndex")?,
            status: PoolStatus::from_code(fields.narrow("Status")),
            operator_fee: fields.narrow("OperatorFee"),
            min_operator_share_rate: fields.narrow("MinOperatorShareRate"),
        })
    });
}

#[test]
fn llp_shares_are_minted_and_burnt_as_lighter_go_signs_them() {
    every("MintShares", &MINT_SHARES, &[], |case| {
        let fields = case.fields();
        Ok(MintShares::new(fields.account("PublicPoolIndex")?, fields.int("ShareAmount")))
    });
    every("BurnShares", &BURN_SHARES, &[], |case| {
        let fields = case.fields();
        Ok(BurnShares::new(fields.account("PublicPoolIndex")?, fields.int("ShareAmount")))
    });
}

#[test]
fn lit_is_staked_and_unstaked_as_lighter_go_signs_it() {
    every("StakeAssets", &STAKE_ASSETS, &[], |case| {
        let fields = case.fields();
        Ok(StakeAssets::new(fields.account("StakingPoolIndex")?, fields.int("ShareAmount")))
    });
    every("UnstakeAssets", &UNSTAKE_ASSETS, &[], |case| {
        let fields = case.fields();
        Ok(UnstakeAssets::new(fields.account("StakingPoolIndex")?, fields.int("ShareAmount")))
    });
}
