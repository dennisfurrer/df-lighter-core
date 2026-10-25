//! Earning on Lighter: public pools (the LLP among them), LIT staking and LIT fee credits.
//!
//! - [`pools`]: creating and running a pool, depositing and withdrawing (shares minted and
//!   burnt), and reading pools, their record and an account's shares.
//! - [`staking`]: staking and unstaking LIT, the staking pool, and the LIT still locked.
//! - [`credits`]: leasing LIT toward a fee tier.

pub mod credits;
pub mod pools;
pub mod staking;

#[cfg(test)]
#[path = "tests/support.rs"]
pub(crate) mod support;

pub use credits::{
    Lease, LeaseOption, LeaseOptions, LeaseStatus, Leases, lease_fee, lease_options, lease_options_call, lease_units,
    leases, leases_call, lit_lease_call,
};
pub use pools::{
    BurnShares, CreatePublicPool, MintShares, PoolDetail, PoolFilter, PoolInfo, PoolKind, PoolStatus, PoolsPage,
    PublicPool, UpdatePublicPool, llp_cooldown_ms, pool, pool_shares, public_pool, public_pools,
};
#[cfg(feature = "sign")]
pub use pools::{deposit, withdraw, withdraw_all};
pub use staking::{
    LIGHTER_STAKING_POOL, PendingUnlock, STAKING_LOCKUP_MS, StakeAssets, UnstakeAssets, pending_unlocks, staked_shares,
    staking_pool, staking_pool_index,
};
#[cfg(feature = "sign")]
pub use staking::{stake, unstake};
