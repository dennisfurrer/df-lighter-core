//! Moving money, read only: what each venue says before a withdrawal or a deposit (the secure
//! withdrawal delay, the fast bridge's limit, the chains intent addresses take, an owner's latest
//! bridged deposit), and what each L1's public node says about the venue's contract. Only
//! `eth_chainId` and `eth_call` go to a node; nothing is ever sent.

use df_lighter_core::Network::{self, Lighter};
use df_lighter_core::client::evm::rpc::BlockTag::Latest;
use df_lighter_core::client::evm::rpc::{CallRequest, Rpc};
use df_lighter_core::client::evm::zklighter::{
    desert_mode, last_account_index, read_account_index, read_asset, read_desert_mode, read_last_account_index,
    token_to_asset_index,
};
use df_lighter_core::transfer::bridge::{deposit_networks, fast_bridge_limit, latest_deposit};
use df_lighter_core::transfer::deposit::{
    account_index_request, allowance_request, balance_request, collateral_amount,
};
use df_lighter_core::transfer::{DepositError, MINIMUM_DEPOSIT, plan_deposit, withdrawal_delay};
use df_lighter_core::utils::{AccountIndex, AssetId, Dec};

use crate::tap::{Tap, node, owner, reader, recording, same_shape, unused};

/// The last account each contract had opened when recorded.
fn recorded_last(network: Network) -> u64 {
    match network {
        Lighter => 763_842,
        Network::Robinhood => 49_473,
    }
}

/// `data` read from the network's contract.
async fn contract_read(rpc: &Rpc<Tap>, network: Network, data: Vec<u8>) -> Vec<u8> {
    rpc.call(&CallRequest::to(network.contract(), data), Latest).await.unwrap()
}

#[tokio::test]
#[ignore = "live"]
async fn the_withdrawal_delay_and_the_fast_bridge_limit_read_on_both_networks() {
    for network in Network::ALL {
        let client = reader(network);
        assert!(withdrawal_delay(&client).await.unwrap() > 0, "{network}");
        same_shape(&client, &recording(network, "withdrawalDelay.json"));
        let limit = fast_bridge_limit(&client).await.unwrap();
        same_shape(&client, &recording(network, "fastbridge_info.json"));
        assert!(!limit.is_negative(), "{network}: {limit}");
    }
}

/// An address that never bridged has no latest deposit on either venue: the venue's 29404, held
/// against Lighter's recording of it.
#[tokio::test]
#[ignore = "live"]
async fn the_chains_intent_addresses_take_and_an_owner_s_latest_deposit_read() {
    for network in Network::ALL {
        let client = reader(network);
        let chains = deposit_networks(&client).await.unwrap();
        same_shape(&client, &recording(network, "deposit_networks.json"));
        assert!(!chains.is_empty() && chains.iter().all(|chain| chain.chain_id > 0), "{network}: {chains:?}");
        assert_eq!(latest_deposit(&client, unused()).await, Ok(None), "{network}");
        same_shape(&client, "lighter/deposit_latest_none.json");
    }

    // The bridge answers a deposit while it is fresh; later the owner reads as having none.
    let client = reader(Lighter);
    match latest_deposit(&client, owner()).await.unwrap() {
        Some(latest) => {
            same_shape(&client, "lighter/deposit_latest.json");
            assert!(latest.amount > Dec::ZERO && latest.created_at <= latest.updated_at, "{latest:?}");
        }
        None => same_shape(&client, "lighter/deposit_latest_none.json"),
    }
}

#[tokio::test]
#[ignore = "live"]
async fn each_l1_is_its_chain_and_its_contract_reads_the_venue_s_state() {
    for network in Network::ALL {
        let rpc = node(network);
        assert_eq!(rpc.chain_id().await, Ok(network.l1_chain_id()), "{network}");
        same_shape(&rpc, &recording(network, "eth_chainId.json"));

        let last = read_last_account_index(&contract_read(&rpc, network, last_account_index()).await).unwrap();
        same_shape(&rpc, &recording(network, "eth_call_lastAccountIndex.json"));
        assert!(last.get() >= recorded_last(network), "{network}: {last}");

        let token = token_to_asset_index(network.collateral().token);
        assert_eq!(read_asset(&contract_read(&rpc, network, token).await), Ok(Some(AssetId::COLLATERAL)));
        same_shape(&rpc, &recording(network, "eth_call_tokenToAssetIndex.json"));

        assert_eq!(read_desert_mode(&contract_read(&rpc, network, desert_mode()).await), Ok(false), "{network}");
        same_shape(&rpc, "lighter/eth_call_desertMode.json");
    }
}

/// Every `eth_call` answers in the same envelope, so the reads with no recording of their own are
/// held against `addressToAccountIndex`'s.
#[tokio::test]
#[ignore = "live"]
async fn a_deposit_reads_who_owns_what_and_what_a_wallet_holds() {
    let rpc = node(Lighter);
    let answer = rpc.run(account_index_request(Lighter, owner())).await.unwrap();
    same_shape(&rpc, "lighter/eth_call_addressToAccountIndex.json");
    assert_eq!(read_account_index(&answer), Ok(Some(AccountIndex::of::<10>())));

    for network in Network::ALL {
        let rpc = node(network);
        let answer = rpc.run(account_index_request(network, unused())).await.unwrap();
        same_shape(&rpc, "lighter/eth_call_addressToAccountIndex.json");
        assert_eq!(read_account_index(&answer), Ok(None), "{network}: no deposit yet");

        let held = collateral_amount(network, &rpc.run(balance_request(network, owner())).await.unwrap()).unwrap();
        same_shape(&rpc, "lighter/eth_call_addressToAccountIndex.json");
        let allowed = collateral_amount(network, &rpc.run(allowance_request(network, owner())).await.unwrap()).unwrap();
        same_shape(&rpc, "lighter/eth_call_addressToAccountIndex.json");
        assert!(!held.is_negative() && !allowed.is_negative(), "{network}: {held} held, {allowed} allowed");

        let refused = plan_deposit(&rpc, network, unused(), unused(), MINIMUM_DEPOSIT).await.unwrap_err();
        let symbol = network.collateral().symbol;
        assert_eq!(refused, DepositError::Short { have: Dec::ZERO, need: MINIMUM_DEPOSIT, symbol }, "{network}");
    }
}
