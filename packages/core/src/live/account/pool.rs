//! `pool_data`: a pool's trades, orders, positions and shares. Its terms and returns
//! (`pool_info`) are the [`crate::earn::pools::PoolInfo`] the pool reads answer. A public pool's
//! need no auth token; another account's pool, its own.

use crate::account::history::PositionFunding;
use crate::account::{Order, PoolShare, Position};
use crate::markets::Trade;
use crate::utils::AccountIndex;

/// `pool_data`: what a pool's account does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolData {
    /// The pool's account.
    pub account: AccountIndex,
    pub trades: Vec<Trade>,
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub shares: Vec<PoolShare>,
    pub fundings: Vec<PositionFunding>,
}
