//! Orders already placed: cancel one, cancel all (now, in one market, or on a schedule as a dead
//! man's switch), modify one.
//!
//! - [`cancel`]: `CancelOrder` (15), by the client index it was placed with or the venue's index.
//! - [`cancel_all`]: `CancelAllOrders` (16).
//! - [`modify`]: `ModifyOrder` (17): a new size, price or trigger, and the version it must pass.

pub mod cancel;
pub mod cancel_all;
pub mod modify;

pub use cancel::CancelOrder;
pub use cancel_all::{CANCEL_ALL_LEAST_MS, CANCEL_ALL_MOST_MS, CancelAllOrders, CancelAllTime};
pub use modify::{ModifyIntent, ModifyOrder};
