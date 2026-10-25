//! Every method this crate asks, as a [`Query`]: its params and how its answer reads.

use crate::client::evm::records::{FeeHistory, Receipt};
use crate::client::evm::rpc::query::{address, data, json, quantity, small, tag};
use crate::client::evm::rpc::{BlockTag, CallRequest, Query};
use crate::client::evm::u256::{B256, U256};
use crate::utils::{Address, Json, hex};

fn query<T>(method: &'static str, params: Vec<Json>, read: fn(&str) -> Result<T, String>) -> Query<T> {
    Query { method, params, read }
}

/// `eth_chainId`: 1 for Ethereum, 4663 for Robinhood Chain.
#[must_use]
pub fn chain_id() -> Query<u64> {
    query("eth_chainId", Vec::new(), small)
}

/// `eth_getBalance`: wei of the chain's coin, ETH on both L1s.
#[must_use]
pub fn balance(owner: Address, block: BlockTag) -> Query<U256> {
    query("eth_getBalance", vec![address(owner), tag(block)], json)
}

/// `eth_getTransactionCount`: the next nonce, counting the pool at [`BlockTag::Pending`].
#[must_use]
pub fn transaction_count(owner: Address, block: BlockTag) -> Query<u64> {
    query("eth_getTransactionCount", vec![address(owner), tag(block)], small)
}

/// `eth_gasPrice`: a legacy price, base fee and tip together, in wei per gas.
#[must_use]
pub fn gas_price() -> Query<U256> {
    query("eth_gasPrice", Vec::new(), json)
}

/// `eth_maxPriorityFeePerGas`: the tip the node suggests, in wei per gas.
#[must_use]
pub fn max_priority_fee() -> Query<U256> {
    query("eth_maxPriorityFeePerGas", Vec::new(), json)
}

/// `eth_feeHistory` over `blocks` blocks up to `newest`, with the tip at each whole percentile in
/// `percentiles`, 0 to 100, rising.
#[must_use]
pub fn fee_history(blocks: u64, newest: BlockTag, percentiles: &[u8]) -> Query<FeeHistory> {
    let percentiles = percentiles.iter().map(|percent| Json::Number(percent.to_string())).collect();
    query("eth_feeHistory", vec![quantity(U256::from_u64(blocks)), tag(newest), Json::Array(percentiles)], json)
}

/// `eth_estimateGas`: the gas the call would use now.
#[must_use]
pub fn estimate_gas(call: &CallRequest) -> Query<u64> {
    query("eth_estimateGas", vec![call.json()], small)
}

/// `eth_call`: what the call returns, as bytes.
#[must_use]
pub fn call(call: &CallRequest, block: BlockTag) -> Query<Vec<u8>> {
    query("eth_call", vec![call.json(), tag(block)], data)
}

/// `eth_sendRawTransaction`: the hash the node took it under. The one signed request.
#[must_use]
pub fn send_raw_transaction(raw: &[u8]) -> Query<B256> {
    query("eth_sendRawTransaction", vec![Json::String(hex::encode_prefixed(raw))], json)
}

/// `eth_getTransactionReceipt`: `None` until it is mined.
#[must_use]
pub fn receipt(hash: B256) -> Query<Option<Receipt>> {
    query("eth_getTransactionReceipt", vec![Json::String(hash.to_hex())], json)
}
