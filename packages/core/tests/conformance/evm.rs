//! The L1, held to go-ethereum 1.15.6: every call to the ZkLighter contract and to the collateral
//! token packed as `abi.Pack` packs it, each through the builder a caller uses (the contract's
//! calls, a deposit's and an approval's [`Call`], the token's reads), its selector the hash of the
//! signature go-ethereum names; and a deposit on Ethereum, an approval and a deposit on Robinhood
//! Chain made into EIP-1559 transactions and signed by the owner (Hardhat's first account) to
//! `types.SignTx`'s bytes and hash.

use df_lighter_core::Network;
use df_lighter_core::client::evm::{Call, Eip1559, Fees, Function, U256, tx_hash, zklighter};
use df_lighter_core::trade::OrderKind;
use df_lighter_core::transfer::deposit::{approve_call, asset_deposit_call, deposit_call};
use df_lighter_core::transfer::erc20;
use df_lighter_core::utils::sign::PublicKey;
use df_lighter_core::utils::{AccountIndex, Address, ApiKeyIndex, AssetId, Dec, MarketId, Route};
use df_lighter_core::utils::{from_units, hex};
use serde_json::Value;

use crate::support::{owner, owner_address, vectors};
use crate::vector::text;

/// The cases on file, calls then transactions.
pub const CASES: [&str; 17] = [
    "deposit-usdc-perps",
    "deposit-eth-spot",
    "approve-lighter",
    "withdraw-usdc",
    "change-pub-key",
    "cancel-all-orders",
    "create-order",
    "burn-shares",
    "withdraw-pending-balance",
    "address-to-account-index",
    "get-pending-balance",
    "token-to-asset-index",
    "allowance",
    "balance-of",
    "deposit-tx-ethereum",
    "approve-tx-robinhood",
    "deposit-tx-robinhood",
];

/// What a deposit's calldata in the vectors moves: 25.5 of the collateral.
const DEPOSIT: Dec = Dec::lit::<1>(255);

fn address(value: &Value) -> Address {
    value.as_str().unwrap().parse().unwrap()
}

/// A number go-ethereum was handed as a JSON number or, past 2^53, as its digits.
fn big(value: &Value) -> U256 {
    match value {
        Value::String(digits) => digits.parse().unwrap(),
        other => U256::from(other.as_u64().unwrap()),
    }
}

fn small(value: &Value) -> u64 {
    big(value).to_u64().unwrap()
}

fn account(value: &Value) -> AccountIndex {
    AccountIndex::new(small(value)).unwrap()
}

fn asset(value: &Value) -> AssetId {
    AssetId::new(u16::try_from(small(value)).unwrap()).unwrap()
}

fn route(value: &Value) -> Route {
    [Route::Perps, Route::Spot].into_iter().find(|route| u64::from(route.code()) == small(value)).unwrap()
}

/// `units` at `places` decimals, as a caller writes the amount.
fn amount(units: U256, places: u32) -> Dec {
    from_units(i128::try_from(units.to_u128().unwrap()).unwrap(), places).unwrap()
}

/// The calldata every builder that makes this call makes, from the arguments go-ethereum packed.
fn built(name: &str, args: &[Value]) -> Vec<Vec<u8>> {
    let lighter = Network::Lighter;
    match name {
        "deposit-usdc-perps" => {
            let (to, units) = (address(&args[0]), big(&args[3]));
            assert_eq!((asset(&args[1]), route(&args[2])), (lighter.collateral().asset, Route::Perps));
            let call = deposit_call(lighter, to, amount(units, lighter.collateral().l1_decimals)).unwrap();
            assert_eq!((call.to, call.value), (Some(lighter.contract()), U256::ZERO), "{name}: the contract, no value");
            vec![zklighter::deposit(to, asset(&args[1]), route(&args[2]), units), call.data]
        }
        "deposit-eth-spot" => {
            let (to, wei) = (address(&args[0]), big(&args[3]));
            let call = asset_deposit_call(lighter, to, AssetId::ETH, Route::Spot, None, 18, amount(wei, 18)).unwrap();
            assert_eq!((call.to, call.value), (Some(lighter.contract()), wei), "{name}: ETH goes as the value");
            vec![zklighter::deposit(to, asset(&args[1]), route(&args[2]), wei), call.data]
        }
        "approve-lighter" => {
            let (spender, units) = (address(&args[0]), big(&args[1]));
            assert_eq!(spender, lighter.contract());
            let call = approve_call(lighter, units);
            assert_eq!(call.to, Some(lighter.collateral().token), "{name}: on USDC");
            vec![erc20::approve(spender, units), call.data]
        }
        "withdraw-usdc" => {
            vec![zklighter::withdraw(account(&args[0]), asset(&args[1]), route(&args[2]), small(&args[3]))]
        }
        "change-pub-key" => {
            let slot = ApiKeyIndex::new(u8::try_from(small(&args[1])).unwrap()).unwrap();
            let key = PublicKey::from_hex(args[2].as_str().unwrap()).unwrap();
            vec![zklighter::change_pub_key(account(&args[0]), slot, &key)]
        }
        "cancel-all-orders" => vec![zklighter::cancel_all_orders(account(&args[0]))],
        "create-order" => {
            let market = MarketId::new(small(&args[1])).unwrap();
            let price = u32::try_from(small(&args[3])).unwrap();
            let is_ask = small(&args[4]) == 1;
            let kind = OrderKind::from_code(u8::try_from(small(&args[5])).unwrap()).unwrap();
            vec![zklighter::create_order(account(&args[0]), market, small(&args[2]), price, is_ask, kind).unwrap()]
        }
        "burn-shares" => vec![zklighter::burn_shares(account(&args[0]), account(&args[1]), small(&args[2]))],
        "withdraw-pending-balance" => {
            let units = big(&args[2]).to_u128().unwrap();
            vec![zklighter::withdraw_pending_balance(address(&args[0]), asset(&args[1]), units)]
        }
        "address-to-account-index" => vec![zklighter::address_to_account_index(address(&args[0]))],
        "get-pending-balance" => vec![zklighter::get_pending_balance(address(&args[0]), asset(&args[1]))],
        "token-to-asset-index" => {
            assert_eq!(address(&args[0]), lighter.collateral().token, "{name}: USDC");
            vec![zklighter::token_to_asset_index(address(&args[0]))]
        }
        "allowance" => vec![erc20::allowance(address(&args[0]), address(&args[1]))],
        "balance-of" => vec![erc20::balance_of(address(&args[0]))],
        other => panic!("no call named {other}"),
    }
}

/// The call a transaction case makes, built from typed values, and the network it is made on.
fn call_of(name: &str) -> (Network, Call) {
    let deposit = |network| deposit_call(network, owner_address(), DEPOSIT).unwrap();
    let units = U256::from(25_500_000u64);
    match name {
        "deposit-tx-ethereum" => (Network::Lighter, deposit(Network::Lighter)),
        "approve-tx-robinhood" => (Network::Robinhood, approve_call(Network::Robinhood, units)),
        "deposit-tx-robinhood" => (Network::Robinhood, deposit(Network::Robinhood)),
        other => panic!("no transaction named {other}"),
    }
}

fn evm_cases() -> Vec<Value> {
    vectors()["evm"].as_array().unwrap().clone()
}

#[test]
fn every_call_packs_as_go_ethereum_packs_it_through_each_builder() {
    let cases: Vec<Value> = evm_cases().into_iter().filter(|case| case["function"] != "eip1559").collect();
    assert_eq!(cases.iter().map(|case| text(case, "name")).collect::<Vec<_>>(), CASES[..14]);
    for case in &cases {
        let name = text(case, "name");
        let calldata = text(case, "calldata");
        let function = Function::parse(text(case, "function")).unwrap();
        for built in built(name, case["args"].as_array().unwrap()) {
            assert_eq!(hex::encode_prefixed(&built), calldata, "{name}");
            assert_eq!(built[..4], function.selector(), "{name}: the selector of {}", function.signature());
        }
    }
}

#[test]
fn every_transaction_signs_to_go_ethereum_s_bytes_and_hash() {
    let cases: Vec<Value> = evm_cases().into_iter().filter(|case| case["function"] == "eip1559").collect();
    assert_eq!(cases.iter().map(|case| text(case, "name")).collect::<Vec<_>>(), CASES[14..]);
    let wallet = owner();
    for case in &cases {
        let name = text(case, "name");
        let args = case["args"].as_array().unwrap();
        let (network, call) = call_of(name);
        assert_eq!(network.l1_chain_id(), small(&args[0]), "{name}: the chain");
        assert_eq!((call.to, call.value), (Some(address(&args[5])), big(&args[6])), "{name}: to and value");
        assert_eq!(hex::encode_prefixed(&call.data), args[7].as_str().unwrap(), "{name}: calldata");

        // The node's nonce, gas and fees, as the vector was made with them.
        let fees = Fees { max_priority_fee: big(&args[2]), max_fee: big(&args[3]) };
        let tx: Eip1559 = call.transaction(network.l1_chain_id(), small(&args[1]), small(&args[4]), fees);
        let raw = tx.sign(&wallet).unwrap();
        assert_eq!(hex::encode_prefixed(&raw), text(&case["tx"], "raw"), "{name}: raw");
        assert_eq!(tx_hash(&raw).to_hex(), text(&case["tx"], "hash"), "{name}: hash");
        let signature = wallet.sign_digest(&tx.signing_hash().0).unwrap();
        assert_eq!(tx.signed(&signature).unwrap(), raw, "{name}: from a signature made elsewhere");
        assert_eq!(tx.signer(&signature).unwrap(), owner_address(), "{name}: the owner signed it");
    }
}
