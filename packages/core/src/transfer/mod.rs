//! Moving money: transfers between accounts and between an account's perps and spot routes,
//! secure and fast withdrawals, deposits through the ZkLighter contract on the L1, and what the
//! venue reads back about them.
//!
//! - [`send`]: `Transfer`, between accounts and routes, with its fee and memo.
//! - [`withdraw`](mod@withdraw): `Withdraw`, a secure withdrawal to the owner's L1 address.
//! - [`fast`]: a fast withdrawal, a transfer to the venue's pool paid out on the L1 at once.
//! - [`deposit`](mod@deposit): a deposit's calldata, its plan and the whole journey with a key held here.
//! - [`erc20`]: the token calls a deposit needs.
//! - [`bridge`]: the venue's bridge: deposits seen, intent addresses, the fast bridge.
//! - [`fees`]: a transfer's fee and the secure withdrawal delay.
//! - [`history`]: deposits, transfers and withdrawals, a page at a time.
//!
//! Amounts: a transfer or a withdrawal is signed in the venue's decimals for the asset (6 for the
//! collateral, `assetDetails` for the rest); a deposit is counted in the token's L1 decimals.
//! Both are exact: a finer amount is refused, never rounded.

pub mod bridge;
pub mod deposit;
pub mod erc20;
pub mod fast;
pub mod fees;
pub mod history;
pub mod send;
pub mod withdraw;

pub use bridge::{DepositNetwork, IntentAddress, LatestDeposit};
#[cfg(feature = "sign")]
pub use deposit::deposit;
pub use deposit::{BadDeposit, DepositError, DepositPlan, Deposited, MINIMUM_DEPOSIT, plan_deposit};
#[cfg(feature = "sign")]
pub use fast::fast_withdraw;
pub use fast::{FastWithdrawInfo, FastWithdrawn, fast_withdrawal};
pub use fees::{transfer_fee, withdrawal_delay};
pub use history::{
    DepositRow, DepositStatus, HistoryFilter, Page, TransferFilter, TransferKind, TransferRow, WithdrawalKind,
    WithdrawalRow, WithdrawalStatus,
};
pub use send::{MAX_AMOUNT, Memo, Transfer};
#[cfg(feature = "sign")]
pub use send::{transfer, transfer_to_owner};
pub use withdraw::Withdraw;
#[cfg(feature = "sign")]
pub use withdraw::withdraw;
