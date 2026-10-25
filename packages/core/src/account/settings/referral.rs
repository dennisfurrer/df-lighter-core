//! Referrals: the account's own code (`referral/get`, `referral/create`, renamed once with
//! `referral/update`), the code it was referred by (`referral/use`), what its referrals traded
//! (`referral/userReferrals`, `referral/stats`), the kickback it hands them
//! (`referral/kickback/update`, once a day), its referral points (`referral/points`, Robinhood
//! Chain only) and the L1 address's referral terms (`l1Metadata`). Every one is the owner's own and
//! carries the auth token.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::lenient::{Done, blank_dec, list, text, zero_dec};
use crate::utils::{AccountIndex, Address, Dec, Figure};

/// What a set of referred trades added up to.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct TradeStats {
    #[serde(default, deserialize_with = "whole")]
    pub count: u64,
    /// Notional, in the collateral.
    #[serde(default, deserialize_with = "zero_dec")]
    pub volume: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub web_count: u64,
    #[serde(default, deserialize_with = "zero_dec")]
    pub web_volume: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub mobile_app_count: u64,
    #[serde(default, deserialize_with = "zero_dec")]
    pub mobile_app_volume: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub mobile_browser_count: u64,
    #[serde(default, deserialize_with = "zero_dec")]
    pub mobile_browser_volume: Dec,
    /// Volume on a fee-free tier, and the rest.
    #[serde(default, deserialize_with = "zero_dec")]
    pub free_maker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub free_taker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub non_free_maker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub non_free_taker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub maker_fees_paid: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub taker_fees_paid: Dec,
}

/// One account the owner referred.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Referral {
    pub l1_address: Address,
    #[serde(default, deserialize_with = "text")]
    pub referral_code: String,
    /// When the code was used, unix seconds as the venue writes it.
    #[serde(default, deserialize_with = "whole")]
    pub used_at: u64,
    #[serde(default)]
    pub trade_stats: TradeStats,
    /// Its account tier's word.
    #[serde(default, deserialize_with = "text")]
    pub tier: String,
    #[serde(default, deserialize_with = "text")]
    pub source: String,
}

/// The sums over every referral.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ReferralTotals {
    #[serde(default, deserialize_with = "whole")]
    pub count: u64,
    #[serde(default, deserialize_with = "zero_dec")]
    pub volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub free_maker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub free_taker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub non_free_maker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub non_free_taker_volume: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub maker_fees_paid: Dec,
    #[serde(default, deserialize_with = "zero_dec")]
    pub taker_fees_paid: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub total_referrals: u64,
    #[serde(default, deserialize_with = "whole")]
    pub total_premium_referrals: u64,
}

/// What `referral/userReferrals` answers: a page of referrals, the code the owner itself used.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UserReferrals {
    #[serde(default, deserialize_with = "list")]
    pub referrals: Vec<Referral>,
    /// The next page's cursor; empty on the last.
    #[serde(default, deserialize_with = "text")]
    pub cursor: String,
    #[serde(default, deserialize_with = "text")]
    pub used_code: String,
    #[serde(default)]
    pub totals: ReferralTotals,
}

/// One window of `referral/stats`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReferralStat {
    /// Unix milliseconds as the venue writes them.
    #[serde(default, deserialize_with = "whole")]
    pub start_timestamp: u64,
    #[serde(default, deserialize_with = "whole")]
    pub end_timestamp: u64,
    #[serde(default)]
    pub trade_stats: TradeStats,
}

/// One referral's points (Robinhood Chain). Points are the venue's floats, kept as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReferralPointEntry {
    pub l1_address: Address,
    pub total_points: Figure,
    pub week_points: Figure,
    pub total_reward_points: Figure,
    pub week_reward_points: Figure,
    #[serde(default, deserialize_with = "text")]
    pub reward_point_multiplier: String,
}

/// What `referral/points` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReferralPoints {
    #[serde(default, deserialize_with = "list")]
    pub referrals: Vec<ReferralPointEntry>,
    pub user_total_points: Figure,
    pub user_last_week_points: Figure,
    pub user_total_referral_reward_points: Figure,
    pub user_last_week_referral_reward_points: Figure,
    #[serde(default, deserialize_with = "text")]
    pub reward_point_multiplier: String,
}

/// A kickback setting from its history.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Kickback {
    /// As the venue writes it.
    #[serde(default, deserialize_with = "whole")]
    pub date: u64,
    /// Percent of the referrer's rewards handed to its referrals.
    #[serde(default, deserialize_with = "blank_dec")]
    pub percentage: Option<Dec>,
}

/// What `l1Metadata` answers: the owner's referral terms.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct L1Metadata {
    pub l1_address: Address,
    #[serde(default)]
    pub can_invite: bool,
    /// Percent; `None` when the venue leaves it blank.
    #[serde(default, deserialize_with = "blank_dec")]
    pub referral_points_percentage: Option<Dec>,
    #[serde(default, deserialize_with = "blank_dec")]
    pub referral_program_kickback: Option<Dec>,
    #[serde(default, deserialize_with = "list")]
    pub referral_program_kickback_history: Vec<Kickback>,
}

/// What `referral/use` posts: the owner, the code, and what the venue's form also takes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UseReferral {
    pub l1_address: Address,
    pub referral_code: String,
    pub discord: Option<String>,
    pub telegram: Option<String>,
    pub x: Option<String>,
    pub signature: Option<String>,
    pub source: Option<String>,
}

#[derive(Deserialize)]
struct Code {
    #[serde(default, deserialize_with = "text")]
    referral_code: String,
}

#[derive(Deserialize)]
struct Success {
    #[serde(default)]
    success: bool,
}

#[derive(Deserialize)]
struct Stats {
    #[serde(default, deserialize_with = "list")]
    stats: Vec<ReferralStat>,
}

/// What `referral/get` asks.
#[must_use]
pub fn referral_code_call(account: AccountIndex) -> Call {
    Call::get("referral/get").field("account_index", account).authed()
}

/// The account's own referral code; empty when it has none.
pub async fn referral_code(transport: &impl Transport, account: AccountIndex) -> Result<String, ApiError> {
    Ok(read::<Code>(transport, referral_code_call(account)).await?.referral_code)
}

/// What `referral/create` posts.
#[must_use]
pub fn create_referral_code_call(account: AccountIndex) -> Call {
    Call::post("referral/create").field("account_index", account).authed()
}

/// Makes the account a referral code and gives it. Weight 3,000.
pub async fn create_referral_code(transport: &impl Transport, account: AccountIndex) -> Result<String, ApiError> {
    Ok(read::<Code>(transport, create_referral_code_call(account)).await?.referral_code)
}

/// What `referral/update` posts.
#[must_use]
pub fn update_referral_code_call(account: AccountIndex, code: &str) -> Call {
    Call::post("referral/update").field("account_index", account).field("new_referral_code", code).authed()
}

/// Renames the account's code, once ever. Whether the venue says it did.
pub async fn update_referral_code(
    transport: &impl Transport,
    account: AccountIndex,
    code: &str,
) -> Result<bool, ApiError> {
    Ok(read::<Success>(transport, update_referral_code_call(account, code)).await?.success)
}

/// What `referral/kickback/update` posts: `percentage` of the referrer's rewards, as a decimal.
#[must_use]
pub fn update_kickback_call(account: AccountIndex, percentage: Dec) -> Call {
    Call::post("referral/kickback/update")
        .field("account_index", account)
        .field("kickback_percentage", percentage.to_wire())
        .authed()
}

/// Sets the kickback, once a day. Whether the venue says it did.
pub async fn update_kickback(
    transport: &impl Transport,
    account: AccountIndex,
    percentage: Dec,
) -> Result<bool, ApiError> {
    Ok(read::<Success>(transport, update_kickback_call(account, percentage)).await?.success)
}

/// What `referral/use` posts; the optional fields only when given.
#[must_use]
pub fn use_referral_code_call(form: &UseReferral) -> Call {
    Call::post("referral/use")
        .field("l1_address", form.l1_address.to_hex())
        .field("referral_code", &form.referral_code)
        .maybe("discord", form.discord.as_deref())
        .maybe("telegram", form.telegram.as_deref())
        .maybe("x", form.x.as_deref())
        .maybe("signature", form.signature.as_deref())
        .maybe("source", form.source.as_deref())
        .authed()
}

/// Sets the code the owner was referred by; it may be changed later. Weight 3,000.
pub async fn use_referral_code(transport: &impl Transport, form: &UseReferral) -> Result<(), ApiError> {
    read::<Done>(transport, use_referral_code_call(form)).await.map(drop)
}

/// What `referral/userReferrals` asks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReferralQuery {
    pub cursor: Option<String>,
    /// Stats from and to, as the venue takes them.
    pub stats_from: Option<u64>,
    pub stats_to: Option<u64>,
    /// 1 to 300; the venue's default when `None`.
    pub limit: Option<u16>,
    /// Only referrals eligible for rewards.
    pub eligible_only: bool,
}

/// What `referral/userReferrals` asks for `owner`.
#[must_use]
pub fn user_referrals_call(owner: Address, query: &ReferralQuery) -> Call {
    Call::get("referral/userReferrals")
        .field("l1_address", owner.to_hex())
        .maybe("cursor", query.cursor.as_deref())
        .maybe("stats_start_timestamp", query.stats_from)
        .maybe("stats_end_timestamp", query.stats_to)
        .maybe("limit", query.limit.map(|limit| limit.clamp(1, 300)))
        .maybe("is_eligible", query.eligible_only.then_some(true))
        .authed()
}

pub async fn user_referrals(
    transport: &impl Transport,
    owner: Address,
    query: &ReferralQuery,
) -> Result<UserReferrals, ApiError> {
    read(transport, user_referrals_call(owner, query)).await
}

/// What `referral/stats` asks: trade stats summed over `owner`'s referrals.
#[must_use]
pub fn referral_stats_call(owner: Address, eligible_only: bool) -> Call {
    Call::get("referral/stats")
        .field("l1_address", owner.to_hex())
        .maybe("is_eligible", eligible_only.then_some(true))
        .authed()
}

pub async fn referral_stats(
    transport: &impl Transport,
    owner: Address,
    eligible_only: bool,
) -> Result<Vec<ReferralStat>, ApiError> {
    Ok(read::<Stats>(transport, referral_stats_call(owner, eligible_only)).await?.stats)
}

/// What `referral/points` asks (Robinhood Chain only).
#[must_use]
pub fn referral_points_call(account: AccountIndex) -> Call {
    Call::get("referral/points").field("account_index", account).authed()
}

pub async fn referral_points(transport: &impl Transport, account: AccountIndex) -> Result<ReferralPoints, ApiError> {
    read(transport, referral_points_call(account)).await
}

/// What `l1Metadata` asks.
#[must_use]
pub fn l1_metadata_call(owner: Address) -> Call {
    Call::get("l1Metadata").field("l1_address", owner.to_hex()).authed()
}

pub async fn l1_metadata(transport: &impl Transport, owner: Address) -> Result<L1Metadata, ApiError> {
    read(transport, l1_metadata_call(owner)).await
}

#[cfg(test)]
#[path = "tests/referral.rs"]
mod tests;
