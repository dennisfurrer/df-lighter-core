//! What a socket feed hands back and why it refuses, the same for the native and the browser
//! feed (`client::native::Feed`, `client::web::Feed`).

use crate::live::{BadFrame, Frame, Refused, Retry};

/// What a feed hands back, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The socket opened and everything wanted was subscribed again.
    Opened,
    /// Boxed: a frame is large and most events are not frames.
    Frame(Box<Frame>),
    /// A frame that did not read. The socket is still alive.
    Unreadable(BadFrame),
    /// The socket closed, or could not open. The feed dials again after `retry.after_ms`; the
    /// transactions in `retry.unanswered` may or may not have landed, so read the account to
    /// find out.
    Closed { reason: String, retry: Retry },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    #[error(transparent)]
    Refused(#[from] Refused),
    /// The feed is gone for good. In the browser this also means the runtime gave no timer
    /// (no `setTimeout`, or one that threw), so the feed could not keep its time.
    #[error("the feed has stopped")]
    Stopped,
}
