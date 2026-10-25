//! Reading pools: `publicPoolsMetadata` (each pool's value, shares and yield, a page at a time),
//! a pool's detail and an account's shares (`account`, which a pool's index answers like any
//! other account's), and what a share is worth.
//!
//! `publicPoolsMetadata` pages down from an exclusive cursor: `index` 2^48 - 1 starts at the LLP,
//! and `index = pool + 1, limit = 1` is exactly one pool, or the next one down when that one is not
//! listed (the staking pool is listed under `filter=stake` alone). Yields and Sharpe ratios are
//! bare JSON floats, read as the digits sent ([`Figure`]). A share is worth the pool's total asset value
//! over its total shares, in collateral; the staking pool counts the LIT it holds instead.

use serde::Deserialize;

use crate::account::{Asset, PoolShare, Position};
use crate::client::http::{ApiError, Call, Transport, codes, read};
use crate::earn::pools::operator::PoolStatus;
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, Address, AssetId, COLLATERAL_DECIMALS, Dec, Figure, LIT_DECIMALS, Round};

/// The cursor a first page starts from: one past the LLP, the highest pool index.
pub const POOLS_FROM: u64 = (1 << 48) - 1;

/// The most pools one page lists.
pub const MOST_POOLS: u8 = 100;

/// Which pools a page lists (`filter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PoolFilter {
    /// The venue's default when no filter is sent.
    #[default]
    Any,
    /// `all`
    All,
    /// `user`: pools operators run.
    User,
    /// `protocol`: the venue's own, the LLP.
    Protocol,
    /// `account_index`: pools tied to the account the page names ([`PoolsPage::holder`]); the
    /// docs do not say more.
    Account,
    /// `stake`: staking pools, which a listing without it skips.
    Stake,
}

impl PoolFilter {
    /// The word `filter` takes; `None` for [`PoolFilter::Any`], which sends none.
    #[must_use]
    pub const fn word(self) -> Option<&'static str> {
        match self {
            PoolFilter::Any => None,
            PoolFilter::All => Some("all"),
            PoolFilter::User => Some("user"),
            PoolFilter::Protocol => Some("protocol"),
            PoolFilter::Account => Some("account_index"),
            PoolFilter::Stake => Some("stake"),
        }
    }
}

/// One page of `publicPoolsMetadata`: pools below `before`, at most `limit` of them, highest
/// index first. With a `holder`, the read is that account's own (it carries its auth token) and
/// each pool says what the holder has in it ([`PublicPool::account_share`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolsPage {
    pub filter: PoolFilter,
    /// Exclusive: pools with a lower index.
    pub before: u64,
    /// 1 to 100.
    pub limit: u8,
    pub holder: Option<AccountIndex>,
}

impl PoolsPage {
    /// The first page of `filter`, `limit` pools long, from the LLP down.
    #[must_use]
    pub const fn first(filter: PoolFilter, limit: u8) -> PoolsPage {
        PoolsPage { filter, before: POOLS_FROM, limit, holder: None }
    }

    /// The page after one that ended at `last`.
    #[must_use]
    pub const fn after(self, last: AccountIndex) -> PoolsPage {
        PoolsPage { before: last.get(), ..self }
    }

    /// The same page as `account` reads it, with its shares in each pool.
    #[must_use]
    pub const fn holder(self, account: AccountIndex) -> PoolsPage {
        PoolsPage { holder: Some(account), ..self }
    }
}

/// What `publicPoolsMetadata` asks for a page.
#[must_use]
pub fn public_pools_call(page: &PoolsPage) -> Call {
    let call = Call::get("publicPoolsMetadata")
        .maybe("filter", page.filter.word())
        .field("index", page.before)
        .field("limit", page.limit.clamp(1, MOST_POOLS));
    match page.holder {
        Some(account) => call.field("account_index", account).authed(),
        None => call,
    }
}

/// A page of pools, highest index first.
pub async fn public_pools(transport: &impl Transport, page: &PoolsPage) -> Result<Vec<PublicPool>, ApiError> {
    Ok(read::<PublicPools>(transport, public_pools_call(page)).await?.public_pools)
}

/// What `publicPoolsMetadata` asks for one pool: `index = pool + 1, limit = 1`, filtered as
/// `filter` (the staking pool is found under [`PoolFilter::Stake`]).
#[must_use]
pub fn public_pool_call(pool: AccountIndex, filter: PoolFilter) -> Call {
    public_pools_call(&PoolsPage { filter, before: pool.get().saturating_add(1), limit: 1, holder: None })
}

/// One pool's metadata; `None` when `pool` is no pool the filter lists (the page would start at
/// the next one down, which is not answered for it).
pub async fn public_pool(
    transport: &impl Transport,
    pool: AccountIndex,
    filter: PoolFilter,
) -> Result<Option<PublicPool>, ApiError> {
    let pools = read::<PublicPools>(transport, public_pool_call(pool, filter)).await?.public_pools;
    Ok(pools.into_iter().find(|listed| listed.pool == pool))
}

#[derive(Deserialize)]
struct PublicPools {
    #[serde(default)]
    public_pools: Vec<PublicPool>,
}

/// What kind of pool, by the `account_type` the venue writes: 3 the LLP, 2 a pool an operator
/// runs, 4 a staking pool (as its answers show; the docs list no codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolKind {
    Protocol,
    Operated,
    Staking,
    Other(u8),
}

impl PoolKind {
    #[must_use]
    pub const fn from_account_type(code: u8) -> PoolKind {
        match code {
            2 => PoolKind::Operated,
            3 => PoolKind::Protocol,
            4 => PoolKind::Staking,
            other => PoolKind::Other(other),
        }
    }
}

/// One pool as `publicPoolsMetadata` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PublicPool {
    #[serde(rename = "account_index")]
    pub pool: AccountIndex,
    /// See [`PoolKind`].
    pub account_type: u8,
    /// Empty for the LLP on Robinhood Chain and for the staking pool.
    #[serde(default)]
    pub name: String,
    /// The operator's L1 address; zero for the venue's own pools.
    pub l1_address: Address,
    /// The operator's master account; 1 for the LLP.
    pub master_account_index: AccountIndex,
    /// Yearly, in percent: `14.525245980602785` is 14.5%.
    pub annual_percentage_yield: Figure,
    #[serde(default)]
    pub sharpe_ratio: Option<Figure>,
    pub status: PoolStatus,
    /// The operator's cut of depositors' profit, in percent: `5.0000` is 5%, which lighter-go
    /// signs as 50,000 millionths.
    pub operator_fee: Dec,
    /// Everything the pool holds, in collateral: what its shares are worth together. `0` for the
    /// staking pool, whose LIT sits in [`PublicPool::assets`].
    pub total_asset_value: Dec,
    #[serde(default)]
    pub total_spot_value: Option<Dec>,
    #[serde(default)]
    pub total_perps_value: Option<Dec>,
    #[serde(deserialize_with = "whole")]
    pub total_shares: u64,
    /// What the page's holder has in the pool, when the page named one.
    #[serde(default)]
    pub account_share: Option<PoolShare>,
    #[serde(default)]
    pub assets: Vec<Asset>,
    /// Unix seconds.
    #[serde(default, deserialize_with = "whole")]
    pub created_at: u64,
}

impl PublicPool {
    #[must_use]
    pub const fn kind(&self) -> PoolKind {
        PoolKind::from_account_type(self.account_type)
    }

    /// Whether a mint would be taken: a frozen pool only pays out.
    #[must_use]
    pub fn takes_deposits(&self) -> bool {
        self.status == PoolStatus::Active
    }

    /// What the pool holds of `asset`.
    #[must_use]
    pub fn holding(&self, asset: AssetId) -> Option<&Asset> {
        self.assets.iter().find(|held| held.asset_id == asset)
    }

    fn collateral(&self) -> Backing {
        Backing { value: self.total_asset_value, shares: self.total_shares }
    }

    fn lit(&self) -> Backing {
        let value = self.holding(AssetId::LIT).map_or(Dec::ZERO, |lit| lit.balance);
        Backing { value, shares: self.total_shares }
    }

    /// One share in collateral, to eighteen places, cut: total asset value over total shares.
    /// `None` for a pool with no shares.
    #[must_use]
    pub fn share_price(&self) -> Option<Dec> {
        self.collateral().price()
    }

    /// The whole shares `collateral` buys at the pool's last value, cut: what a mint of that
    /// much asks. `None` for a pool with no value or no shares.
    #[must_use]
    pub fn shares_for(&self, collateral: Dec) -> Option<u64> {
        self.collateral().shares_for(collateral)
    }

    /// What `shares` are worth at the pool's last value, in collateral cut to millionths.
    #[must_use]
    pub fn value_of(&self, shares: u64) -> Option<Dec> {
        self.collateral().value_of(shares, COLLATERAL_DECIMALS)
    }

    /// One staking share in LIT, to eighteen places, cut: the LIT the pool holds over its shares.
    #[must_use]
    pub fn lit_per_share(&self) -> Option<Dec> {
        self.lit().price()
    }

    /// The whole staking shares `lit` buys at the pool's last count, cut.
    #[must_use]
    pub fn shares_for_lit(&self, lit: Dec) -> Option<u64> {
        self.lit().shares_for(lit)
    }

    /// What `shares` of the staking pool are worth in LIT, cut to its eight places.
    #[must_use]
    pub fn lit_of(&self, shares: u64) -> Option<Dec> {
        self.lit().value_of(shares, LIT_DECIMALS)
    }
}

/// A pool's value and the shares it is split into.
struct Backing {
    value: Dec,
    shares: u64,
}

impl Backing {
    fn count(&self) -> Option<Dec> {
        (self.shares > 0).then(|| Dec::new(i128::from(self.shares), 0)).flatten()
    }

    fn price(&self) -> Option<Dec> {
        self.value.checked_div(self.count()?, 18, Round::Down)
    }

    fn shares_for(&self, amount: Dec) -> Option<u64> {
        if !self.value.is_positive() || amount.is_negative() {
            return None;
        }
        let shares = amount.checked_mul(self.count()?)?.checked_div(self.value, 0, Round::Down)?;
        u64::try_from(shares.units_at(0, Round::Down)?).ok()
    }

    fn value_of(&self, shares: u64, places: u32) -> Option<Dec> {
        let held = Dec::new(i128::from(shares), 0)?;
        held.checked_mul(self.value)?.checked_div(self.count()?, places, Round::Down)
    }
}

/// A pool's terms and record, as `account` answers for a pool's index (`pool_info`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PoolInfo {
    pub status: PoolStatus,
    /// In percent of depositors' profit, as [`PublicPool::operator_fee`].
    pub operator_fee: Dec,
    /// As the venue writes it, to four places like the fee: `0.0000` for none. No recorded pool
    /// sets one, so whether it is a percent too is unseen.
    pub min_operator_share_rate: Dec,
    #[serde(deserialize_with = "whole")]
    pub total_shares: u64,
    /// What the operator holds itself; the account's own `shares` never lists them.
    #[serde(deserialize_with = "whole")]
    pub operator_shares: u64,
    /// Yearly, in percent.
    pub annual_percentage_yield: Figure,
    #[serde(default)]
    pub sharpe_ratio: Option<Figure>,
    /// Oldest first, one a day.
    #[serde(default, deserialize_with = "crate::utils::lenient::list")]
    pub daily_returns: Vec<DailyReturn>,
    /// Oldest first, one a day; a pool opens at 0.001.
    #[serde(default, deserialize_with = "crate::utils::lenient::list")]
    pub share_prices: Vec<SharePrice>,
    /// The LLP's collateral by strategy slot, `None` for a slot no strategy fills (Robinhood
    /// Chain's LLP lists eight, two filled); empty for other pools.
    #[serde(default, deserialize_with = "crate::utils::lenient::list")]
    pub strategies: Vec<Option<Strategy>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DailyReturn {
    /// Unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    /// The day's return as a fraction: `0.0008643413478037765` is 0.086%.
    pub daily_return: Figure,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SharePrice {
    /// Unix seconds.
    #[serde(deserialize_with = "whole")]
    pub timestamp: u64,
    /// One share in collateral.
    pub share_price: Figure,
}

/// One of the LLP's strategy buckets: a segregated shard of its collateral.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Strategy {
    pub collateral: Dec,
}

/// A pool as `account` answers for its index: who runs it, what it holds and trades, and its
/// terms and record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolDetail {
    pub pool: AccountIndex,
    /// See [`PoolKind`].
    pub account_type: u8,
    pub name: String,
    pub description: String,
    /// The operator's L1 address; zero for the venue's own pools.
    pub l1_address: Address,
    /// In collateral: what the shares are worth together.
    pub total_asset_value: Dec,
    /// Unix seconds.
    pub created_at: u64,
    pub info: PoolInfo,
    /// Every perp market the pool ever touched, flat ones included.
    pub positions: Vec<Position>,
    pub assets: Vec<Asset>,
}

impl PoolDetail {
    #[must_use]
    pub const fn kind(&self) -> PoolKind {
        PoolKind::from_account_type(self.account_type)
    }

    /// One share in collateral now, to eighteen places, cut; `None` with no shares.
    #[must_use]
    pub fn share_price(&self) -> Option<Dec> {
        Backing { value: self.total_asset_value, shares: self.info.total_shares }.price()
    }
}

/// An account as `account` answers it, read for what a pool and its holders need.
#[derive(Deserialize)]
pub(crate) struct AccountRow {
    index: AccountIndex,
    #[serde(default)]
    account_type: u8,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    l1_address: Address,
    total_asset_value: Dec,
    #[serde(default, deserialize_with = "whole")]
    created_at: u64,
    #[serde(default)]
    pool_info: Option<PoolInfo>,
    #[serde(default)]
    positions: Vec<Position>,
    #[serde(default)]
    assets: Vec<Asset>,
    #[serde(default)]
    pub(crate) shares: Vec<PoolShare>,
    #[serde(default)]
    pub(crate) pending_unlocks: Vec<crate::earn::staking::PendingUnlock>,
}

#[derive(Deserialize)]
struct Accounts {
    #[serde(default)]
    accounts: Vec<AccountRow>,
}

/// What `account` asks for one index: a pool's detail, or an account's shares and unlocks.
#[must_use]
pub fn pool_call(index: AccountIndex) -> Call {
    Call::get("account").field("by", "index").field("value", index)
}

/// What `account` answers for an index no account has: `{"code":29404,"message":"not found"}`
/// with a 400.
pub use crate::client::http::codes::NOT_FOUND;

/// The account at `index`; `None` when the venue knows none (`29404`, or `21100` as the
/// account reads by address answer).
pub(crate) async fn account_row(
    transport: &impl Transport,
    index: AccountIndex,
) -> Result<Option<AccountRow>, ApiError> {
    match read::<Accounts>(transport, pool_call(index)).await {
        Ok(page) => Ok(page.accounts.into_iter().find(|row| row.index == index)),
        Err(error) if matches!(error.code(), Some(NOT_FOUND | codes::ACCOUNT_NOT_FOUND)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// A pool's detail; `None` when `pool` is no pool (an account without `pool_info`) or no account.
pub async fn pool(transport: &impl Transport, pool: AccountIndex) -> Result<Option<PoolDetail>, ApiError> {
    let Some(row) = account_row(transport, pool).await? else { return Ok(None) };
    Ok(row.pool_info.map(|info| PoolDetail {
        pool: row.index,
        account_type: row.account_type,
        name: row.name,
        description: row.description,
        l1_address: row.l1_address,
        total_asset_value: row.total_asset_value,
        created_at: row.created_at,
        info,
        positions: row.positions,
        assets: row.assets,
    }))
}

/// Every pool `account` holds shares in, the staking pool among them; none for an index no
/// account has. A pool's operator finds its own stake in [`PoolInfo::operator_shares`], never
/// here.
pub async fn pool_shares(transport: &impl Transport, account: AccountIndex) -> Result<Vec<PoolShare>, ApiError> {
    Ok(account_row(transport, account).await?.map(|row| row.shares).unwrap_or_default())
}

/// The shares `account` holds in `pool`: 0 when it holds none.
pub async fn shares_in(transport: &impl Transport, account: AccountIndex, pool: AccountIndex) -> Result<u64, ApiError> {
    let shares = pool_shares(transport, account).await?;
    Ok(shares.iter().find(|share| share.pool == pool).map_or(0, |share| share.shares))
}

#[cfg(test)]
#[path = "tests/reads.rs"]
mod tests;
