//! A position's margin: its leverage and margin mode, and the collateral an isolated one holds.
//!
//! - [`leverage`]: `UpdateLeverage` (20): the initial margin fraction (10,000 over the leverage)
//!   and cross or isolated, held to what the market allows.
//! - [`isolated`]: `UpdateMargin` (29): collateral into or out of an isolated position.

pub mod isolated;
pub mod leverage;

pub use isolated::{MAX_MARGIN_UNITS, MarginDirection, UpdateMargin};
pub use leverage::{MARGIN_FRACTION_TICK, MarginLimits, UpdateLeverage, leverage_fraction};
