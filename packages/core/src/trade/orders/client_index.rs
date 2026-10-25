//! Client order indexes from the clock. An index names an order while it is open, across every
//! market of the account, so two open orders must never share one. The current unix millisecond is
//! one, unless the last handed out is as late, when it is the last plus one: a burst in one
//! millisecond still gets distinct indexes, and they keep rising across a restart as the clock
//! does. Indexes run from 1 to 2^48 - 1; the clock passes that in the year 10889, after which they
//! count on from 1.

use std::sync::{Mutex, PoisonError};

use crate::utils::ClientOrderIndex;
use crate::utils::ids::WIDTH_48;

/// Hands out client order indexes for one account: give each signing client one, or share one
/// (`static INDICES: ClientIndices = ClientIndices::new();`) among every client of an account.
#[derive(Debug, Default)]
pub struct ClientIndices {
    last: Mutex<u64>,
}

impl ClientIndices {
    #[must_use]
    pub const fn new() -> ClientIndices {
        ClientIndices { last: Mutex::new(0) }
    }

    /// The next index at `now_ms`: `now_ms`, or one past the last handed out when that is later.
    pub fn next(&self, now_ms: u64) -> ClientOrderIndex {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let clock = if now_ms <= WIDTH_48 { now_ms } else { 0 };
        let next = clock.max(last.saturating_add(1));
        let next = if next > WIDTH_48 { 1 } else { next };
        *last = next;
        ClientOrderIndex::new(next).unwrap_or(ClientOrderIndex::of::<1>())
    }

    /// Counts `used` (an index placed elsewhere, or read back from open orders after a restart) as
    /// handed out, so the next is past it.
    pub fn observe(&self, used: ClientOrderIndex) {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        *last = (*last).max(used.get());
    }
}

#[cfg(test)]
#[path = "tests/client_index.rs"]
mod tests;
