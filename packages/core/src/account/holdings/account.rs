//! The whole account as `account` answers it: what kind it is, its collateral and what is free of
//! it, its totals and cross margin requirements, a position per perp market it ever touched, its
//! binary options positions, its asset balances, its pool shares, unlocks under way, the
//! integrators it approved, its trading mode and its metadata. It is public: anyone may read any
//! account, by its index or by its owner's L1 address.
//!
//! Amounts are the collateral's (USDC on Lighter, USDG on Robinhood Chain) unless said otherwise.
//! `total_asset_value` is collateral plus unrealized PnL (and, in unified mode, the margin value of
//! the assets enabled as margin); `available_balance` is what is free for new orders and
//! withdrawals.

use serde::{Deserialize, Deserializer};

use crate::account::holdings::positions::{Asset, PoolShare, Position};
use crate::account::settings::integrators::ApprovedIntegrator;
use crate::account::settings::metadata::SubAccountMetadata;
use crate::account::settings::mode::{TradingMode, known_mode};
use crate::client::http::codes;
use crate::client::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::lenient::{blank_dec, list, text, unsigned, zero_dec};
use crate::utils::{AccountIndex, Address, AssetId, Dec, MarketId};

/// Which accounts a lookup names: one by its index, or those of an L1 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lookup {
    Index(AccountIndex),
    L1Address(Address),
}

impl Lookup {
    /// `by` as the venue takes it.
    #[must_use]
    pub const fn by(&self) -> &'static str {
        match self {
            Lookup::Index(_) => "index",
            Lookup::L1Address(_) => "l1_address",
        }
    }

    /// `value`: the index's digits, or the address in lowercase hex.
    #[must_use]
    pub fn value(&self) -> String {
        match self {
            Lookup::Index(index) => index.to_string(),
            Lookup::L1Address(address) => address.to_hex(),
        }
    }
}

impl From<AccountIndex> for Lookup {
    fn from(index: AccountIndex) -> Lookup {
        Lookup::Index(index)
    }
}

impl From<Address> for Lookup {
    fn from(address: Address) -> Lookup {
        Lookup::L1Address(address)
    }
}

/// What kind of account it is, `account_type`. The venue names only two codes; every other is a
/// pool of some kind (the LLP is 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountKind {
    /// 0: the account an address gets with its first deposit.
    Master,
    /// 1: opened by a master with `CreateSubAccount`.
    SubAccount,
    /// Any other code: a public pool, the LLP, a staking pool.
    Pool(u8),
}

impl AccountKind {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            AccountKind::Master => 0,
            AccountKind::SubAccount => 1,
            AccountKind::Pool(code) => code,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> AccountKind {
        match code {
            0 => AccountKind::Master,
            1 => AccountKind::SubAccount,
            code => AccountKind::Pool(code),
        }
    }
}

impl<'de> Deserialize<'de> for AccountKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = whole(deserializer)?;
        Ok(AccountKind::from_code(u8::try_from(code).unwrap_or(u8::MAX)))
    }
}

/// A binary options position: shares of one side of one market (Lighter only).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BoPosition {
    pub market_id: MarketId,
    /// `1` holds Up (yes), `-1` holds Down (no).
    pub sign: i8,
    /// Shares, positive.
    pub size: Dec,
    /// What they cost, in the collateral.
    #[serde(default, deserialize_with = "zero_dec")]
    pub entry_quote: Dec,
    /// The price paid for the side held: the Down price for a Down position.
    #[serde(default, deserialize_with = "zero_dec")]
    pub avg_entry_price: Dec,
}

impl BoPosition {
    /// Whether it holds Up (yes) shares.
    #[must_use]
    pub const fn is_up(&self) -> bool {
        self.sign >= 0
    }
}

/// What a staking account is owed back, and a pool account's own terms: the earn module's rows.
pub use crate::earn::pools::PoolInfo;
pub use crate::earn::staking::PendingUnlock;

/// One account, whole.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub index: AccountIndex,
    #[serde(rename = "account_type")]
    pub kind: AccountKind,
    /// The owner; the LLP's is the zero address.
    pub l1_address: Address,
    /// `None` for a code this build does not know.
    #[serde(rename = "account_trading_mode", default, deserialize_with = "known_mode")]
    pub trading_mode: Option<TradingMode>,
    /// The venue's account status code: 0 for an ordinary account.
    #[serde(default)]
    pub status: u8,
    /// Deposited and realized, before unrealized PnL.
    #[serde(deserialize_with = "zero_dec")]
    pub collateral: Dec,
    /// Free for new orders and withdrawals.
    #[serde(deserialize_with = "zero_dec")]
    pub available_balance: Dec,
    /// Collateral plus unrealized PnL (and enabled margin assets).
    #[serde(default, deserialize_with = "zero_dec")]
    pub total_asset_value: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub cross_asset_value: Dec,
    /// What the cross positions ask: to open (initial) and to stay open (maintenance).
    #[serde(default, deserialize_with = "zero_dec")]
    pub cross_initial_margin_requirement: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub cross_maintenance_margin_requirement: Dec,
    /// When a scheduled cancel-all fires, unix milliseconds; 0 for none.
    #[serde(default, deserialize_with = "unsigned")]
    pub cancel_all_time: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub total_order_count: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub total_isolated_order_count: u64,
    #[serde(default, deserialize_with = "unsigned")]
    pub pending_order_count: u64,
    /// A row per perp market the account ever set or traded, flat ones included.
    #[serde(default, deserialize_with = "list")]
    pub positions: Vec<Position>,
    #[serde(default, deserialize_with = "list")]
    pub bo_positions: Vec<BoPosition>,
    #[serde(default, deserialize_with = "list")]
    pub assets: Vec<Asset>,
    #[serde(default, deserialize_with = "list")]
    pub shares: Vec<PoolShare>,
    #[serde(default, deserialize_with = "list")]
    pub pending_unlocks: Vec<PendingUnlock>,
    #[serde(default, deserialize_with = "list")]
    pub approved_integrators: Vec<ApprovedIntegrator>,
    #[serde(default, deserialize_with = "text")]
    pub name: String,
    #[serde(default, deserialize_with = "text")]
    pub description: String,
    #[serde(default)]
    pub can_invite: bool,
    /// Percent; `None` when the venue leaves it blank.
    #[serde(default, deserialize_with = "blank_dec")]
    pub referral_points_percentage: Option<Dec>,
    #[serde(default)]
    pub can_rfq: bool,
    #[serde(default, deserialize_with = "market_ids")]
    pub can_rfq_market_ids: Vec<MarketId>,
    #[serde(default)]
    pub agent_enabled: bool,
    /// Unix seconds; 0 when the venue does not say.
    #[serde(default, deserialize_with = "unsigned")]
    pub created_at: u64,
    /// The last change, unix microseconds.
    #[serde(default, deserialize_with = "unsigned")]
    pub transaction_time: u64,
    #[serde(default)]
    pub metadata: SubAccountMetadata,
    /// A pool's terms; `None` for a trader's account.
    #[serde(default)]
    pub pool_info: Option<PoolInfo>,
}

/// The perps ledger in one place: what an app shows as the account's balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerpsBalance {
    pub collateral: Dec,
    /// Collateral plus unrealized PnL: `total_asset_value`.
    pub total: Dec,
    /// Free for new orders and withdrawals.
    pub available: Dec,
    /// `total` less `collateral`.
    pub unrealized_pnl: Dec,
    /// `total` less `available`, never below zero.
    pub margin_used: Dec,
}

impl Account {
    /// Whether it is a master account (not a sub-account or a pool).
    #[must_use]
    pub const fn is_master(&self) -> bool {
        matches!(self.kind, AccountKind::Master)
    }

    /// Whether it margins spot and perps as one.
    #[must_use]
    pub fn is_unified(&self) -> bool {
        self.trading_mode == Some(TradingMode::Unified)
    }

    /// The positions that hold something, flat rows left out.
    pub fn open_positions(&self) -> impl Iterator<Item = &Position> {
        self.positions.iter().filter(|position| position.is_open())
    }

    /// The account's row for `market`, flat or not: it carries the leverage and margin mode set
    /// there. `None` for a market the account never set nor traded.
    #[must_use]
    pub fn position(&self, market: MarketId) -> Option<&Position> {
        self.positions.iter().find(|position| position.market_id == market)
    }

    /// The position on `market`, when it holds something.
    #[must_use]
    pub fn open_position(&self, market: MarketId) -> Option<&Position> {
        self.position(market).filter(|position| position.is_open())
    }

    /// What is free for new orders and withdrawals: `available_balance`.
    #[must_use]
    pub const fn free_collateral(&self) -> Dec {
        self.available_balance
    }

    /// The perps ledger: collateral, total, available, unrealized PnL and the margin in use.
    #[must_use]
    pub fn perps_balance(&self) -> PerpsBalance {
        let total = self.total_asset_value;
        let unrealized_pnl = total.checked_sub(self.collateral).unwrap_or(Dec::ZERO);
        let margin_used = total.checked_sub(self.available_balance).filter(|used| used.is_positive());
        PerpsBalance {
            collateral: self.collateral,
            total,
            available: self.available_balance,
            unrealized_pnl,
            margin_used: margin_used.unwrap_or(Dec::ZERO),
        }
    }

    /// One asset's balance.
    #[must_use]
    pub fn asset(&self, asset: AssetId) -> Option<&Asset> {
        self.assets.iter().find(|held| held.asset_id == asset)
    }

    /// The assets held in spot: balance above zero.
    pub fn spot_assets(&self) -> impl Iterator<Item = &Asset> {
        self.assets.iter().filter(|held| held.balance.is_positive())
    }

    /// The shares held in one pool.
    #[must_use]
    pub fn pool_share(&self, pool: AccountIndex) -> Option<&PoolShare> {
        self.shares.iter().find(|share| share.pool == pool)
    }

    /// The approval `integrator` holds, if any; it may have lapsed ([`ApprovedIntegrator::is_active`]).
    #[must_use]
    pub fn integrator(&self, integrator: AccountIndex) -> Option<&ApprovedIntegrator> {
        self.approved_integrators.iter().find(|approved| approved.integrator == integrator)
    }
}

/// A page of `account`: one account by index, or an owner's accounts by L1 address.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountPage {
    #[serde(default, deserialize_with = "list")]
    pub accounts: Vec<Account>,
    #[serde(default, deserialize_with = "unsigned")]
    pub total: u64,
    /// Empty or absent on the last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// What `account` asks. `active_only` leaves out the flat rows of markets the account set or
/// traded once.
#[must_use]
pub fn account_call(lookup: Lookup, active_only: bool, cursor: Option<&str>) -> Call {
    Call::get("account")
        .field("by", lookup.by())
        .field("value", lookup.value())
        .maybe("active_only", active_only.then_some(true))
        .maybe("cursor", cursor)
}

/// A page of accounts; empty when the venue knows none (code 21100).
pub async fn accounts(
    transport: &impl Transport,
    lookup: Lookup,
    active_only: bool,
    cursor: Option<&str>,
) -> Result<AccountPage, ApiError> {
    match read::<AccountPage>(transport, account_call(lookup, active_only, cursor)).await {
        Err(error) if error.code() == Some(codes::ACCOUNT_NOT_FOUND) => {
            Ok(AccountPage { accounts: Vec::new(), total: 0, next_cursor: None })
        }
        answer => answer,
    }
}

/// One account by its index, every row; `None` when there is none (the venue's 21100).
pub async fn account(transport: &impl Transport, index: AccountIndex) -> Result<Option<Account>, ApiError> {
    let page = accounts(transport, Lookup::Index(index), false, None).await?;
    Ok(page.accounts.into_iter().find(|account| account.index == index))
}

/// Market ids as the venue lists them, strings or numbers; one that does not read is left out.
fn market_ids<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<MarketId>, D::Error> {
    let listed = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(listed
        .iter()
        .filter_map(|id| match id {
            serde_json::Value::String(text) => text.parse().ok(),
            serde_json::Value::Number(number) => number.as_u64().and_then(|id| MarketId::new(id).ok()),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
#[path = "tests/account.rs"]
mod tests;
