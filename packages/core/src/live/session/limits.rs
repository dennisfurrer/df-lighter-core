//! The venue's per-IP websocket limits, the part of them one session holds ([`Limits`]), and why
//! a session sends nothing ([`Refused`]).

use crate::client::http::MOST_IN_BATCH;
use crate::live::BadSubscription;
use crate::utils::json::BadJson;

/// Websocket connections an IP may hold.
pub const MAX_CONNECTIONS: usize = 255;
/// Subscriptions one connection may hold.
pub const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 500;
/// Subscriptions an IP may hold across its connections.
pub const MAX_SUBSCRIPTIONS: usize = 5_000;
/// Distinct accounts an IP's account channels may follow.
pub const MAX_ACCOUNTS: usize = 5_000;
/// Frames a client may send a minute across its connections; transactions are counted with the
/// REST API's instead.
pub const MAX_MESSAGES_PER_MINUTE: usize = 200;
/// Frames a client may have in flight; transactions are not counted.
pub const MAX_INFLIGHT_MESSAGES: usize = 50;

/// What one session may hold. [`Limits::VENUE`] is one connection's share of an IP's allowance;
/// sessions sharing an IP split [`MAX_SUBSCRIPTIONS`] and [`MAX_ACCOUNTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub subscriptions: usize,
    pub accounts: usize,
    /// Transactions sent and not yet answered. The venue counts none of them in flight; this is
    /// the session's own bound, so a socket that stops answering cannot pile them up.
    pub transactions: usize,
}

impl Limits {
    pub const VENUE: Limits = Limits {
        subscriptions: MAX_SUBSCRIPTIONS_PER_CONNECTION,
        accounts: MAX_ACCOUNTS,
        transactions: MAX_INFLIGHT_MESSAGES,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::VENUE
    }
}

/// Why nothing was sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("{limit} subscriptions is the most this session holds; the venue allows a connection 500")]
    Subscriptions { limit: usize },
    #[error("{limit} accounts is the most this session follows; the venue allows an IP 5000")]
    Accounts { limit: usize },
    #[error("{limit} transactions are already waiting for an answer")]
    Transactions { limit: usize },
    /// A channel that needs the account's token, on a session given none.
    #[error("`{0}` needs the account's auth token, and the session has none")]
    NoToken(String),
    /// A transaction is never held to send later: one sent late may land after the moment it was
    /// meant for, and one sent again may land twice.
    #[error("the socket is not open, and a transaction is never held to send later")]
    Closed,
    /// The read-only socket (`?readonly=true`) serves data and takes no transaction.
    #[error("the read-only socket takes no transactions")]
    ReadOnly,
    #[error("a batch carries 1 to {MOST_IN_BATCH} transactions, not {0}")]
    Batch(usize),
    #[error("`tx_info` is not JSON: {0}")]
    Unwritable(BadJson),
    #[error(transparent)]
    Bad(#[from] BadSubscription),
}

#[cfg(test)]
#[path = "tests/limits.rs"]
mod tests;
