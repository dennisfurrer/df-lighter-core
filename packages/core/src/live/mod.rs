//! The websocket without the socket: subscriptions, the frames sent and received, the
//! connection's state, what a feed hands back, and a store that keeps live state for an app. Each
//! piece is sans-IO and stands alone: an app that runs its own socket subscribes with
//! [`subscribe`], reads with [`decode`] and keeps a book with [`store::Book`]; the native and
//! browser feeds (`client::native::Feed`, `client::web::Feed`) carry a [`Session`] over a socket
//! for an app that does not. Rows the socket sends in the shapes the reads answer are the
//! [`crate::account`] and [`crate::markets`] types; only the socket's own are defined here.
//!
//! - [`subscription`]: every channel, its name, and whether it needs the auth token.
//! - [`request`]: subscribe, unsubscribe, ping, and a signed transaction over the socket.
//! - [`frame`]: received frames read into [`Frame`]s.
//! - [`market`], [`account`]: what each channel sends.
//! - [`session`]: one connection's state: what is wanted, the ping, the reconnect, the book
//!   chains, the transactions waiting for an answer.
//! - [`feed`]: what a feed hands back.
//! - [`store`]: kept books, market figures and accounts.

pub mod account;
pub mod feed;
pub mod frame;
pub mod market;
pub mod request;
pub mod session;
pub mod store;
pub mod subscription;

pub use account::{
    AccountAll, AccountAllAssets, AccountAllOrders, AccountAllPositions, AccountAllTrades, AccountMarket,
    AccountOrders, AccountTx, AvgEntryPrice, AvgEntryPrices, LivePoints, NoteContent, Notification, Notifications,
    PoolData, Rfq, Stats, UserStats, Volumes,
};
pub use feed::{Event, FeedError};
pub use frame::{BadFrame, Data, Frame, Gap, Refusal, decode};
pub use market::{
    BoMarketStats, BoStatus, BookUpdate, Candles, Height, Level, MarkPriceCandles, MarketStats, Resolution,
    SpotMarketStats, Ticker, Trades, UpDownCandles, UpDownResolution,
};
pub use request::{PING, PONG, resubscribe, send_tx, send_tx_batch, subscribe, unsubscribe};
pub use session::{
    Limits, MAX_ACCOUNTS, MAX_CONNECTIONS, MAX_INFLIGHT_MESSAGES, MAX_MESSAGES_PER_MINUTE, MAX_SUBSCRIPTIONS,
    MAX_SUBSCRIPTIONS_PER_CONNECTION, Refused, Retry, Session, Tick, Timing,
};
pub use subscription::{BadSubscription, Subscription};

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;
