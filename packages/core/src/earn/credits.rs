//! LIT fee credits: LIT leased for a while, counted toward a Premium account's staking tier as if
//! staked. `leaseOptions` lists the terms (a duration and its yearly rate) and the account the fee
//! is paid to; `leases` an account's leases, newest first; `litLease` takes a lease with its fee
//! paid upfront, by a signed LIT transfer to that account.
//!
//! The fee is the venue's integer formula ([`lease_fee`]): `lease_amount * (annual_rate * 100)
//! * duration_days / (360 * 10000)`, in LIT's own units (1e8 a LIT), cut.

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, BadUnits, Dec, Figure, LIT_DECIMALS, Round, exact_units};

/// The most LIT units one lease counts, past which the venue's 64-bit integers do not go.
pub const MOST_LEASE_UNITS: u64 = i64::MAX.unsigned_abs();

/// The most leases one page lists.
pub const MOST_LEASES: u8 = 100;

/// One lease term.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeaseOption {
    pub duration_days: u32,
    /// Yearly, in percent: `12.5` is 12.5%.
    pub annual_rate: Figure,
}

impl LeaseOption {
    /// The fee for leasing `lease_units` (LIT in 1e8 units) on these terms; see [`lease_fee`].
    #[must_use]
    pub fn fee(&self, lease_units: u64) -> Option<u64> {
        lease_fee(lease_units, &self.annual_rate, self.duration_days)
    }
}

/// What `leaseOptions` answers: the terms, longest first, and who the fee goes to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeaseOptions {
    pub options: Vec<LeaseOption>,
    /// The account a lease's fee is transferred to.
    #[serde(rename = "lit_incentives_account_index")]
    pub fee_account: AccountIndex,
}

impl LeaseOptions {
    /// The term lasting `days`; a lease must take one of them.
    #[must_use]
    pub fn lasting(&self, days: u32) -> Option<&LeaseOption> {
        self.options.iter().find(|option| option.duration_days == days)
    }
}

/// What `leaseOptions` asks.
#[must_use]
pub const fn lease_options_call() -> Call {
    Call::get("leaseOptions")
}

/// The lease terms both venues offer, and the account fees go to.
pub async fn lease_options(transport: &impl Transport) -> Result<LeaseOptions, ApiError> {
    read(transport, lease_options_call()).await
}

/// The venue's fee for `lease_units` of LIT at `annual_rate` percent over `days`, in LIT units:
/// the rate is taken to hundredths of a percent, as the formula's `annual_rate * 100` does, and
/// the quotient cut. `None` for a rate below zero or a fee past 64 bits.
#[must_use]
pub fn lease_fee(lease_units: u64, annual_rate: &Figure, days: u32) -> Option<u64> {
    let hundredths = annual_rate.to_dec()?.units_at(2, Round::Down)?;
    let hundredths = u128::try_from(hundredths).ok()?;
    let product = u128::from(lease_units).checked_mul(hundredths)?.checked_mul(u128::from(days))?;
    u64::try_from(product / (360 * 10_000)).ok()
}

/// `lit` as the LIT units a lease counts, eight places at most: refused rather than rounded.
pub fn lease_units(lit: Dec) -> Result<u64, BadUnits> {
    exact_units(lit, LIT_DECIMALS, MOST_LEASE_UNITS)
}

/// Where a lease stands.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LeaseStatus {
    /// Taken, its fee not yet seen.
    WaitingFee,
    /// In force.
    Leased,
    Expired,
    /// Refused or undone; [`Lease::error`] says why.
    Canceled,
    /// A word this build does not know, kept as written.
    Other(String),
}

impl LeaseStatus {
    #[must_use]
    pub fn from_word(word: &str) -> LeaseStatus {
        match word {
            "waiting_fee" => LeaseStatus::WaitingFee,
            "leased" => LeaseStatus::Leased,
            "expired" => LeaseStatus::Expired,
            "canceled" => LeaseStatus::Canceled,
            other => LeaseStatus::Other(other.to_owned()),
        }
    }
}

impl<'de> Deserialize<'de> for LeaseStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|word| LeaseStatus::from_word(&word))
    }
}

/// One lease.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Lease {
    #[serde(deserialize_with = "whole")]
    pub id: u64,
    pub master_account_index: AccountIndex,
    /// LIT leased, in 1e8 units.
    #[serde(deserialize_with = "whole")]
    pub lease_amount: u64,
    /// LIT paid, in 1e8 units.
    #[serde(deserialize_with = "whole")]
    pub fee_amount: u64,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub start: u64,
    /// Unix milliseconds.
    #[serde(deserialize_with = "whole")]
    pub end: u64,
    pub status: LeaseStatus,
    /// Why it was canceled; empty otherwise.
    #[serde(default)]
    pub error: String,
}

impl Lease {
    /// Whether it counts toward the tier at `now_ms`.
    #[must_use]
    pub fn is_active(&self, now_ms: u64) -> bool {
        self.status == LeaseStatus::Leased && (self.start..self.end).contains(&now_ms)
    }
}

/// One page of `leases`, and the cursor to the next when there is one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Leases {
    #[serde(default)]
    pub leases: Vec<Lease>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// What `leases` asks: the account's own, with its auth token. `limit` is 1 to 100, 20 when
/// `None`.
#[must_use]
pub fn leases_call(account: AccountIndex, cursor: Option<&str>, limit: Option<u8>) -> Call {
    Call::get("leases")
        .field("account_index", account)
        .maybe("cursor", cursor)
        .maybe("limit", limit.map(|limit| limit.clamp(1, MOST_LEASES)))
        .authed()
}

/// A page of the account's leases, newest first.
pub async fn leases(
    transport: &impl Transport,
    account: AccountIndex,
    cursor: Option<&str>,
    limit: Option<u8>,
) -> Result<Leases, ApiError> {
    read(transport, leases_call(account, cursor, limit)).await
}

/// What `litLease` posts: the signed transfer that pays the fee (its `tx_info`, with the owner's
/// signature where it needs one), the LIT leased in 1e8 units, and a term's days. The fee must be
/// [`lease_fee`]'s for that term, paid to [`LeaseOptions::fee_account`]. Posting it moves money.
#[must_use]
pub fn lit_lease_call(tx_info: String, lease_units: u64, duration_days: u32) -> Call {
    Call::post("litLease")
        .field("tx_info", tx_info)
        .field("lease_amount", lease_units)
        .field("duration_days", duration_days)
}

#[cfg(test)]
#[path = "tests/credits.rs"]
mod tests;
