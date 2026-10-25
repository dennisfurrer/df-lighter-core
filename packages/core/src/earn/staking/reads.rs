//! Reading the stake: the staking pool (`publicPoolsMetadata` with `filter=stake`, the only filter
//! that lists it), an account's stake (its staking pool share, in `account`'s `shares`) and the
//! LIT it unstaked that is still locked (`account`'s `pending_unlocks`).

use serde::Deserialize;

use crate::client::Network;
use crate::client::http::{ApiError, Transport};
use crate::earn::pools::reads::{PoolFilter, PublicPool, account_row, public_pool};
use crate::earn::staking::actions::staking_pool_index;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, AssetId, Dec};

/// The network's staking pool as `publicPoolsMetadata` lists it: its LIT ([`PublicPool::assets`]),
/// its shares, and [`PublicPool::lit_per_share`]. `None` on Robinhood Chain, which runs none,
/// without asking.
pub async fn staking_pool(transport: &impl Transport, network: Network) -> Result<Option<PublicPool>, ApiError> {
    match staking_pool_index(network) {
        Some(index) => public_pool(transport, index, PoolFilter::Stake).await,
        None => Ok(None),
    }
}

/// Unstaked LIT still locked: paid back to the account when the lockup it started runs out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PendingUnlock {
    /// When it is paid back, as the venue writes it. The docs give no unit; the lockup it ends
    /// is counted in milliseconds (`staking_pool_lockup_period`).
    #[serde(deserialize_with = "whole")]
    pub unlock_timestamp: u64,
    /// LIT: only LIT is ever locked (`21141`).
    #[serde(rename = "asset_index")]
    pub asset: AssetId,
    /// In the asset's own units: LIT, to eight places.
    pub amount: Dec,
}

/// The LIT `account` unstaked that is still locked, as `account` lists it; none for an index no
/// account has.
pub async fn pending_unlocks(
    transport: &impl Transport,
    account: AccountIndex,
) -> Result<Vec<PendingUnlock>, ApiError> {
    Ok(account_row(transport, account).await?.map(|row| row.pending_unlocks).unwrap_or_default())
}

/// The shares `account` holds in the network's staking pool: 0 when it holds none or the
/// network runs no staking pool (then nothing is asked).
pub async fn staked_shares(
    transport: &impl Transport,
    network: Network,
    account: AccountIndex,
) -> Result<u64, ApiError> {
    let Some(pool) = staking_pool_index(network) else { return Ok(0) };
    let shares = account_row(transport, account).await?.map(|row| row.shares).unwrap_or_default();
    Ok(shares.iter().find(|share| share.pool == pool).map_or(0, |share| share.shares))
}

#[cfg(test)]
#[path = "tests/reads.rs"]
mod tests;
