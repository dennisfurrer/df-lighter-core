//! The venue's bridge, its own REST reads about money arriving: the chains an intent address
//! takes deposits from (`deposit/networks`), an address's latest deposit (`deposit/latest`), an
//! intent address made for it (`createIntentAddress`), and what the fast bridge can carry now
//! (`fastbridge/info`). All are public.
//!
//! An intent address is persistent and tied to the address that made it: a plain transfer of the
//! collateral to it, on a chain `deposit/networks` lists, is credited with no contract call
//! (Circle's CCTP on Lighter, from 5 USDC; Robinhood Chain itself on Robinhood Chain, from 1
//! USDG).

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::{Address, Dec};

/// A chain an intent address takes deposits from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DepositNetwork {
    pub name: String,
    /// The EVM chain id (Solana's is not an EVM one), written as text.
    #[serde(deserialize_with = "chain")]
    pub chain_id: u64,
    /// The chain's block explorer; empty when the venue gives none.
    #[serde(default)]
    pub explorer: String,
}

#[derive(Deserialize)]
struct DepositNetworks {
    networks: Vec<DepositNetwork>,
}

/// An address's most recent deposit, as the bridge tracks it from its source chain to the
/// account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LatestDeposit {
    /// Where it came from: `Solana`, `Arbitrum`, and the like.
    #[serde(default)]
    pub source: String,
    /// The source chain's id as the venue writes it (`101` is Solana's, not an EVM chain id).
    #[serde(default)]
    pub source_chain_id: String,
    /// The collateral, in whole units: `5.000000` is five.
    pub amount: Dec,
    #[serde(default)]
    pub intent_address: String,
    /// `completed` once credited; the bridge's own words otherwise.
    #[serde(default)]
    pub status: String,
    /// The bridge's step, as text.
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub description: String,
    /// Unix seconds.
    #[serde(default)]
    pub created_at: u64,
    /// Unix seconds.
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub fast_bridge_tx_hash: String,
    #[serde(default)]
    pub batch_claim_tx_hash: String,
    #[serde(default)]
    pub cctp_burn_tx_hash: String,
    #[serde(default)]
    pub is_external_deposit: bool,
    #[serde(default)]
    pub is_next_bridge_fast: bool,
}

impl LatestDeposit {
    /// Credited to the account.
    #[must_use]
    pub fn is_completed(&self) -> bool {
        self.status == "completed"
    }
}

/// What `createIntentAddress` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IntentAddress {
    /// Where a plain transfer of the collateral is credited to the address that made it: an EVM
    /// address, or a Solana one.
    pub intent_address: String,
}

#[derive(Deserialize)]
struct FastBridgeInfo {
    fast_bridge_limit: Dec,
}

/// A JSON string of digits as an integer.
fn chain<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    crate::utils::ids::whole(deserializer)
}

/// What `deposit/networks` asks.
#[must_use]
pub fn deposit_networks_call() -> Call {
    Call::get("deposit/networks")
}

/// The chains an intent address takes deposits from: Arbitrum, Base, Avalanche, Arc and HyperEVM
/// on Lighter; Robinhood Chain itself on Robinhood Chain.
pub async fn deposit_networks(transport: &impl Transport) -> Result<Vec<DepositNetwork>, ApiError> {
    Ok(read::<DepositNetworks>(transport, deposit_networks_call()).await?.networks)
}

/// What `deposit/latest` asks.
#[must_use]
pub fn latest_deposit_call(l1_address: Address) -> Call {
    Call::get("deposit/latest").field("l1_address", l1_address)
}

/// The code `deposit/latest` refuses an address with no deposit with ("not found: User has no
/// pending deposit").
pub const NO_DEPOSIT: i64 = crate::client::http::codes::NOT_FOUND;

/// The deposit of `l1_address`'s still on its way; `None` when none is (the venue answers 29404
/// "User has no pending deposit" once the last one has landed).
pub async fn latest_deposit(
    transport: &impl Transport,
    l1_address: Address,
) -> Result<Option<LatestDeposit>, ApiError> {
    match read(transport, latest_deposit_call(l1_address)).await {
        Err(error) if error.code() == Some(NO_DEPOSIT) => Ok(None),
        answer => answer.map(Some),
    }
}

/// What `createIntentAddress` posts (weight 3,000): an intent address for `from` on `chain_id`,
/// `amount` the collateral expected (`0` for any), `external` for one a third party pays into.
#[must_use]
pub fn intent_address_call(chain_id: u64, from: Address, amount: Dec, external: bool) -> Call {
    Call::post("createIntentAddress")
        .field("chain_id", chain_id)
        .field("from_addr", from)
        .field("amount", amount)
        .field("is_external_deposit", external)
}

/// Makes, or finds again, `from`'s intent address on `chain_id`.
pub async fn intent_address(
    transport: &impl Transport,
    chain_id: u64,
    from: Address,
    amount: Dec,
    external: bool,
) -> Result<IntentAddress, ApiError> {
    read(transport, intent_address_call(chain_id, from, amount, external)).await
}

/// What `fastbridge/info` asks.
#[must_use]
pub fn fast_bridge_call() -> Call {
    Call::get("fastbridge/info")
}

/// What the fast bridge can carry now, in whole units of the collateral as the venue writes it:
/// `0` when it carries nothing and deposits go the slow way.
pub async fn fast_bridge_limit(transport: &impl Transport) -> Result<Dec, ApiError> {
    Ok(read::<FastBridgeInfo>(transport, fast_bridge_call()).await?.fast_bridge_limit)
}

#[cfg(test)]
#[path = "tests/bridge.rs"]
mod tests;
