//! The contract's calls held to go-ethereum 1.15.6's ABI packing (`vectors/lighter_go.json`,
//! `evm`), every selector to the hash of its signature, and its answers as Ethereum's and Robinhood
//! Chain's public nodes gave them (`fixtures/*/eth_call_*.json`).

use serde_json::Value;

use super::*;
use crate::client::evm::rpc::{BlockTag, CallRequest, ErrorKind, Revert, RpcError, eth};
use crate::test_support::vectors;
use crate::utils::hex;
use crate::utils::sign::keccak256;

fn case(name: &str) -> Value {
    vectors()["evm"].as_array().unwrap().iter().find(|case| case["name"] == name).unwrap().clone()
}

fn address(value: &Value) -> Address {
    value.as_str().unwrap().parse().unwrap()
}

fn big(value: &Value) -> U256 {
    match value {
        Value::String(text) => text.parse().unwrap(),
        other => U256::from(other.as_u64().unwrap()),
    }
}

fn small(value: &Value) -> u64 {
    big(value).to_u64().unwrap()
}

fn check(name: &str, calldata: &[u8]) {
    let case = case(name);
    assert_eq!(hex::encode_prefixed(calldata), case["calldata"].as_str().unwrap(), "{name}");
    let function = abi::Function::parse(case["function"].as_str().unwrap()).unwrap();
    assert_eq!(function.selector(), calldata[..4], "{name}: selector");
}

fn account(value: &Value) -> AccountIndex {
    AccountIndex::new(small(value)).unwrap()
}

fn asset(value: &Value) -> AssetId {
    AssetId::new(u16::try_from(small(value)).unwrap()).unwrap()
}

fn route(value: &Value) -> Route {
    match small(value) {
        0 => Route::Perps,
        _ => Route::Spot,
    }
}

#[test]
fn every_selector_is_the_hash_of_its_signature() {
    for (signature, selector) in SIGNATURES {
        assert_eq!(keccak256(signature.as_bytes())[..4], selector, "{signature}");
        assert_eq!(abi::selector(signature).unwrap(), selector, "{signature}");
    }
}

#[test]
fn every_call_packs_as_go_ethereum_packs_it() {
    for name in ["deposit-usdc-perps", "deposit-eth-spot"] {
        let args = &case(name)["args"];
        check(name, &deposit(address(&args[0]), asset(&args[1]), route(&args[2]), big(&args[3])));
    }
    let args = &case("withdraw-usdc")["args"];
    check("withdraw-usdc", &withdraw(account(&args[0]), asset(&args[1]), route(&args[2]), small(&args[3])));
    let args = &case("change-pub-key")["args"];
    let key = PublicKey::from_hex(args[2].as_str().unwrap()).unwrap();
    check("change-pub-key", &change_pub_key(account(&args[0]), ApiKeyIndex::new(4).unwrap(), &key));
    check("cancel-all-orders", &cancel_all_orders(account(&case("cancel-all-orders")["args"][0])));
    let args = &case("create-order")["args"];
    let market = MarketId::new(small(&args[1])).unwrap();
    let price = u32::try_from(small(&args[3])).unwrap();
    let order = create_order(account(&args[0]), market, small(&args[2]), price, true, OrderKind::Market).unwrap();
    check("create-order", &order);
    let args = &case("burn-shares")["args"];
    check("burn-shares", &burn_shares(account(&args[0]), account(&args[1]), small(&args[2])));
    let args = &case("withdraw-pending-balance")["args"];
    let amount = big(&args[2]).to_u128().unwrap();
    check("withdraw-pending-balance", &withdraw_pending_balance(address(&args[0]), asset(&args[1]), amount));
    let args = &case("address-to-account-index")["args"];
    check("address-to-account-index", &address_to_account_index(address(&args[0])));
    let args = &case("get-pending-balance")["args"];
    check("get-pending-balance", &get_pending_balance(address(&args[0]), asset(&args[1])));
    let args = &case("token-to-asset-index")["args"];
    check("token-to-asset-index", &token_to_asset_index(address(&args[0])));
    assert_eq!((desert_mode(), last_account_index()), (DESERT_MODE.to_vec(), LAST_ACCOUNT_INDEX.to_vec()));
}

#[test]
fn a_batch_needs_lists_as_long_and_wider_orders_are_refused() {
    let owner: Address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".parse().unwrap();
    let one = AccountIndex::new(100_001).unwrap();
    let batch = deposit_batch(&[25_500_000], &[owner], &[one]).unwrap();
    let decoded = abi::Function::parse("depositBatch(uint64[],address[],uint48[])").unwrap().decode_input(&batch);
    assert_eq!(decoded.unwrap()[1], Token::Array(vec![Token::address(owner)]));
    assert_eq!(deposit_batch(&[1, 2], &[owner], &[one]), Err(AbiError::Count { wants: 2, got: 1 }));
    assert_eq!(deposit_batch(&[1], &[owner], &[]), Err(AbiError::Count { wants: 1, got: 0 }));
    let wide = MarketId::new(2048 * 64).unwrap();
    assert!(create_order(one, wide, 1, 1, false, OrderKind::Market).is_err(), "a market past 16 bits");
    assert!(create_order(one, MarketId::new(1).unwrap(), 1 << 48, 1, false, OrderKind::Market).is_err());
}

/// What the public nodes answered, through the same reads.
fn answer(text: &str) -> Vec<u8> {
    eth::call(&CallRequest::default(), BlockTag::Latest).answer(text).unwrap()
}

#[test]
fn the_contract_s_answers_read_as_the_nodes_gave_them() {
    let ten = answer(include_str!("../../../../fixtures/lighter/eth_call_addressToAccountIndex.json"));
    assert_eq!(read_account_index(&ten), Ok(Some(AccountIndex::new(10).unwrap())), "0x4786...F91A owns account 10");
    assert_eq!(read_account_index(&[0u8; 32]), Ok(None), "no deposit yet");
    let last = answer(include_str!("../../../../fixtures/lighter/eth_call_lastAccountIndex.json"));
    assert_eq!(read_last_account_index(&last), Ok(AccountIndex::new(763_842).unwrap()));
    let last = answer(include_str!("../../../../fixtures/robinhood/eth_call_lastAccountIndex.json"));
    assert_eq!(read_last_account_index(&last), Ok(AccountIndex::new(49_473).unwrap()));
    let desert = answer(include_str!("../../../../fixtures/lighter/eth_call_desertMode.json"));
    assert_eq!(read_desert_mode(&desert), Ok(false));
    for text in [
        include_str!("../../../../fixtures/lighter/eth_call_tokenToAssetIndex.json"),
        include_str!("../../../../fixtures/robinhood/eth_call_tokenToAssetIndex.json"),
    ] {
        assert_eq!(read_asset(&answer(text)), Ok(Some(AssetId::COLLATERAL)), "USDC and USDG are asset 3");
    }
    assert_eq!(read_asset(&[0u8; 32]), Ok(None));
    assert_eq!(read_pending_balance(&U256::from(10_000_000u64).to_be_bytes()), Ok(10_000_000));
    assert!(read_desert_mode(&[0u8; 31]).is_err());
}

#[test]
fn a_deposit_without_an_allowance_reverts_with_the_token_s_reason() {
    let text = include_str!("../../../../fixtures/lighter/eth_call_deposit_reverted.json");
    let refused = eth::call(&CallRequest::default(), BlockTag::Latest).answer(text).unwrap_err();
    let RpcError::Node { error, .. } = refused else { panic!("{refused:?}") };
    assert_eq!((error.code, error.kind()), (3, ErrorKind::Reverted));
    assert_eq!(error.revert(), Some(Revert::Message("ERC20: transfer amount exceeds allowance".to_owned())));
}
