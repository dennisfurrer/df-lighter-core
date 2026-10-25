//! The token calls held to go-ethereum 1.15.6's ABI packing (`vectors/lighter_go.json`, `evm`),
//! each selector to the hash of its signature.

use serde_json::Value;

use super::*;
use crate::test_support::vectors;
use crate::utils::hex;
use crate::utils::sign::keccak256;

fn case(name: &str) -> Value {
    vectors()["evm"].as_array().unwrap().iter().find(|case| case["name"] == name).unwrap().clone()
}

fn address(value: &Value) -> Address {
    value.as_str().unwrap().parse().unwrap()
}

#[test]
fn every_call_packs_as_go_ethereum_packs_it() {
    for (signature, selector) in [
        ("approve(address,uint256)", APPROVE),
        ("allowance(address,address)", ALLOWANCE),
        ("balanceOf(address)", BALANCE_OF),
    ] {
        assert_eq!(keccak256(signature.as_bytes())[..4], selector, "{signature}");
    }
    let approval = case("approve-lighter");
    let amount: U256 = approval["args"][1].as_str().unwrap().parse().unwrap();
    let calldata = approve(address(&approval["args"][0]), amount);
    assert_eq!(hex::encode_prefixed(&calldata), approval["calldata"].as_str().unwrap());
    let read = case("allowance");
    let calldata = allowance(address(&read["args"][0]), address(&read["args"][1]));
    assert_eq!(hex::encode_prefixed(&calldata), read["calldata"].as_str().unwrap());
    let balance = case("balance-of");
    assert_eq!(hex::encode_prefixed(&balance_of(address(&balance["args"][0]))), balance["calldata"].as_str().unwrap());
}

#[test]
fn an_amount_reads_from_its_word() {
    assert_eq!(read_amount(&U256::from(25_500_000u64).to_be_bytes()), Ok(U256::from(25_500_000u64)));
    assert_eq!(read_amount(&U256::MAX.to_be_bytes()), Ok(U256::MAX), "an allowance that never runs out");
    assert_eq!(read_amount(&[]), Err(AbiError::Short(0)));
}
