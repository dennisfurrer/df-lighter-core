//! Staking LIT: `StakeAssets` (35) and `UnstakeAssets` (36). Like a pool's mint and burn, both
//! count in the staking pool's shares, never in LIT: the venue prices them at the LIT the pool
//! holds over its shares when it runs the transaction (a share started at 0.00001 LIT and grows
//! with the rewards). A stake takes the LIT from the account's balance; an unstake becomes a
//! pending unlock, paid back once [`STAKING_LOCKUP_MS`] has passed.
//!
//! Only Lighter runs a staking pool ([`staking_pool_index`]). The venue takes at most one unstake
//! every fifteen seconds.

use crate::client::Network;
use crate::earn::pools::shares::check_pool_shares;
use crate::utils::AccountIndex;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Tx, TxType, element, number};
#[cfg(feature = "sign")]
use {
    crate::account::keys::ApiKey,
    crate::client::{Client, Http, SendError, Sent},
    crate::earn::staking::reads::staking_pool,
    crate::utils::Dec,
    crate::utils::sign::SignTxError,
};

/// Lighter's LIT staking pool (`systemConfig`'s `staking_pool_index`).
pub const LIGHTER_STAKING_POOL: AccountIndex = AccountIndex::of::<281_474_976_624_800>();

/// How long unstaked LIT waits before it is paid back (`staking_pool_lockup_period`): three days.
pub const STAKING_LOCKUP_MS: u64 = 259_200_000;

/// The network's staking pool: Lighter's, and none on Robinhood Chain, whose `systemConfig`
/// names none and whose `publicPoolsMetadata` lists no staking pool.
#[must_use]
pub const fn staking_pool_index(network: Network) -> Option<AccountIndex> {
    match network {
        Network::Lighter => Some(LIGHTER_STAKING_POOL),
        Network::Robinhood => None,
    }
}

/// `StakeAssets`: `shares` of the staking pool, bought with the account's LIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StakeAssets {
    pub pool: AccountIndex,
    pub shares: u64,
}

impl StakeAssets {
    #[must_use]
    pub const fn new(pool: AccountIndex, shares: u64) -> StakeAssets {
        StakeAssets { pool, shares }
    }
}

impl Tx for StakeAssets {
    fn tx_type(&self) -> TxType {
        TxType::StakeAssets
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.pool.get()), element(self.shares)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("StakingPoolIndex", number(self.pool.get())), ("ShareAmount", number(self.shares))]
    }

    fn check(&self) -> Result<(), BadTx> {
        check_pool_shares("StakingPoolIndex", self.pool, self.shares)
    }
}

/// `UnstakeAssets`: `shares` of the staking pool sold back, their LIT locked until the lockup ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnstakeAssets {
    pub pool: AccountIndex,
    pub shares: u64,
}

impl UnstakeAssets {
    #[must_use]
    pub const fn new(pool: AccountIndex, shares: u64) -> UnstakeAssets {
        UnstakeAssets { pool, shares }
    }
}

impl Tx for UnstakeAssets {
    fn tx_type(&self) -> TxType {
        TxType::UnstakeAssets
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.pool.get()), element(self.shares)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("StakingPoolIndex", number(self.pool.get())), ("ShareAmount", number(self.shares))]
    }

    fn check(&self) -> Result<(), BadTx> {
        check_pool_shares("StakingPoolIndex", self.pool, self.shares)
    }
}

#[cfg(feature = "sign")]
fn refused(field: &'static str, why: impl Into<String>) -> SendError {
    SendError::Sign(SignTxError::Bad(BadTx::new(field, why)))
}

#[cfg(feature = "sign")]
fn pool_of<H: Http>(client: &Client<H, ApiKey>) -> Result<AccountIndex, SendError> {
    let network = client.network();
    staking_pool_index(network).ok_or_else(|| refused("StakingPoolIndex", format!("{network} runs no staking pool")))
}

/// Stakes about `lit`: the whole staking shares it buys at the pool's count as
/// `publicPoolsMetadata` answers it now, staked once. Refused before anything is signed on a
/// network with no staking pool, or when `lit` buys no whole share.
#[cfg(feature = "sign")]
pub async fn stake<H: Http>(client: &Client<H, ApiKey>, lit: Dec) -> Result<Sent, SendError> {
    let index = pool_of(client)?;
    let Some(pool) = staking_pool(client, client.network()).await? else {
        return Err(refused("StakingPoolIndex", format!("the venue lists no staking pool at {index}")));
    };
    match pool.shares_for_lit(lit) {
        Some(shares) if shares > 0 => client.send(&StakeAssets::new(index, shares)).await,
        _ => Err(refused("ShareAmount", format!("{lit} LIT buys no whole staking share"))),
    }
}

/// Unstakes `shares` of the network's staking pool, sent once; the LIT comes back after the
/// lockup. Refused before anything is signed on a network with no staking pool.
#[cfg(feature = "sign")]
pub async fn unstake<H: Http>(client: &Client<H, ApiKey>, shares: u64) -> Result<Sent, SendError> {
    let index = pool_of(client)?;
    client.send(&UnstakeAssets::new(index, shares)).await
}

#[cfg(test)]
#[path = "tests/actions.rs"]
mod tests;
