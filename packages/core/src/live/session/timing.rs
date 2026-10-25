//! The session's clock: its rules in milliseconds ([`Timing`]), what a tick asks of the socket's
//! owner, and how long a closed socket waits before it connects again.

use crate::live::session::{Link, Session};

/// The clock's rules, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Ping this long after the last ping, or the open. The venue closes a socket it has had no
    /// frame from for two minutes, however much it sends itself; the pong also shows the socket
    /// still answers.
    pub ping_after_ms: u64,
    /// A socket nothing has arrived on for this long, not even the pong to a ping, is dead. When
    /// no close came, the connection is half open.
    pub silent_ms: u64,
    /// A socket open this long that heard something worked: the next failure starts the
    /// reconnect delays over. One dropped sooner keeps backing off.
    pub steady_ms: u64,
    /// The first reconnect delay; each failure after doubles it.
    pub first_retry_ms: u64,
    /// The longest reconnect delay, before jitter adds up to a quarter.
    pub last_retry_ms: u64,
}

impl Timing {
    /// A 30 s ping, a quarter of the venue's idle limit; dead after 60 s of nothing; reconnects
    /// 250 ms doubling to 20 s.
    pub const DEFAULT: Timing = Timing {
        ping_after_ms: 30_000,
        silent_ms: 60_000,
        steady_ms: 30_000,
        first_retry_ms: 250,
        last_retry_ms: 20_000,
    };
}

impl Default for Timing {
    fn default() -> Self {
        Timing::DEFAULT
    }
}

/// What a tick asks of the socket's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Wait,
    /// Send [`crate::live::PING`].
    Ping,
    /// Nothing has arrived for too long: close the socket, call [`Session::closed`], and
    /// connect again after the delay it gives.
    Reconnect,
}

/// What a closed socket leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    /// Wait this long, then connect again.
    pub after_ms: u64,
    /// Transactions sent and never answered, by id. Unknown, not failed: any may have landed, so
    /// none is sent again; `account_tx` or a read of the transaction says.
    pub unanswered: Vec<u64>,
}

impl Session {
    /// What the socket needs at `now`: a ping every [`Timing::ping_after_ms`], busy or not, or a
    /// reconnect after [`Timing::silent_ms`] of nothing at all.
    pub fn tick(&mut self, now: u64) -> Tick {
        let Link::Open { heard, pinged, .. } = &mut self.link else { return Tick::Wait };
        if now.saturating_sub(*heard) >= self.timing.silent_ms {
            return Tick::Reconnect;
        }
        if now.saturating_sub(*pinged) >= self.timing.ping_after_ms {
            *pinged = now;
            return Tick::Ping;
        }
        Tick::Wait
    }

    /// When a tick next has something to do; `None` while closed.
    #[must_use]
    pub fn due(&self) -> Option<u64> {
        let Link::Open { heard, pinged, .. } = self.link else { return None };
        let ping = pinged.saturating_add(self.timing.ping_after_ms);
        let silent = heard.saturating_add(self.timing.silent_ms);
        Some(ping.min(silent))
    }

    /// The socket closed at `now`, or a connect failed. Gives how long to wait before the next
    /// connect, and the transactions left unanswered. The wait doubles from
    /// [`Timing::first_retry_ms`] to [`Timing::last_retry_ms`] and `random` adds up to a quarter
    /// more, so many clients dropped at once do not return at once. It starts over after a
    /// socket that held ([`Timing::steady_ms`]). There is no last attempt.
    pub fn closed(&mut self, now: u64, random: u64) -> Retry {
        if let Link::Open { since, talked: true, .. } = self.link
            && now.saturating_sub(since) >= self.timing.steady_ms
        {
            self.failures = 0;
        }
        self.link = Link::Closed;
        self.chains.clear();
        self.owed.clear();
        let doubled = 1u64.checked_shl(self.failures).unwrap_or(u64::MAX);
        let step = self.timing.first_retry_ms.saturating_mul(doubled).min(self.timing.last_retry_ms);
        let jitter = random % (step / 4).saturating_add(1);
        self.failures = self.failures.saturating_add(1);
        Retry { after_ms: step.saturating_add(jitter), unanswered: std::mem::take(&mut self.waiting) }
    }
}

#[cfg(test)]
#[path = "tests/timing.rs"]
mod tests;
