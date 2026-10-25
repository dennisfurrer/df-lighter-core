//! The staking pool, an account's stake and its pending unlocks, from the venue's own answers and
//! one made to the docs' shape (no recorded account had an unlock pending).

use super::*;
use crate::test_support::{Recorded, block_on};

const STAKE: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_stake.json");
const RH_STAKE: &str = include_str!("../../../../fixtures/robinhood/publicPoolsMetadata_stake.json");
const ACCOUNT_10: &str = include_str!("../../../../fixtures/lighter/account_10.json");

fn index(value: u64) -> AccountIndex {
    AccountIndex::new(value).unwrap()
}

#[test]
fn lighter_s_staking_pool_reads_with_its_lit() {
    let transport = Recorded::answering(STAKE);
    let pool = block_on(staking_pool(&transport, Network::Lighter)).unwrap().unwrap();
    assert_eq!(pool.holding(AssetId::LIT).map(|lit| lit.balance), Some(Dec::parse("111639831.81372573").unwrap()));
    assert_eq!(transport.only().value("filter"), Some("stake"));
}

#[test]
fn robinhood_chain_has_no_staking_pool_and_is_not_asked() {
    let transport = Recorded::answering(STAKE);
    assert_eq!(block_on(staking_pool(&transport, Network::Robinhood)), Ok(None));
    assert!(transport.sent().is_empty());
    assert_eq!(block_on(staked_shares(&transport, Network::Robinhood, index(10))), Ok(0));
    assert!(transport.sent().is_empty());
    let empty: serde_json::Value = serde_json::from_str(RH_STAKE).unwrap();
    assert_eq!(empty["public_pools"].as_array().map(Vec::len), Some(0), "as its own listing shows");
}

#[test]
fn an_account_s_stake_is_its_staking_pool_share() {
    let transport = Recorded::answering(ACCOUNT_10);
    assert_eq!(block_on(staked_shares(&transport, Network::Lighter, index(10))), Ok(965));
    assert_eq!(block_on(pending_unlocks(&transport, index(10))), Ok(Vec::new()));
}

#[test]
fn a_pending_unlock_reads_its_time_asset_and_amount() {
    let answer = r#"{"code":200,"total":1,"accounts":[{"index":10,"account_type":0,
        "l1_address":"0x478686468057D9873E8718083D40d2927b12F91A","total_asset_value":"1.0",
        "shares":[],"pending_unlocks":[{"unlock_timestamp":1790259200000,"asset_index":2,"amount":"103.66838586"}]}]}"#;
    let transport = Recorded::answering(answer);
    let unlocks = block_on(pending_unlocks(&transport, index(10))).unwrap();
    assert_eq!(
        unlocks,
        [PendingUnlock {
            unlock_timestamp: 1_790_259_200_000,
            asset: AssetId::LIT,
            amount: Dec::parse("103.66838586").unwrap()
        }]
    );
    let call = transport.only();
    assert_eq!((call.value("by"), call.value("value"), call.auth), (Some("index"), Some("10"), false));
}
