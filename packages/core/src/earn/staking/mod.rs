//! LIT staking: one pool on Lighter whose shares are bought with LIT and earn its rewards, and
//! which the LLP, Premium fee tiers and binary options fee discounts all count.
//!
//! - [`actions`]: `StakeAssets` and `UnstakeAssets`, the senders that size them, the pool's index
//!   and the lockup.
//! - [`reads`]: the staking pool, an account's staked shares and its pending unlocks.

pub mod actions;
pub mod reads;

pub use actions::{LIGHTER_STAKING_POOL, STAKING_LOCKUP_MS, StakeAssets, UnstakeAssets, staking_pool_index};
#[cfg(feature = "sign")]
pub use actions::{stake, unstake};
pub use reads::{PendingUnlock, pending_unlocks, staked_shares, staking_pool};
