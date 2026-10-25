//! What an account holds.
//!
//! - [`account`](mod@account): the whole account (`account`, by index or by owner): balances,
//!   totals, margin requirements, positions, binary options positions, assets, pool shares,
//!   unlocks, approved integrators, and the perps balance an app shows.
//! - [`owned`]: the accounts an L1 address owns (`accountsByL1Address`) and its master.
//! - [`positions`]: perp positions, asset balances, public pool shares.
//! - [`margin`]: what a position implies: leverage, margin, entry and liquidation prices, and the
//!   leverage and margin mode set on a market.

pub mod account;

pub mod margin;
pub mod owned;
pub mod positions;

pub use account::{
    Account, AccountKind, AccountPage, BoPosition, Lookup, PendingUnlock, PerpsBalance, PoolInfo, account,
    account_call, accounts,
};
pub use margin::{BPS, Setting, imf_bps, leverage_of};
pub use owned::{OwnedAccount, OwnedAccounts, accounts_by_l1_address, accounts_by_l1_address_call, master_account};
pub use positions::{Asset, PoolShare, Position};
