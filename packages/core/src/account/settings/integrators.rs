//! Partners (integrators): `ApproveIntegrator`, which lets another account append its fees to this
//! account's orders up to the rates approved, until a time; the same transaction with every fee
//! zero and no expiry revokes it. An account approves at most four integrators at once, and lists
//! them as `approved_integrators` ([`ApprovedIntegrator`]).
//!
//! Fees are millionths of notional ([`FEE_TICK`]): 100 is one basis point. Perps fees are charged
//! in the collateral; spot fees in the asset received, and binary options use the spot rates. The
//! venue caps each rate system-wide (`systemConfig`); non-zero fees need the client on Plus or
//! Premium.
//!
//! The owner's signature: lighter-go writes the text the owner signs ([`Tx::l1_message`]) for every
//! approval, and `tx_info` carries `L1Sig` after `Sig`. The venue asks for it only when a fee is
//! above zero and the integrator's account belongs to another L1 address
//! ([`ApproveIntegrator::needs_owner`] says the first half; the caller knows the second). Send
//! with [`crate::Client::send_with_owner`] then, with [`crate::Client::send`] otherwise.

use serde::Deserialize;

use crate::utils::ids::{WIDTH_48, whole};
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Envelope, FEE_TICK, Tx, TxType, element, hex16, number};
use crate::utils::{AccountIndex, Dec};

/// `ApproveIntegrator`: `integrator` may charge up to these rates (millionths of notional) until
/// `expiry`, unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApproveIntegrator {
    pub integrator: AccountIndex,
    pub max_perps_taker_fee: u32,
    pub max_perps_maker_fee: u32,
    pub max_spot_taker_fee: u32,
    pub max_spot_maker_fee: u32,
    /// Unix milliseconds; 0 only for a revocation.
    pub expiry: u64,
}

impl ApproveIntegrator {
    /// The same rates for taker and maker, on perps and on spot.
    #[must_use]
    pub const fn new(integrator: AccountIndex, perps_fee: u32, spot_fee: u32, expiry: u64) -> ApproveIntegrator {
        ApproveIntegrator {
            integrator,
            max_perps_taker_fee: perps_fee,
            max_perps_maker_fee: perps_fee,
            max_spot_taker_fee: spot_fee,
            max_spot_maker_fee: spot_fee,
            expiry,
        }
    }

    /// Takes `integrator`'s approval away: every fee zero, no expiry.
    #[must_use]
    pub const fn revoke(integrator: AccountIndex) -> ApproveIntegrator {
        ApproveIntegrator::new(integrator, 0, 0, 0)
    }

    /// Whether it revokes rather than approves.
    #[must_use]
    pub const fn is_revocation(&self) -> bool {
        self.expiry == 0 && !self.charges()
    }

    /// Whether any rate is above zero.
    #[must_use]
    pub const fn charges(&self) -> bool {
        self.max_perps_taker_fee != 0
            || self.max_perps_maker_fee != 0
            || self.max_spot_taker_fee != 0
            || self.max_spot_maker_fee != 0
    }

    /// Whether the venue may ask for the owner's signature: when any rate is above zero, and then
    /// only for an integrator under another L1 address. A zero-fee approval (tracking only) and a
    /// revocation are the API key's alone.
    #[must_use]
    pub const fn needs_owner(&self) -> bool {
        self.charges()
    }

    fn fees(&self) -> [u32; 4] {
        [self.max_perps_taker_fee, self.max_perps_maker_fee, self.max_spot_taker_fee, self.max_spot_maker_fee]
    }
}

impl Tx for ApproveIntegrator {
    fn tx_type(&self) -> TxType {
        TxType::ApproveIntegrator
    }

    fn hash_fields(&self) -> Vec<Fp> {
        let mut fields = vec![element(self.integrator.get())];
        fields.extend(self.fees().map(element));
        fields.push(element(self.expiry));
        fields
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("IntegratorAccountIndex", number(self.integrator.get())),
            ("MaxPerpsTakerFee", number(self.max_perps_taker_fee)),
            ("MaxPerpsMakerFee", number(self.max_perps_maker_fee)),
            ("MaxSpotTakerFee", number(self.max_spot_taker_fee)),
            ("MaxSpotMakerFee", number(self.max_spot_maker_fee)),
            ("ApprovalExpiry", number(self.expiry)),
        ]
    }

    fn trailing_l1_sig(&self) -> bool {
        true
    }

    /// Written for every approval, as lighter-go writes it; see the module's note on when the
    /// venue asks for it.
    fn l1_message(&self, envelope: &Envelope, chain_id: u32) -> Option<String> {
        Some(format!(
            "Approve Integrator\n\nnonce: {}\naccount index: {}\napi key index: {}\nintegrator account index: {}\nmax perps taker fee: {}\nmax perps maker fee: {}\nmax spot taker fee: {}\nmax spot maker fee: {}\napproval expiry: {}\nchainId: {}\nOnly sign this message for a trusted client!",
            hex16(envelope.nonce),
            hex16(envelope.account.get()),
            hex16(u64::from(envelope.api_key.get())),
            hex16(self.integrator.get()),
            hex16(u64::from(self.max_perps_taker_fee)),
            hex16(u64::from(self.max_perps_maker_fee)),
            hex16(u64::from(self.max_spot_taker_fee)),
            hex16(u64::from(self.max_spot_maker_fee)),
            hex16(self.expiry),
            hex16(u64::from(chain_id)),
        ))
    }

    /// lighter-go's `Validate`: each rate at most [`FEE_TICK`], fees only with an expiry, the expiry
    /// below 2^48.
    fn check(&self) -> Result<(), BadTx> {
        if self.fees().iter().any(|fee| *fee > FEE_TICK) {
            return Err(BadTx::new("MaxFee", "an integrator rate is at most 1,000,000, all of the notional"));
        }
        if self.expiry == 0 && self.charges() {
            return Err(BadTx::new("ApprovalExpiry", "an approval with fees needs an expiry; zero revokes"));
        }
        if self.expiry > WIDTH_48 {
            return Err(BadTx::new("ApprovalExpiry", "an expiry is unix milliseconds below 2^48"));
        }
        Ok(())
    }
}

/// An integrator the account has approved, as `account` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApprovedIntegrator {
    #[serde(rename = "account_index")]
    pub integrator: AccountIndex,
    #[serde(default)]
    pub name: String,
    /// Millionths of notional.
    #[serde(default)]
    pub max_perps_taker_fee: u32,
    #[serde(default)]
    pub max_perps_maker_fee: u32,
    #[serde(default)]
    pub max_spot_taker_fee: u32,
    #[serde(default)]
    pub max_spot_maker_fee: u32,
    /// Unix milliseconds, after which the approval lapses.
    #[serde(rename = "approval_expiry", deserialize_with = "whole")]
    pub expiry: u64,
}

impl ApprovedIntegrator {
    /// Whether the approval still stands at `now_ms`.
    #[must_use]
    pub const fn is_active(&self, now_ms: u64) -> bool {
        self.expiry > now_ms
    }

    /// A rate as a fraction of notional: 1000 is `0.001`, ten basis points.
    #[must_use]
    pub fn fraction(rate: u32) -> Dec {
        Dec::new(i128::from(rate), 6).unwrap_or(Dec::ZERO)
    }
}

#[cfg(test)]
#[path = "tests/integrators.rs"]
mod tests;
