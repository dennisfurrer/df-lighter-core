//! The account's tier and what it allows: `accountLimits` (the tier, the fee rates it trades at,
//! staked LIT, the LLP cap) and `changeAccountTier`. A tier belongs to the L1 address, master and
//! sub-accounts alike. An upgrade lands at once; a downgrade waits 24 hours from the last change
//! and needs the sub-account count to fit the lower tier.
//!
//! `accountLimits` answers only with an auth token signed by a key registered for the account, so
//! a `code: 200` from it is also the venue's word that a key works.

use serde::Deserialize;

use crate::client::{ApiError, Call, Tier, Transport, read};
use crate::utils::ids::whole;
use crate::utils::lenient::{Done, blank_dec, text};
use crate::utils::{AccountIndex, Dec, Figure};

/// What `accountLimits` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountLimits {
    /// The tier's word as the venue writes it; [`AccountLimits::tier`] reads it.
    #[serde(default, deserialize_with = "text")]
    pub user_tier: String,
    /// The fee tier's display name.
    #[serde(default, deserialize_with = "text")]
    pub user_tier_name: String,
    /// When the tier last changed, unix milliseconds; 0 when the venue does not say.
    #[serde(default, deserialize_with = "whole")]
    pub user_tier_last_update: u64,
    /// The account's own fee rates, millionths of notional: 40 is 0.004%.
    #[serde(default)]
    pub current_maker_fee_tick: u32,
    #[serde(default)]
    pub current_taker_fee_tick: u32,
    /// Staked LIT counted for fees, leases included.
    #[serde(default, deserialize_with = "blank_dec")]
    pub effective_lit_stakes: Option<Dec>,
    /// LIT leased and still active.
    #[serde(default, deserialize_with = "blank_dec")]
    pub leased_lit: Option<Dec>,
    #[serde(default)]
    pub can_create_public_pool: bool,
    /// The most the account may hold of the LLP, in percent and in the collateral.
    #[serde(default)]
    pub max_llp_percentage: u32,
    #[serde(default, deserialize_with = "blank_dec")]
    pub max_llp_amount: Option<Dec>,
    /// Binary options: the volume tier's name and the fee multiplier (`0.72` is 28% off), Lighter
    /// only.
    #[serde(default, deserialize_with = "text")]
    pub bo_user_tier_name: String,
    #[serde(default)]
    pub bo_fee_rate: Option<Figure>,
}

impl AccountLimits {
    /// The tier, when its word is one of the four this crate knows (any case).
    #[must_use]
    pub fn tier(&self) -> Option<Tier> {
        tier_of(&self.user_tier)
    }
}

/// A tier from the venue's word: `standard` (or `std`), `plus`, `premium` (or `advanced`),
/// `builder`, in any case.
#[must_use]
pub fn tier_of(word: &str) -> Option<Tier> {
    match word.trim().to_ascii_lowercase().as_str() {
        "standard" | "std" => Some(Tier::Standard),
        "plus" => Some(Tier::Plus),
        "premium" | "advanced" => Some(Tier::Premium),
        "builder" => Some(Tier::Builder),
        _ => None,
    }
}

/// What `accountLimits` asks: the account's own.
#[must_use]
pub fn account_limits_call(account: AccountIndex) -> Call {
    Call::get("accountLimits").field("account_index", account).authed()
}

pub async fn account_limits(transport: &impl Transport, account: AccountIndex) -> Result<AccountLimits, ApiError> {
    read(transport, account_limits_call(account)).await
}

/// What `changeAccountTier` posts: the account and the tier's name ([`Tier::name`]). Weight 3,000.
#[must_use]
pub fn change_account_tier_call(account: AccountIndex, tier: Tier) -> Call {
    Call::post("changeAccountTier").field("account_index", account).field("new_tier", tier.name()).authed()
}

/// Moves the owner's accounts to `tier`. Give the client the new tier
/// ([`crate::Client::with_tier`]) once it lands, so reads are paced against its budget.
pub async fn change_account_tier(
    transport: &impl Transport,
    account: AccountIndex,
    tier: Tier,
) -> Result<(), ApiError> {
    read::<Done>(transport, change_account_tier_call(account, tier)).await.map(drop)
}

#[cfg(test)]
#[path = "tests/tier.rs"]
mod tests;
