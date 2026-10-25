//! Lighter SDK for Rust, on Lighter and on Lighter on Robinhood Chain.
//!
//! - [`client`]: the [`Client`], the REST API over HTTP, rate limits, the two networks, and the
//!   native and browser wires.
//! - [`markets`]: what is listed and what it trades at.
//! - [`account`]: what an account holds and who may sign for it.
//! - [`trade`]: orders on every market.
//! - [`transfer`]: moving money.
//! - [`earn`]: public pools, LIT staking and LIT fee credits.
//! - [`deploy`]: binary options markets.
//! - [`live`]: websocket subscriptions and kept live state.
//! - [`utils`]: decimals, a market's grid, ids, units and the signing primitives.
//!
//! Money is exact decimals, never floats. Features: `sign` and `native` (default), `web`.

pub mod account;
pub mod client;
pub mod deploy;
pub mod earn;
pub mod live;
pub mod markets;
pub mod trade;
pub mod transfer;
pub mod utils;

#[cfg(test)]
pub(crate) mod test_support;

pub use client::{Client, Network, SendError};
pub use utils::{Address, Dec};
