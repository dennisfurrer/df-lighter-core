//! The accounts an L1 address owns: `accountsByL1Address`, its master and its sub-accounts (and
//! the pools it operates), each in brief. Public. An address that never deposited owns none, which
//! the venue answers as an error (code 21100) and this reads as an empty list.
//!
//! The master is the one that trades by default and the one a deposit credits: the lowest index of
//! kind master, or the lowest index of all when no row says master ([`OwnedAccounts::master`]). Its
//! collateral tells whether the first deposit has landed. Keeping the answer (an absent account
//! does not appear until a deposit lands) is the caller's business.

use serde::Deserialize;

use crate::account::holdings::account::AccountKind;
use crate::account::settings::mode::{TradingMode, known_mode};
use crate::client::http::codes;
use crate::client::{ApiError, Call, Transport, read};
use crate::utils::lenient::{blank_dec, list, unsigned, zero_dec};
use crate::utils::{AccountIndex, Address, Dec};

/// One account an address owns, in brief: no positions or balances (read the account for those).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OwnedAccount {
    pub index: AccountIndex,
    #[serde(rename = "account_type")]
    pub kind: AccountKind,
    pub l1_address: Address,
    #[serde(rename = "account_trading_mode", default, deserialize_with = "known_mode")]
    pub trading_mode: Option<TradingMode>,
    #[serde(default)]
    pub status: u8,
    /// Deposited and realized, in the collateral.
    #[serde(default, deserialize_with = "zero_dec")]
    pub collateral: Dec,
    /// The venue leaves it blank in this list; `None` then.
    #[serde(default, deserialize_with = "blank_dec")]
    pub available_balance: Option<Dec>,
    #[serde(default, deserialize_with = "unsigned")]
    pub cancel_all_time: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub total_order_count: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub total_isolated_order_count: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub pending_order_count: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub transaction_time: u64,
}

impl OwnedAccount {
    #[must_use]
    pub const fn is_master(&self) -> bool {
        matches!(self.kind, AccountKind::Master)
    }

    /// Whether money has landed in it: collateral above zero.
    #[must_use]
    pub const fn is_funded(&self) -> bool {
        self.collateral.is_positive()
    }
}

/// What `accountsByL1Address` answers: one page of the owner's accounts.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct OwnedAccounts {
    /// As the venue echoes it; `None` on the empty answer.
    #[serde(default)]
    pub l1_address: Option<Address>,
    #[serde(rename = "sub_accounts", default, deserialize_with = "list")]
    pub accounts: Vec<OwnedAccount>,
    /// Empty or absent on the last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl OwnedAccounts {
    /// The master: the lowest index of kind master, else the lowest index; `None` when the owner
    /// has no account.
    #[must_use]
    pub fn master(&self) -> Option<&OwnedAccount> {
        let master = self.accounts.iter().filter(|account| account.is_master()).min_by_key(|account| account.index);
        master.or_else(|| self.accounts.iter().min_by_key(|account| account.index))
    }

    /// Every account but the master: sub-accounts and pools.
    pub fn sub_accounts(&self) -> impl Iterator<Item = &OwnedAccount> {
        let master = self.master().map(|master| master.index);
        self.accounts.iter().filter(move |account| Some(account.index) != master)
    }

    /// Whether the owner has no account here yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

/// What `accountsByL1Address` asks.
#[must_use]
pub fn accounts_by_l1_address_call(owner: Address, cursor: Option<&str>) -> Call {
    Call::get("accountsByL1Address").field("l1_address", owner.to_hex()).maybe("cursor", cursor)
}

/// A page of `owner`'s accounts; empty, not an error, when it has none (code 21100).
pub async fn accounts_by_l1_address(
    transport: &impl Transport,
    owner: Address,
    cursor: Option<&str>,
) -> Result<OwnedAccounts, ApiError> {
    match read::<OwnedAccounts>(transport, accounts_by_l1_address_call(owner, cursor)).await {
        Err(error) if error.code() == Some(codes::ACCOUNT_NOT_FOUND) => {
            Ok(OwnedAccounts { l1_address: Some(owner), ..OwnedAccounts::default() })
        }
        answer => answer,
    }
}

/// `owner`'s master account, from the first page; `None` when it has no account yet (no deposit
/// has landed). Its [`OwnedAccount::collateral`] says whether one has been funded.
pub async fn master_account(transport: &impl Transport, owner: Address) -> Result<Option<OwnedAccount>, ApiError> {
    Ok(accounts_by_l1_address(transport, owner, None).await?.master().cloned())
}

#[cfg(test)]
#[path = "tests/owned.rs"]
mod tests;
