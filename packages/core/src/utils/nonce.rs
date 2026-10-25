//! One API key's nonces. The venue counts them per key, strictly up by one (unless a transaction
//! asks to skip), so the count is fetched once from `nextNonce` and kept here. A send whose fate is
//! unknown forgets it, and the next send asks the venue again.

/// The next nonce of one key, while it is known.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NonceCounter {
    next: Option<u64>,
}

impl NonceCounter {
    /// Nothing known: the first send asks `nextNonce`.
    #[must_use]
    pub const fn unknown() -> Self {
        NonceCounter { next: None }
    }

    /// Starting at `next`, as `nextNonce` answered.
    #[must_use]
    pub const fn at(next: u64) -> Self {
        NonceCounter { next: Some(next) }
    }

    /// The next nonce, if known.
    #[must_use]
    pub const fn next(&self) -> Option<u64> {
        self.next
    }

    /// Hands out the next nonce and counts past it; `None` when it must be asked for.
    pub fn take(&mut self) -> Option<u64> {
        let nonce = self.next?;
        self.next = Some(nonce.saturating_add(1));
        Some(nonce)
    }

    /// The venue's answer, to count from.
    pub fn set(&mut self, next: u64) {
        self.next = Some(next);
    }

    /// The count may be wrong (a send's fate is unknown, or the venue refused a nonce): ask again.
    pub fn forget(&mut self) {
        self.next = None;
    }

    /// Hands out `count` nonces in a row, for a batch, and gives the first; `None` when the count
    /// must be asked for.
    pub fn take_run(&mut self, count: u64) -> Option<u64> {
        let first = self.next?;
        self.next = Some(first.saturating_add(count));
        Some(first)
    }

    /// The `count` nonces from `first` were turned away before they counted: hand them out again
    /// if nothing was taken after them, else ask again, since a later nonce now leaves a gap the
    /// venue refuses.
    pub fn give_back(&mut self, first: u64, count: u64) {
        self.next = match self.next == Some(first.saturating_add(count)) {
            true => Some(first),
            false => None,
        };
    }
}

#[cfg(test)]
#[path = "tests/nonce.rs"]
mod tests;
