//! The venue's bridge reads, from each venue's own answers.

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn each_venue_lists_the_chains_its_intent_addresses_take() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/deposit_networks.json"));
    let networks = block_on(deposit_networks(&transport)).unwrap();
    let ids: Vec<u64> = networks.iter().map(|network| network.chain_id).collect();
    assert_eq!(ids, [43_114, 5_042, 8_453, 999, 42_161]);
    assert_eq!((networks[2].name.as_str(), networks[2].explorer.as_str()), ("Base", "https://basescan.org/"));
    assert_eq!(transport.only().url_path(), "/api/v1/deposit/networks");
    let transport = Recorded::answering(include_str!("../../../fixtures/robinhood/deposit_networks.json"));
    let networks = block_on(deposit_networks(&transport)).unwrap();
    assert_eq!((networks.len(), networks[0].chain_id, networks[0].explorer.as_str()), (1, 4_663, ""));
}

#[test]
fn the_latest_deposit_reads_as_the_bridge_tracked_it() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/deposit_latest.json"));
    let owner: Address = "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap();
    let deposit = block_on(latest_deposit(&transport, owner)).unwrap().unwrap();
    assert!(deposit.is_completed());
    assert_eq!((deposit.source.as_str(), deposit.source_chain_id.as_str()), ("Solana", "101"));
    assert_eq!(
        (deposit.amount, deposit.created_at, deposit.updated_at),
        (Dec::from_int(5), 1_763_508_589, 1_763_509_137)
    );
    assert_eq!((deposit.step.as_str(), deposit.is_next_bridge_fast, deposit.is_external_deposit), ("9", true, false));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("deposit/latest", false));
    assert_eq!(call.fields, [("l1_address", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned())]);
    let none = Recorded::answering(include_str!("../../../fixtures/lighter/deposit_latest_none.json"));
    assert_eq!(block_on(latest_deposit(&none, Address::ZERO)), Ok(None), "an address that never deposited");
}

#[test]
fn an_intent_address_is_asked_for_with_a_form_post() {
    let transport =
        Recorded::answering(r#"{"code":200,"intent_address":"0x1111111111111111111111111111111111111111"}"#);
    let owner: Address = "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap();
    let made = block_on(intent_address(&transport, 42_161, owner, Dec::ZERO, true)).unwrap();
    assert_eq!(made.intent_address, "0x1111111111111111111111111111111111111111");
    let call = transport.only();
    assert_eq!((call.method, call.path, call.auth), (crate::client::Method::Post, "createIntentAddress", false));
    assert_eq!(
        call.fields,
        [
            ("chain_id", "42161".to_owned()),
            ("from_addr", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned()),
            ("amount", "0".to_owned()),
            ("is_external_deposit", "true".to_owned()),
        ]
    );
}

#[test]
fn the_fast_bridge_limit_is_each_venue_s_own() {
    for (text, limit) in [
        (include_str!("../../../fixtures/lighter/fastbridge_info.json"), Dec::ZERO),
        (include_str!("../../../fixtures/robinhood/fastbridge_info.json"), Dec::from_int(748_931)),
    ] {
        let transport = Recorded::answering(text);
        assert_eq!(block_on(fast_bridge_limit(&transport)), Ok(limit));
        assert_eq!(transport.only().url_path(), "/api/v1/fastbridge/info");
    }
}
