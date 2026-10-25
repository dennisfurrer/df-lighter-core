//! Why a builder made no transaction: a figure that does not fit the market's grid, a slippage or
//! an expiry outside what the venue takes, a leverage past the market's, or a transaction lighter-go
//! would refuse. Nothing is signed.

use crate::utils::sign::BadTx;
use crate::utils::{BadUnits, Dec};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TradeError {
    /// A size, price or amount that is not a whole number of the grid's units (`field` is the one
    /// signed: `BaseAmount`, `Price`, `TriggerPrice`, `USDCAmount`), or past its range.
    #[error("{field}: {error}")]
    Units { field: &'static str, error: BadUnits },
    /// A quote-sized order with nothing to price it at, or a reference that is not positive.
    #[error("{0} needs a positive price")]
    NoPrice(&'static str),
    /// More slippage than [`crate::trade::MAX_SLIPPAGE_BPS`] (10%), Lighter's own client limit.
    #[error("{0} basis points of slippage is more than the 1000 allowed")]
    Slippage(u32),
    /// A market order's worst price further than [`crate::trade::MAX_SLIPPAGE_BPS`] from its
    /// reference.
    #[error("the worst price {worst} is more than 10% from the reference {reference}")]
    TooFar { worst: Dec, reference: Dec },
    /// An expiry outside the venue's window: 5 minutes to 30 days ahead for an order, 5 minutes
    /// to 15 days for a scheduled cancel-all, 5 minutes to 24 hours for a TWAP's run.
    #[error("{what} is between {least_ms} and {most_ms} ms ahead; {at} is not (now {now})")]
    Expiry { field: &'static str, what: &'static str, at: u64, now: u64, least_ms: u64, most_ms: u64 },
    /// What this kind of order does not take: an expiry on an immediate-or-cancel order, a time in
    /// force on a market or trigger order, a whole-position size on an order that may open.
    #[error("{0}")]
    NotFor(&'static str),
    /// Below the market's least size (`min_base_amount`); only a resting order is held to it.
    #[error("{size} is below the market's least size {least}")]
    BelowLeastSize { size: Dec, least: Dec },
    /// Worth less than the market's least order value (`min_quote_amount`); only a resting order
    /// is held to it.
    #[error("{value} is below the market's least order value {least}")]
    BelowLeastValue { value: Dec, least: Dec },
    /// Leverage is a whole number from 1 to 10,000.
    #[error("leverage is 1x or more")]
    NoLeverage,
    /// More leverage than the market's least initial margin fraction allows.
    #[error("the most leverage on this market is {most}x, not {asked}x")]
    Leverage { asked: u16, most: u16 },
    /// Cross margin on a market that trades isolated margin only. Refused rather than switched.
    #[error("this market trades isolated margin only")]
    IsolatedOnly,
    /// What lighter-go refuses.
    #[error(transparent)]
    Bad(#[from] BadTx),
}

impl TradeError {
    /// The field it is about, as lighter-go names it.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        match self {
            TradeError::Units { field, .. } => field,
            TradeError::NoPrice(_) | TradeError::Slippage(_) | TradeError::TooFar { .. } => "Price",
            TradeError::Expiry { field, .. } => field,
            TradeError::NotFor(_) => "Type",
            TradeError::BelowLeastSize { .. } | TradeError::BelowLeastValue { .. } => "BaseAmount",
            TradeError::NoLeverage | TradeError::Leverage { .. } => "InitialMarginFraction",
            TradeError::IsolatedOnly => "MarginMode",
            TradeError::Bad(bad) => bad.field,
        }
    }
}

/// As a refusal before signing, so a convenience call answers it as a
/// [`crate::client::SendError::Sign`].
impl From<TradeError> for BadTx {
    fn from(error: TradeError) -> BadTx {
        match error {
            TradeError::Bad(bad) => bad,
            other => BadTx::new(other.field(), other.to_string()),
        }
    }
}

/// The units a figure signs as, or why not, naming the field.
pub(crate) fn units<T>(field: &'static str, converted: Result<T, BadUnits>) -> Result<T, TradeError> {
    converted.map_err(|error| TradeError::Units { field, error })
}
