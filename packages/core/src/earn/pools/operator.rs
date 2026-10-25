//! Running a public pool: `CreatePublicPool` (10), which opens one with the operator's own
//! collateral, and `UpdatePublicPool` (11), which freezes or reopens it and changes its terms.
//! Both are signed by the operator's master account: a pool is a sub-account the venue opens for
//! it, at the next index down from the LLP.
//!
//! Units, as lighter-go counts them: the operator fee in millionths of depositors' profit
//! (100,000 is 10%), the minimum operator share in ten-thousandths of the pool (1,000 is 10%),
//! and the initial shares at 0.001 collateral each, so 1,000,000 shares open a pool with 1,000
//! USDC (USDG on Robinhood Chain). The venue asks at least that much, and lets an operator fee
//! only come down. Creating one is gated: six pools an L1 address, after $10M of paid volume.

use crate::utils::ids::AccountIndex;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Envelope, FEE_TICK, Tx, TxType, element, number};
use crate::utils::{BadUnits, Dec, exact_units};

/// The minimum operator share counts in ten-thousandths of the pool: 10,000 is all of it.
pub const SHARE_TICK: u16 = 10_000;

/// What one share is worth when a pool opens, in collateral: 0.001.
pub const INITIAL_SHARE_PRICE: Dec = Dec::lit::<3>(1);

/// The fewest shares a pool opens with, as the venue counts it: 1,000 collateral's worth.
pub const MIN_INITIAL_TOTAL_SHARES: u64 = 1_000_000;

/// The most shares a pool opens with: a billion collateral's worth.
pub const MAX_INITIAL_TOTAL_SHARES: u64 = 1_000_000_000_000;

/// Whether a pool takes deposits: lighter-go's `Status`, 0 or 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PoolStatus {
    /// Takes deposits and withdrawals.
    #[default]
    Active,
    /// Withdrawals only: a mint is refused (`21203`).
    Frozen,
    /// A status this build does not know, kept as written.
    Other(u8),
}

impl PoolStatus {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            PoolStatus::Active => 0,
            PoolStatus::Frozen => 1,
            PoolStatus::Other(code) => code,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> PoolStatus {
        match code {
            0 => PoolStatus::Active,
            1 => PoolStatus::Frozen,
            other => PoolStatus::Other(other),
        }
    }
}

/// From the number the venue writes; one it does not know is kept, never a failed read.
impl<'de> serde::Deserialize<'de> for PoolStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u8::deserialize(deserializer).map(PoolStatus::from_code)
    }
}

/// A rate as a count of `10^-places`, zero allowed: refused when negative, finer than that or
/// past `most`.
fn rate(value: Dec, places: u32, most: u64) -> Result<u64, BadUnits> {
    match value.is_zero() {
        true => Ok(0),
        false => exact_units(value, places, most),
    }
}

/// The operator fee, a fraction of depositors' profit (`0.1` is 10%), in millionths.
pub fn operator_fee_units(fee: Dec) -> Result<u32, BadUnits> {
    let units = rate(fee, 6, u64::from(FEE_TICK))?;
    Ok(u32::try_from(units).unwrap_or(FEE_TICK))
}

/// The minimum operator share, a fraction of the pool (`0.1` is 10%), in ten-thousandths.
pub fn min_operator_share_units(share: Dec) -> Result<u16, BadUnits> {
    let units = rate(share, 4, u64::from(SHARE_TICK))?;
    Ok(u16::try_from(units).unwrap_or(SHARE_TICK))
}

fn check_fee(fee: u32) -> Result<(), BadTx> {
    match fee > FEE_TICK {
        true => Err(BadTx::new("OperatorFee", "an operator fee is at most 1,000,000, all of the profit")),
        false => Ok(()),
    }
}

fn check_share_rate(rate: u16) -> Result<(), BadTx> {
    match rate > SHARE_TICK {
        true => Err(BadTx::new("MinOperatorShareRate", "a minimum operator share is at most 10,000, all of the pool")),
        false => Ok(()),
    }
}

/// `CreatePublicPool`: a new pool, opened with `initial_total_shares` of the operator's own
/// collateral at [`INITIAL_SHARE_PRICE`]. The envelope's account must be a master account; the
/// venue answers the pool's index in the transaction's event (`a`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatePublicPool {
    /// Millionths of depositors' profit.
    pub operator_fee: u32,
    pub initial_total_shares: u64,
    /// Ten-thousandths of the pool.
    pub min_operator_share_rate: u16,
}

impl CreatePublicPool {
    /// A pool charging `operator_fee` of profit (a fraction, six places at most), opened with
    /// `initial` collateral (whole thousandths: one share each) and keeping the operator at
    /// `min_operator_share` of it (four places at most). Refused rather than rounded.
    pub fn new(operator_fee: Dec, initial: Dec, min_operator_share: Dec) -> Result<CreatePublicPool, BadUnits> {
        Ok(CreatePublicPool {
            operator_fee: operator_fee_units(operator_fee)?,
            initial_total_shares: exact_units(initial, 3, MAX_INITIAL_TOTAL_SHARES)?,
            min_operator_share_rate: min_operator_share_units(min_operator_share)?,
        })
    }
}

impl Tx for CreatePublicPool {
    fn tx_type(&self) -> TxType {
        TxType::CreatePublicPool
    }

    /// Only a master account creates a pool (lighter-go's `MaxMasterAccountIndex`).
    fn check_signing(&self, _chain_id: u32, envelope: &Envelope) -> Result<(), BadTx> {
        match envelope.account.is_master() {
            true => Ok(()),
            false => Err(BadTx::new("AccountIndex", "a public pool is created by a master account")),
        }
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.operator_fee), element(self.initial_total_shares), element(self.min_operator_share_rate)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("OperatorFee", number(self.operator_fee)),
            ("InitialTotalShares", number(self.initial_total_shares)),
            ("MinOperatorShareRate", number(self.min_operator_share_rate)),
        ]
    }

    /// lighter-go's `Validate`: it refuses no shares, not too few; the venue holds the minimum.
    fn check(&self) -> Result<(), BadTx> {
        check_fee(self.operator_fee)?;
        if self.initial_total_shares == 0 {
            return Err(BadTx::new("InitialTotalShares", "a pool opens with at least one share"));
        }
        if self.initial_total_shares > MAX_INITIAL_TOTAL_SHARES {
            return Err(BadTx::new("InitialTotalShares", "a pool opens with at most 10^12 shares"));
        }
        check_share_rate(self.min_operator_share_rate)
    }
}

/// `UpdatePublicPool`: the pool's status and terms, all three set at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdatePublicPool {
    pub pool: AccountIndex,
    pub status: PoolStatus,
    /// Millionths of depositors' profit; the venue lets it only come down.
    pub operator_fee: u32,
    /// Ten-thousandths of the pool.
    pub min_operator_share_rate: u16,
}

impl UpdatePublicPool {
    /// `pool` set to `status`, charging `operator_fee` (a fraction, six places at most) and
    /// keeping the operator at `min_operator_share` (four places at most).
    pub fn new(
        pool: AccountIndex,
        status: PoolStatus,
        operator_fee: Dec,
        min_operator_share: Dec,
    ) -> Result<UpdatePublicPool, BadUnits> {
        Ok(UpdatePublicPool {
            pool,
            status,
            operator_fee: operator_fee_units(operator_fee)?,
            min_operator_share_rate: min_operator_share_units(min_operator_share)?,
        })
    }
}

impl Tx for UpdatePublicPool {
    fn tx_type(&self) -> TxType {
        TxType::UpdatePublicPool
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![
            element(self.pool.get()),
            element(self.status.code()),
            element(self.operator_fee),
            element(self.min_operator_share_rate),
        ]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("PublicPoolIndex", number(self.pool.get())),
            ("Status", number(self.status.code())),
            ("OperatorFee", number(self.operator_fee)),
            ("MinOperatorShareRate", number(self.min_operator_share_rate)),
        ]
    }

    /// lighter-go's `Validate`: any account index names the pool (the venue says whether it is
    /// one); the status is active or frozen.
    fn check(&self) -> Result<(), BadTx> {
        if !matches!(self.status, PoolStatus::Active | PoolStatus::Frozen) {
            return Err(BadTx::new("Status", "a pool is active (0) or frozen (1)"));
        }
        check_fee(self.operator_fee)?;
        check_share_rate(self.min_operator_share_rate)
    }
}

#[cfg(test)]
#[path = "tests/operator.rs"]
mod tests;
