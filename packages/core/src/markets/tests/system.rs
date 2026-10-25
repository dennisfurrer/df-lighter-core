//! The venue's own settings on both networks, recorded on 2026-10-25, checked against what
//! [`Network`] states.

use super::*;
use crate::client::Network;
use crate::test_support::{Recorded, block_on};

fn fixture(network: Network, name: &str) -> &'static str {
    match (network, name) {
        (Network::Lighter, "systemConfig") => include_str!("../../../fixtures/lighter/systemConfig.json"),
        (Network::Robinhood, "systemConfig") => include_str!("../../../fixtures/robinhood/systemConfig.json"),
        (Network::Lighter, "info") => include_str!("../../../fixtures/lighter/info.json"),
        (Network::Robinhood, "info") => include_str!("../../../fixtures/robinhood/info.json"),
        (Network::Lighter, "layer1BasicInfo") => include_str!("../../../fixtures/lighter/layer1BasicInfo.json"),
        (Network::Robinhood, "layer1BasicInfo") => include_str!("../../../fixtures/robinhood/layer1BasicInfo.json"),
        (Network::Lighter, "withdrawalDelay") => include_str!("../../../fixtures/lighter/withdrawalDelay.json"),
        (Network::Robinhood, "withdrawalDelay") => include_str!("../../../fixtures/robinhood/withdrawalDelay.json"),
        (Network::Lighter, "announcement") => include_str!("../../../fixtures/lighter/announcement.json"),
        (Network::Robinhood, "announcement") => include_str!("../../../fixtures/robinhood/announcement.json"),
        _ => "",
    }
}

#[test]
fn the_system_config_names_the_pools_each_network_runs() {
    let transport = Recorded::answering(fixture(Network::Lighter, "systemConfig"));
    let lighter = block_on(system_config(&transport)).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/systemConfig");
    assert_eq!(
        (lighter.liquidity_pool(), lighter.staking_pool().map(AccountIndex::get)),
        (Some(AccountIndex::LLP), Some(281_474_976_624_800))
    );
    assert_eq!((lighter.liquidity_pool_cooldown_period, lighter.staking_pool_lockup_period), (300_000, 259_200_000));
    assert_eq!((lighter.max_integrator_perps_taker_fee, lighter.max_integrator_spot_taker_fee), (1_000, 10_000));
    assert_eq!(lighter.market_maker_incentive_account_index, 723_106);

    let robinhood = block_on(system_config(&Recorded::answering(fixture(Network::Robinhood, "systemConfig")))).unwrap();
    assert_eq!((robinhood.liquidity_pool(), robinhood.staking_pool()), (None, None), "2^48 - 1: none");
    assert_eq!((robinhood.fee_collector_account_index, robinhood.max_integrator_perps_maker_fee), (18_971, 10_000));
}

#[test]
fn the_contract_and_the_l1_are_the_ones_each_network_states() {
    for network in Network::ALL {
        let transport = Recorded::answering(fixture(network, "info"));
        assert_eq!(block_on(contract_address(&transport)), Ok(network.contract()), "{network}");
        let call = transport.only();
        assert_eq!((call.url_path(), call.fields.len()), ("/info".to_owned(), 0), "from the host's root");

        let transport = Recorded::answering(fixture(network, "layer1BasicInfo"));
        let l1 = block_on(layer1_basic_info(&transport)).unwrap();
        assert_eq!(transport.only().url_path(), "/api/v1/layer1BasicInfo");
        assert_eq!((l1.chain_id(), l1.l1_providers_health), (Some(network.l1_chain_id()), true), "{network}");
        assert_eq!(l1.contract("ZkLighterContract"), Some(network.contract()));
        assert_eq!(l1.contract("USDCContract"), Some(network.collateral().token), "USDG's on Robinhood Chain");
        assert_eq!((l1.validator_info.len(), l1.contract("nothing")), (0, None));
    }
}

#[test]
fn the_withdrawal_delay_is_seconds() {
    let transport = Recorded::answering(fixture(Network::Lighter, "withdrawalDelay"));
    assert_eq!(block_on(withdrawal_delay(&transport)), Ok(1_060));
    assert_eq!(transport.only().url_path(), "/api/v1/withdrawalDelay");
    assert_eq!(
        block_on(withdrawal_delay(&Recorded::answering(fixture(Network::Robinhood, "withdrawalDelay")))),
        Ok(376)
    );
}

#[test]
fn announcements_read_newest_first_on_both_networks() {
    let transport = Recorded::answering(fixture(Network::Lighter, "announcement"));
    let lighter = block_on(announcements(&transport)).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/announcement");
    assert_eq!(
        (lighter.len(), lighter[0].title.as_str(), lighter[0].created_at),
        (213, "New Perp Listing", 1_791_455_694)
    );
    assert!(lighter.windows(2).all(|pair| pair[0].created_at >= pair[1].created_at));
    assert_eq!(
        lighter.iter().filter(|announcement| announcement.expired_at == 0).count(),
        81,
        "left out on older ones"
    );
    let robinhood = block_on(announcements(&Recorded::answering(fixture(Network::Robinhood, "announcement")))).unwrap();
    assert_eq!((robinhood.len(), robinhood[0].expired_at), (14, 1_792_952_280));
    assert!(robinhood[0].content.contains('\n'));
}
