//! Depositing into a pool and withdrawing: `MintShares` (18) and `BurnShares` (19). Both count
//! in the pool's shares, never in collateral: the venue prices them when it runs the transaction,
//! at the pool's value over its shares then, and debits or credits the collateral that comes to.
//! A deposit sized in collateral is converted at the last value read ([`deposit`]), so what it
//! costs moves with the pool between the read and the run.
//!
//! Anyone may mint into a pool that is active and burn from any; the LLP also asks staked LIT
//! (10 USDC a LIT on Lighter) and holds each deposit for [`llp_cooldown_ms`] before it may leave.
//! Both venues take at most one mint every fifteen seconds.

use crate::client::Network;
use crate::utils::AccountIndex;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Tx, TxType, element, number};
#[cfg(feature = "sign")]
use {
    crate::account::keys::ApiKey,
    crate::client::{Client, Http, SendError, Sent},
    crate::earn::pools::reads::{PoolFilter, public_pool, shares_in},
    crate::utils::Dec,
    crate::utils::sign::SignTxError,
};

/// The most shares one mint or burn moves: 2^60 - 1.
pub const MOST_SHARES: u64 = (1 << 60) - 1;

/// How long a deposit into the LLP stays before it may be withdrawn (`systemConfig`'s
/// `liquidity_pool_cooldown_period`): five minutes on Lighter, none on Robinhood Chain.
#[must_use]
pub const fn llp_cooldown_ms(network: Network) -> u64 {
    match network {
        Network::Lighter => 300_000,
        Network::Robinhood => 0,
    }
}

/// A pool's index and a share count, checked as lighter-go checks a mint, a burn, a stake and an
/// unstake: a pool sits among the sub-accounts, at 2^47 and above, and moves 1 to 2^60 - 1
/// shares.
pub(crate) fn check_pool_shares(pool_field: &'static str, pool: AccountIndex, shares: u64) -> Result<(), BadTx> {
    if pool.is_master() {
        return Err(BadTx::new(pool_field, "a pool's index is 2^47 or above, among the sub-accounts"));
    }
    if shares == 0 || shares > MOST_SHARES {
        return Err(BadTx::new("ShareAmount", "a share amount is 1 to 2^60 - 1"));
    }
    Ok(())
}

/// `MintShares`: `shares` of `pool` bought with the account's collateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MintShares {
    pub pool: AccountIndex,
    pub shares: u64,
}

impl MintShares {
    #[must_use]
    pub const fn new(pool: AccountIndex, shares: u64) -> MintShares {
        MintShares { pool, shares }
    }
}

impl Tx for MintShares {
    fn tx_type(&self) -> TxType {
        TxType::MintShares
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.pool.get()), element(self.shares)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("PublicPoolIndex", number(self.pool.get())), ("ShareAmount", number(self.shares))]
    }

    fn check(&self) -> Result<(), BadTx> {
        check_pool_shares("PublicPoolIndex", self.pool, self.shares)
    }
}

/// `BurnShares`: `shares` of `pool` sold back for collateral, less the operator's fee on their
/// profit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurnShares {
    pub pool: AccountIndex,
    pub shares: u64,
}

impl BurnShares {
    #[must_use]
    pub const fn new(pool: AccountIndex, shares: u64) -> BurnShares {
        BurnShares { pool, shares }
    }
}

impl Tx for BurnShares {
    fn tx_type(&self) -> TxType {
        TxType::BurnShares
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.pool.get()), element(self.shares)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("PublicPoolIndex", number(self.pool.get())), ("ShareAmount", number(self.shares))]
    }

    fn check(&self) -> Result<(), BadTx> {
        check_pool_shares("PublicPoolIndex", self.pool, self.shares)
    }
}

#[cfg(feature = "sign")]
fn refused(field: &'static str, why: impl Into<String>) -> SendError {
    SendError::Sign(SignTxError::Bad(BadTx::new(field, why)))
}

/// Deposits about `collateral` into `pool`: the whole shares it buys at the pool's value as
/// `publicPoolsMetadata` answers it now, minted once. Refused before anything is signed when the
/// pool is unknown, frozen, or `collateral` buys no whole share.
#[cfg(feature = "sign")]
pub async fn deposit<H: Http>(
    client: &Client<H, ApiKey>,
    pool: AccountIndex,
    collateral: Dec,
) -> Result<Sent, SendError> {
    let Some(listed) = public_pool(client, pool, PoolFilter::Any).await? else {
        return Err(refused("PublicPoolIndex", format!("{pool} is no public pool")));
    };
    if !listed.takes_deposits() {
        return Err(refused("PublicPoolIndex", format!("pool {pool} is frozen: it only pays out")));
    }
    match listed.shares_for(collateral) {
        Some(shares) if shares > 0 => client.send(&MintShares::new(pool, shares)).await,
        _ => Err(refused("ShareAmount", format!("{collateral} buys no whole share of pool {pool}"))),
    }
}

/// Withdraws `shares` of `pool`, sent once.
#[cfg(feature = "sign")]
pub async fn withdraw<H: Http>(client: &Client<H, ApiKey>, pool: AccountIndex, shares: u64) -> Result<Sent, SendError> {
    client.send(&BurnShares::new(pool, shares)).await
}

/// Withdraws every share the client's account holds in `pool`, as `account` lists them now.
/// Refused before anything is signed when it holds none.
#[cfg(feature = "sign")]
pub async fn withdraw_all<H: Http>(client: &Client<H, ApiKey>, pool: AccountIndex) -> Result<Sent, SendError> {
    match shares_in(client, client.key().account(), pool).await? {
        0 => Err(refused("ShareAmount", format!("the account holds no shares of pool {pool}"))),
        shares => withdraw(client, pool, shares).await,
    }
}

#[cfg(test)]
#[path = "tests/shares.rs"]
mod tests;
