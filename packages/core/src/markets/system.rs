//! The venue itself: its own accounts and limits (`systemConfig`), its contract (`/info`, from the
//! host's root), its L1 (`layer1BasicInfo`), how long a secure withdrawal waits
//! (`withdrawalDelay`) and its announcements (`announcement`).
//!
//! Robinhood Chain writes 2^48 - 1, which is no account, for the pools it does not run (the LLP
//! and staking); [`SystemConfig::liquidity_pool`] and [`SystemConfig::staking_pool`] read it as
//! `None`.

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::markets::listed;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, Address};

/// `systemConfig`: the venue's own accounts, the pools' timings, and the most an integrator may
/// charge. Fees are in millionths of the notional, times in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SystemConfig {
    #[serde(deserialize_with = "whole")]
    pub liquidity_pool_index: u64,
    #[serde(deserialize_with = "whole")]
    pub staking_pool_index: u64,
    #[serde(deserialize_with = "whole")]
    pub funding_fee_rebate_account_index: u64,
    #[serde(default, deserialize_with = "whole")]
    pub market_maker_incentive_account_index: u64,
    #[serde(deserialize_with = "whole")]
    pub fee_collector_account_index: u64,
    /// How long a share of the LLP waits before it may be withdrawn.
    #[serde(deserialize_with = "whole")]
    pub liquidity_pool_cooldown_period: u64,
    /// How long staked LIT stays locked.
    #[serde(deserialize_with = "whole")]
    pub staking_pool_lockup_period: u64,
    pub max_integrator_spot_taker_fee: u32,
    pub max_integrator_spot_maker_fee: u32,
    pub max_integrator_perps_taker_fee: u32,
    pub max_integrator_perps_maker_fee: u32,
}

impl SystemConfig {
    /// The LLP; `None` where the venue runs none.
    #[must_use]
    pub fn liquidity_pool(&self) -> Option<AccountIndex> {
        AccountIndex::new(self.liquidity_pool_index).ok()
    }

    /// The LIT staking pool; `None` where the venue runs none.
    #[must_use]
    pub fn staking_pool(&self) -> Option<AccountIndex> {
        AccountIndex::new(self.staking_pool_index).ok()
    }
}

/// What `systemConfig` asks.
#[must_use]
pub fn system_config_call() -> Call {
    Call::get("systemConfig")
}

pub async fn system_config(transport: &impl Transport) -> Result<SystemConfig, ApiError> {
    read(transport, system_config_call()).await
}

#[derive(Deserialize)]
struct ZkLighterInfo {
    contract_address: Address,
}

/// What `/info` asks, from the host's root rather than under `/api/v1/`.
#[must_use]
pub fn info_call() -> Call {
    Call::get("/info")
}

/// The ZkLighter contract on the network's L1, as the venue states it.
pub async fn contract_address(transport: &impl Transport) -> Result<Address, ApiError> {
    Ok(read::<ZkLighterInfo>(transport, info_call()).await?.contract_address)
}

/// An L1 node the venue reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct L1Provider {
    #[serde(rename = "chainId", deserialize_with = "whole")]
    pub chain_id: u64,
    #[serde(rename = "networkId", deserialize_with = "whole")]
    pub network_id: u64,
    /// Written 0.
    #[serde(rename = "latestBlockNumber", default, deserialize_with = "whole")]
    pub latest_block_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Validator {
    pub address: Address,
    pub is_active: bool,
}

/// A contract the venue names.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NamedContract {
    /// `ZkLighterContract`, `USDCContract` (USDG's on Robinhood Chain).
    pub name: String,
    pub address: Address,
}

/// `layer1BasicInfo`: the L1 the network settles on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct L1Info {
    #[serde(default, deserialize_with = "listed")]
    pub l1_providers: Vec<L1Provider>,
    pub l1_providers_health: bool,
    #[serde(default, deserialize_with = "listed")]
    pub validator_info: Vec<Validator>,
    #[serde(default, deserialize_with = "listed")]
    pub contract_addresses: Vec<NamedContract>,
    #[serde(default, deserialize_with = "whole")]
    pub latest_l1_generic_block: u64,
    #[serde(default, deserialize_with = "whole")]
    pub latest_l1_governance_block: u64,
    #[serde(default, deserialize_with = "whole")]
    pub latest_l1_desert_block: u64,
}

impl L1Info {
    /// The contract named `name`.
    #[must_use]
    pub fn contract(&self, name: &str) -> Option<Address> {
        self.contract_addresses.iter().find(|contract| contract.name == name).map(|contract| contract.address)
    }

    /// The L1's chain id.
    #[must_use]
    pub fn chain_id(&self) -> Option<u64> {
        self.l1_providers.first().map(|provider| provider.chain_id)
    }
}

/// What `layer1BasicInfo` asks.
#[must_use]
pub fn layer1_basic_info_call() -> Call {
    Call::get("layer1BasicInfo")
}

pub async fn layer1_basic_info(transport: &impl Transport) -> Result<L1Info, ApiError> {
    read(transport, layer1_basic_info_call()).await
}

#[derive(Deserialize)]
struct WithdrawalDelay {
    #[serde(deserialize_with = "whole")]
    seconds: u64,
}

/// What `withdrawalDelay` asks.
#[must_use]
pub fn withdrawal_delay_call() -> Call {
    Call::get("withdrawalDelay")
}

/// How long a secure withdrawal waits now, in seconds.
pub async fn withdrawal_delay(transport: &impl Transport) -> Result<u64, ApiError> {
    Ok(read::<WithdrawalDelay>(transport, withdrawal_delay_call()).await?.seconds)
}

/// One announcement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Announcement {
    pub title: String,
    /// Plain text, lines split by `\n`.
    pub content: String,
    /// Unix seconds.
    #[serde(deserialize_with = "whole")]
    pub created_at: u64,
    /// When it stops showing, unix seconds; 0 when the venue left it out (older ones on Lighter).
    #[serde(default, deserialize_with = "whole")]
    pub expired_at: u64,
}

#[derive(Deserialize)]
struct Announcements {
    #[serde(default, deserialize_with = "listed")]
    announcements: Vec<Announcement>,
}

/// What `announcement` asks.
#[must_use]
pub fn announcements_call() -> Call {
    Call::get("announcement")
}

/// The venue's announcements, newest first.
pub async fn announcements(transport: &impl Transport) -> Result<Vec<Announcement>, ApiError> {
    Ok(read::<Announcements>(transport, announcements_call()).await?.announcements)
}

#[cfg(test)]
#[path = "tests/system.rs"]
mod tests;
