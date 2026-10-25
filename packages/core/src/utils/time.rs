//! The venue's clocks. Each field has its own unit, so each is converted by name, never by guess:
//! an order's expiry and a trade's `timestamp` are milliseconds, an order's `timestamp` and a
//! funding's are seconds, `transaction_time` and `last_updated_at` microseconds. There is no clock
//! here: the caller passes now, in milliseconds.

pub const SECOND_MS: u64 = 1_000;
pub const MINUTE_MS: u64 = 60 * SECOND_MS;
pub const HOUR_MS: u64 = 60 * MINUTE_MS;
pub const DAY_MS: u64 = 24 * HOUR_MS;

/// How long a transaction stays good after it is signed (lighter-go's `DefaultExpireTime`): ten
/// minutes less a second.
pub const TX_LIFETIME_MS: u64 = 10 * MINUTE_MS - SECOND_MS;

/// An order's expiry is between five minutes and thirty days ahead.
pub const MIN_ORDER_EXPIRY_MS: u64 = 5 * MINUTE_MS;
pub const MAX_ORDER_EXPIRY_MS: u64 = 30 * DAY_MS;

/// How long a resting order lives unless told otherwise: the venue's own apps' 28 days.
pub const RESTING_MS: u64 = 28 * DAY_MS;

/// Seconds as milliseconds.
#[must_use]
pub const fn seconds_to_ms(seconds: u64) -> u64 {
    seconds.saturating_mul(SECOND_MS)
}

/// Microseconds as whole milliseconds, the fraction dropped.
#[must_use]
pub const fn micros_to_ms(micros: u64) -> u64 {
    micros / 1_000
}

/// Milliseconds as whole seconds, the fraction dropped.
#[must_use]
pub const fn ms_to_seconds(ms: u64) -> u64 {
    ms / SECOND_MS
}
