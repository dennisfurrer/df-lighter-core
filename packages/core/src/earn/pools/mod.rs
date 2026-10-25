//! Public pools: sub-accounts an operator trades for depositors, who hold the pool's shares. The
//! LLP ([`crate::utils::AccountIndex::LLP`], 2^48 - 2 on both venues) is the venue's own, which
//! market-makes and takes over liquidations.
//!
//! - [`operator`]: `CreatePublicPool` and `UpdatePublicPool`, the operator's terms and status.
//! - [`shares`]: `MintShares` and `BurnShares`, a deposit and a withdrawal, and the senders that
//!   size them.
//! - [`reads`]: `publicPoolsMetadata` a page at a time, a pool's detail and record (`account`),
//!   an account's shares, and what a share is worth.

pub mod operator;
pub mod reads;
pub mod shares;

pub use operator::{
    CreatePublicPool, INITIAL_SHARE_PRICE, MAX_INITIAL_TOTAL_SHARES, MIN_INITIAL_TOTAL_SHARES, PoolStatus, SHARE_TICK,
    UpdatePublicPool, min_operator_share_units, operator_fee_units,
};
pub use reads::{
    DailyReturn, MOST_POOLS, NOT_FOUND, POOLS_FROM, PoolDetail, PoolFilter, PoolInfo, PoolKind, PoolsPage, PublicPool,
    SharePrice, Strategy, pool, pool_call, pool_shares, public_pool, public_pool_call, public_pools, public_pools_call,
    shares_in,
};
pub use shares::{BurnShares, MOST_SHARES, MintShares, llp_cooldown_ms};
#[cfg(feature = "sign")]
pub use shares::{deposit, withdraw, withdraw_all};
