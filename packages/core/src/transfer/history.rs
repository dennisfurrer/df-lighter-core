//! An account's money movements, a page at a time: deposits (`deposit/history`), transfers
//! (`transfer/history`) and withdrawals (`withdraw/history`). Each is the account's own read,
//! but a public pool's transfers, which anyone may read. A page carries the cursor the next one
//! is asked with; the last carries none.
//!
//! Amounts are decimal strings in whole units of the asset (`1299.997816` USDC). A transfer's
//! `timestamp` is unix milliseconds, as recorded; the deposit and withdrawal histories, which
//! need the owner's token, are read the same.

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, Address, AssetId, Dec, Route};

/// One page of rows, and the cursor of the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub rows: Vec<T>,
    /// What the next page is asked with; `None` on the last.
    pub cursor: Option<String>,
}

/// Which deposits or withdrawals a page lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HistoryFilter {
    #[default]
    All,
    Pending,
    /// Proved on the L1 and waiting to be claimed.
    Claimable,
}

impl HistoryFilter {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            HistoryFilter::All => "all",
            HistoryFilter::Pending => "pending",
            HistoryFilter::Claimable => "claimable",
        }
    }
}

/// Where a deposit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositStatus {
    Pending,
    Completed,
    Claimable,
    Failed,
    /// A word this build does not know.
    #[serde(other)]
    Other,
}

/// One deposit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DepositRow {
    pub id: String,
    #[serde(rename = "asset_id")]
    pub asset: AssetId,
    /// Whole units of the asset.
    pub amount: Dec,
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    pub status: DepositStatus,
    /// The deposit's transaction on the L1.
    #[serde(default)]
    pub l1_tx_hash: String,
}

/// Where a withdrawal is: a secure one is `claimable` once proved, `completed` only a fast one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalStatus {
    Pending,
    Claimable,
    Completed,
    Refunded,
    Failed,
    /// A word this build does not know.
    #[serde(other)]
    Other,
}

/// How a withdrawal pays out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalKind {
    /// Proved on the L1, to the owner.
    Secure,
    /// Out of the pool, to any address, at once.
    Fast,
    /// A word this build does not know.
    #[serde(other)]
    Other,
}

/// One withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WithdrawalRow {
    pub id: String,
    #[serde(rename = "asset_id")]
    pub asset: AssetId,
    /// Whole units of the asset.
    pub amount: Dec,
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    pub status: WithdrawalStatus,
    #[serde(rename = "type")]
    pub kind: WithdrawalKind,
    /// The payout's transaction on the L1, once there is one.
    #[serde(default)]
    pub l1_tx_hash: String,
}

/// What a transfer row records, from the account's side: money in (`Inflow`) or out
/// (`Outflow`), and what moved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum TransferKind {
    L2TransferInflow,
    L2TransferOutflow,
    /// Between an account's own routes.
    L2SelfTransfer,
    L2BurnSharesInflow,
    L2BurnSharesOutflow,
    L2MintSharesInflow,
    L2MintSharesOutflow,
    L2ForceBurnSharesInflow,
    L2ForceBurnSharesOutflow,
    L2StakeAssetInflow,
    L2StakeAssetOutflow,
    L2UnstakeAssetInflow,
    L2UnstakeAssetOutflow,
    L1BurnSharesInflow,
    L1BurnSharesOutflow,
    L1UnstakeAssetInflow,
    L1UnstakeAssetOutflow,
    L2CreatePublicPoolInflow,
    L2CreatePublicPoolOutflow,
    L2CreateStakingPoolInflow,
    L2CreateStakingPoolOutflow,
    /// A word this build does not know.
    #[serde(other)]
    Other,
}

/// One transfer, or one pool or staking movement the venue lists with them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TransferRow {
    pub id: String,
    #[serde(rename = "asset_id")]
    pub asset: AssetId,
    /// Whole units of the asset.
    pub amount: Dec,
    /// The collateral, whole units.
    pub fee: Dec,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    #[serde(rename = "type")]
    pub kind: TransferKind,
    /// The zero address for a pool's own side.
    pub from_l1_address: Address,
    pub to_l1_address: Address,
    #[serde(rename = "from_account_index")]
    pub from_account: AccountIndex,
    #[serde(rename = "to_account_index")]
    pub to_account: AccountIndex,
    /// `None` for a route this build does not know.
    #[serde(deserialize_with = "route")]
    pub from_route: Option<Route>,
    #[serde(deserialize_with = "route")]
    pub to_route: Option<Route>,
    pub tx_hash: String,
}

/// `perps` or `spot`, leniently.
fn route<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Route>, D::Error> {
    Ok(match Option::<String>::deserialize(deserializer)?.as_deref() {
        Some("perps") => Some(Route::Perps),
        Some("spot") => Some(Route::Spot),
        _ => None,
    })
}

/// Which movements a transfer page lists, by the venue's word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransferFilter {
    #[default]
    All,
    L2Transfer,
    L2MintShares,
    L2BurnShares,
    L2StakeAssets,
    L2UnstakeAssets,
    L2CreatePublicPool,
    L2CreateStakingPool,
    L1BurnShares,
    L1UnstakeAssets,
    ReferralProgramPayout,
}

impl TransferFilter {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            TransferFilter::All => "all",
            TransferFilter::L2Transfer => "L2Transfer",
            TransferFilter::L2MintShares => "L2MintShares",
            TransferFilter::L2BurnShares => "L2BurnShares",
            TransferFilter::L2StakeAssets => "L2StakeAssets",
            TransferFilter::L2UnstakeAssets => "L2UnstakeAssets",
            TransferFilter::L2CreatePublicPool => "L2CreatePublicPool",
            TransferFilter::L2CreateStakingPool => "L2CreateStakingPool",
            TransferFilter::L1BurnShares => "L1BurnShares",
            TransferFilter::L1UnstakeAssets => "L1UnstakeAssets",
            TransferFilter::ReferralProgramPayout => "ReferralProgramPayout",
        }
    }
}

#[derive(Deserialize)]
struct Deposits {
    #[serde(default)]
    deposits: Vec<DepositRow>,
    #[serde(default)]
    cursor: String,
}

#[derive(Deserialize)]
struct Withdrawals {
    #[serde(default)]
    withdraws: Vec<WithdrawalRow>,
    #[serde(default)]
    cursor: String,
}

#[derive(Deserialize)]
struct Transfers {
    #[serde(default)]
    transfers: Vec<TransferRow>,
    #[serde(default)]
    cursor: String,
}

fn page<T>(rows: Vec<T>, cursor: String) -> Page<T> {
    let cursor = (!cursor.is_empty() && !rows.is_empty()).then_some(cursor);
    Page { rows, cursor }
}

/// What `deposit/history` asks: `account`'s deposits from its owner `l1_address`.
#[must_use]
pub fn deposit_history_call(
    account: AccountIndex,
    l1_address: Address,
    filter: HistoryFilter,
    cursor: Option<&str>,
) -> Call {
    Call::get("deposit/history")
        .field("account_index", account)
        .field("l1_address", l1_address)
        .field("filter", filter.name())
        .maybe("cursor", cursor)
        .authed()
}

/// One page of `account`'s deposits, the latest first.
pub async fn deposit_history(
    transport: &impl Transport,
    account: AccountIndex,
    l1_address: Address,
    filter: HistoryFilter,
    cursor: Option<&str>,
) -> Result<Page<DepositRow>, ApiError> {
    let answer: Deposits = read(transport, deposit_history_call(account, l1_address, filter, cursor)).await?;
    Ok(page(answer.deposits, answer.cursor))
}

/// What `withdraw/history` asks.
#[must_use]
pub fn withdraw_history_call(account: AccountIndex, filter: HistoryFilter, cursor: Option<&str>) -> Call {
    Call::get("withdraw/history")
        .field("account_index", account)
        .field("filter", filter.name())
        .maybe("cursor", cursor)
        .authed()
}

/// One page of `account`'s withdrawals, secure and fast, the latest first.
pub async fn withdraw_history(
    transport: &impl Transport,
    account: AccountIndex,
    filter: HistoryFilter,
    cursor: Option<&str>,
) -> Result<Page<WithdrawalRow>, ApiError> {
    let answer: Withdrawals = read(transport, withdraw_history_call(account, filter, cursor)).await?;
    Ok(page(answer.withdraws, answer.cursor))
}

/// What `transfer/history` asks: the account's own read. A public pool's history needs no
/// token: clear `auth` on the call to read it without one.
#[must_use]
pub fn transfer_history_call(account: AccountIndex, filter: TransferFilter, cursor: Option<&str>) -> Call {
    let call = Call::get("transfer/history").field("account_index", account);
    let call = match filter {
        TransferFilter::All => call,
        other => call.field("type", other.name()),
    };
    call.maybe("cursor", cursor).authed()
}

/// One page of `account`'s transfers and pool movements, the latest first.
pub async fn transfer_history(
    transport: &impl Transport,
    account: AccountIndex,
    filter: TransferFilter,
    cursor: Option<&str>,
) -> Result<Page<TransferRow>, ApiError> {
    let answer: Transfers = read(transport, transfer_history_call(account, filter, cursor)).await?;
    Ok(page(answer.transfers, answer.cursor))
}

#[cfg(test)]
#[path = "tests/history.rs"]
mod tests;
