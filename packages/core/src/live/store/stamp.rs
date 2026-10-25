//! When each piece was last told and last changed, in the milliseconds the caller hands in.

use std::ops::Deref;

/// A piece of state, when the venue last told it (`heard`) and when what it said last differed
/// (`changed`). Staleness is read from `heard`: a book nobody trades stays the same while it is
/// still being sent, and is not stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamped<T> {
    pub value: T,
    /// Milliseconds, as handed in.
    pub heard: u64,
    /// Milliseconds, as handed in.
    pub changed: u64,
}

impl<T> Stamped<T> {
    /// First told at `now`.
    #[must_use]
    pub const fn new(now: u64, value: T) -> Stamped<T> {
        Stamped { value, heard: now, changed: now }
    }

    /// How long since the venue last told it, at `now`.
    #[must_use]
    pub const fn age(&self, now: u64) -> u64 {
        now.saturating_sub(self.heard)
    }

    /// Told again at `now`, by a change in place that says whether it changed anything. Neither
    /// time goes back.
    pub fn touch(&mut self, now: u64, changed: bool) {
        self.heard = self.heard.max(now);
        if changed {
            self.changed = self.changed.max(now);
        }
    }
}

impl<T: PartialEq> Stamped<T> {
    /// Told `value` at `now`. Says whether it differs from what was held.
    pub fn set(&mut self, now: u64, value: T) -> bool {
        let changed = self.value != value;
        if changed {
            self.value = value;
        }
        self.touch(now, changed);
        changed
    }
}

impl<T> Deref for Stamped<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

/// `value` into a slot that may hold nothing yet, at `now`. Says whether it changed.
pub(crate) fn put<T: PartialEq>(slot: &mut Option<Stamped<T>>, now: u64, value: T) -> bool {
    match slot {
        Some(held) => held.set(now, value),
        None => {
            *slot = Some(Stamped::new(now, value));
            true
        }
    }
}
