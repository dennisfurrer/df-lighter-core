//! The venue refusing something sent ([`Refusal`]): a subscribe, an unsubscribe, a transaction.
//! It comes as `{"error":{"code":..,"message":..}}`, with the `id` a transaction was sent with
//! when it refuses one; a subscription's refusal names its channel at the end of the message
//! (`Already Subscribed to : height`).

use crate::client::ApiError;
use crate::live::Subscription;

/// The socket's own codes, seen on the venue's answers.
pub mod codes {
    /// `invalid param`: among others, a private channel subscribed without its token
    /// (`auth field is required: account_all_orders:10`).
    pub const INVALID_PARAM: i64 = 20_001;
    /// `Not Subscribed to : <channel>`: an unsubscribe of what was not subscribed.
    pub const NOT_SUBSCRIBED: i64 = 30_002;
    /// `Already Subscribed to : <channel>`: a second subscribe, which brings no snapshot.
    pub const ALREADY_SUBSCRIBED: i64 = 30_003;
    /// `Invalid Channel`.
    pub const INVALID_CHANNEL: i64 = 30_005;
    /// A batch of more than fifty transactions.
    pub const BATCH_TOO_LARGE: i64 = 21_514;
}

/// The venue refusing something, in its own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: i64,
    pub message: String,
    /// The id of the transaction or batch it refuses.
    pub id: Option<String>,
}

impl Refusal {
    /// The channel it names, as the venue writes channels (`account_all_orders:10`), when its
    /// message ends with one.
    #[must_use]
    pub fn channel(&self) -> Option<&str> {
        let (_, last) = self.message.rsplit_once(": ")?;
        let last = last.trim();
        let plain = !last.is_empty()
            && last
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b':'));
        plain.then_some(last)
    }

    /// The subscription it refuses, when it names one this crate reads.
    #[must_use]
    pub fn subscription(&self) -> Option<Subscription> {
        Subscription::parse(self.channel()?).ok()
    }

    /// As the REST API's refusal, for code that handles both the same way.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        ApiError::Refused { code: self.code, message: self.message.clone() }
    }
}

#[cfg(test)]
#[path = "tests/control.rs"]
mod tests;
