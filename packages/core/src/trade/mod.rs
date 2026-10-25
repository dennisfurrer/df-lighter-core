//! Orders on every market: place, group, cancel, modify, leverage and isolated margin, each a
//! transaction of plain data that signs as lighter-go signs it.
//!
//! - [`orders`]: `CreateOrder` (14) and `CreateGroupedOrders` (28); an order built from decimals
//!   on a market's grid, its client index, a taker's worst price.
//! - [`manage`]: `CancelOrder` (15), `CancelAllOrders` (16), `ModifyOrder` (17).
//! - [`margin`]: `UpdateLeverage` (20), `UpdateMargin` (29).
//! - [`error`]: why a builder made nothing.
//! - [`read_back`](mod@read_back): a transaction another signer returned, held to what was asked.
//! - `send` (with `sign`): each signed at the key's next nonce and sent through a [`Client`].
//!
//! Every type implements [`Tx`]: sign it with [`Client::send`] (or [`Client::sign`] to post it
//! another way), or with [`crate::utils::sign::sign_drawing`] and randomness of the caller's own.
//! Builders read no clock and no network: the caller passes now, the market's [`Grid`] and a
//! reference price.
//!
//! [`Client`]: crate::client::Client
//! [`Client::send`]: crate::client::Client::send
//! [`Client::sign`]: crate::client::Client::sign
//! [`Tx`]: crate::utils::sign::Tx
//! [`Grid`]: crate::utils::Grid

pub mod error;
pub mod manage;
pub mod margin;
pub mod orders;
pub mod read_back;
#[cfg(feature = "sign")]
pub mod send;

pub use crate::utils::sign::{Attributes, SelfTrade, SelfTradeScope};
pub use crate::utils::{ClientOrderIndex, MarginMode, OrderIndex, OrderKind, OrderRef, Side, TimeInForce};
pub use error::TradeError;
pub use manage::{
    CANCEL_ALL_LEAST_MS, CANCEL_ALL_MOST_MS, CancelAllOrders, CancelAllTime, CancelOrder, ModifyIntent, ModifyOrder,
};
pub use margin::{
    MARGIN_FRACTION_TICK, MAX_MARGIN_UNITS, MarginDirection, MarginLimits, UpdateLeverage, UpdateMargin,
    leverage_fraction,
};
pub use orders::{
    ClientIndices, CreateGroupedOrders, CreateOrder, DEFAULT_SLIPPAGE_BPS, Exit, Fit, Grouping, MAX_SLIPPAGE_BPS,
    MOST_IN_GROUP, Order, OrderIntent, Size, TRIGGER_SLIPPAGE_BPS, TWAP_LEAST_MS, TWAP_MOST_MS, worst_price,
};
pub use read_back::read_back;
#[cfg(feature = "sign")]
pub use send::{
    abort_cancel_all, add_margin, cancel, cancel_all, cancel_all_in, cancel_all_in_markets, modify, place, place_group,
    remove_margin, schedule_cancel_all, set_leverage,
};

#[cfg(test)]
#[path = "tests/support.rs"]
pub(crate) mod support;
