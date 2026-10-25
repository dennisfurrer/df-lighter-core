//! Received frames read into types ([`decode`]). A channel's frame is its whole on subscribe
//! (`subscribed/<channel>`, a [`Frame::Snapshot`]) or what changed (`update/<channel>`, a
//! [`Frame::Update`]), its data typed by channel ([`Data`]); the rest are the socket's own: the
//! greeting, pings and pongs, an unsubscribe's answer, a refusal, a transaction's answer. Numbers
//! are exact decimals, never floats. An unknown channel is [`Data::Other`] and an unknown frame
//! [`Frame::Other`], kept as written; a frame that does not read is a [`BadFrame`], never a panic.
//!
//! - [`read`]: [`decode`] and [`BadFrame`].
//! - [`control`]: the venue's refusals ([`Refusal`]) and their codes.

pub mod control;
pub(crate) mod fields;
pub mod read;

pub use control::{Refusal, codes};
pub use read::{BadFrame, decode};

use std::fmt;

use crate::client::{Sent, SentBatch};
use crate::earn::pools::PoolInfo;
use crate::live::account::{
    AccountAll, AccountAllAssets, AccountAllOrders, AccountAllPositions, AccountAllTrades, AccountMarket,
    AccountOrders, AccountTx, AvgEntryPrices, LivePoints, Notifications, PoolData, Rfq, UserStats,
};
use crate::live::market::{
    BoMarketStats, BookUpdate, Candles, Height, MarkPriceCandles, MarketStats, SpotMarketStats, Ticker, Trades,
    UpDownCandles,
};
use crate::utils::{AccountIndex, MarketId};

/// One frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// `connected`: the socket is up. The id names this connection in the venue's logs.
    Connected { session_id: String },
    /// `pong`: the answer to [`crate::live::PING`].
    Pong,
    /// `ping`: the venue asks for a [`crate::live::PONG`]. A [`crate::live::Session`] owes one.
    Ping,
    /// `unsubscribed`: the venue stopped a channel, named as its frames name it.
    Unsubscribed { channel: String },
    /// `{"error":{"code":..,"message":..}}`: the venue refused something sent.
    Error(Refusal),
    /// The venue took a transaction sent over the socket: the id it was sent with, and what
    /// `sendTx` would have answered. Taken is not executed; `account_tx` says how it went.
    Sent { id: String, sent: Sent },
    /// The venue took a batch.
    SentBatch { id: String, sent: SentBatch },
    /// `subscribed/<channel>`: the channel's whole as it stands.
    Snapshot(Data),
    /// `update/<channel>`: what changed since.
    Update(Data),
    /// An `order_book` update whose `begin_nonce` is not the nonce the book was at: one was lost,
    /// and the book is wrong from here until a fresh snapshot. Only a [`crate::live::Session`]
    /// says this, in place of the update; it owes the venue the resubscribe that brings the
    /// snapshot. [`decode`] alone hands the update on, and [`crate::live::store::Book`] finds
    /// the gap itself.
    Gap(Gap),
    /// A frame this build does not read: its `type`, and the frame as written.
    Other { kind: String, text: String },
}

/// A channel's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data {
    OrderBook(BookUpdate),
    Ticker(Ticker),
    /// One perp's figures, or on `all` each perp's that moved.
    MarketStats(Vec<MarketStats>),
    SpotMarketStats(Vec<SpotMarketStats>),
    BoMarketStats(Vec<BoMarketStats>),
    Trades(Trades),
    Candles(Candles),
    MarkPriceCandles(MarkPriceCandles),
    UpDownCandles(UpDownCandles),
    Height(Height),
    /// Boxed: an account's frame is large and most frames are not one.
    AccountAll(Box<AccountAll>),
    AccountMarket(Box<AccountMarket>),
    UserStats(Box<UserStats>),
    AccountTx(AccountTx),
    AccountAllOrders(AccountAllOrders),
    AccountOrders(AccountOrders),
    AccountAllTrades(AccountAllTrades),
    AccountAllPositions(AccountAllPositions),
    AccountAllAssets(AccountAllAssets),
    AvgEntryPrices(AvgEntryPrices),
    PoolData(Box<PoolData>),
    /// A pool's terms, by the pool's account.
    PoolInfo {
        account: AccountIndex,
        info: Box<PoolInfo>,
    },
    Notifications(Notifications),
    Rfqs(Vec<Rfq>),
    LivePoints(LivePoints),
    /// A channel this build does not read: its channel as named, and the frame as written.
    Other {
        channel: String,
        text: String,
    },
}

/// An order book update that does not follow the book: `expected` was its nonce, the update
/// began at `got`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    pub market: MarketId,
    pub expected: u64,
    pub got: u64,
}

impl fmt::Display for Gap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "market {}: an order book update began at nonce {} where the book was at {}; a frame was lost",
            self.market, self.got, self.expected
        )
    }
}

impl std::error::Error for Gap {}

impl Frame {
    /// A channel's data, on subscribe or after.
    #[must_use]
    pub const fn data(&self) -> Option<&Data> {
        match self {
            Frame::Snapshot(data) | Frame::Update(data) => Some(data),
            _ => None,
        }
    }

    /// Whether it is a channel's whole, which replaces what was held.
    #[must_use]
    pub const fn is_snapshot(&self) -> bool {
        matches!(self, Frame::Snapshot(_))
    }

    /// The id of the transaction or batch it answers, taken or refused.
    #[must_use]
    pub fn answers(&self) -> Option<&str> {
        match self {
            Frame::Sent { id, .. } | Frame::SentBatch { id, .. } => Some(id),
            Frame::Error(refusal) => refusal.id.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "tests/frame.rs"]
mod tests;
