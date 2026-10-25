//! Request weights against Lighter's rate limit (the docs' "Rate Limits", each venue's own table),
//! the account tier's budget, the last minute's spend, and the gate a 429 or a 405 closes. No
//! clock and no I/O: every call is handed the time.
//!
//! A Standard account may send 60 requests a minute, and no more of a heavy endpoint than a
//! Premium account's 24,000 weight allows: one request costs the larger of 400 and its weight
//! against 24,000 a minute, which says both. Plus, Premium and Builder accounts spend weight.
//! The limits apply per IP, and per L1 address when a request is authenticated.

use std::collections::VecDeque;

use crate::client::network::Network;

/// The window every figure here is over.
pub const WINDOW_MS: u64 = 60_000;

/// The first cooldown a 429 closes the gate for; each strike doubles it.
pub const BASE_COOLDOWN_MS: u64 = 2_000;

/// The longest cooldown a strike sets, though a `Retry-After` may ask longer. A 405 is the
/// firewall's, which holds an IP for this long at once.
pub const MAX_COOLDOWN_MS: u64 = 60_000;

const MAX_STRIKES: u32 = 6;

/// What a Standard request costs at least, against [`STANDARD_BUDGET`]: 60 requests a minute.
pub const STANDARD_FLOOR: u32 = 400;

/// A Standard account's minute, in the units [`STANDARD_FLOOR`] counts.
pub const STANDARD_BUDGET: u32 = 24_000;

/// What the account's tier lets it spend a minute on reads (and, for Standard, on sends too).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tier {
    /// The default: 60 requests a minute, sends included.
    #[default]
    Standard,
    /// Opt-in through `changeAccountTier`: a weighted read budget, sends in a bucket of their own.
    Plus,
    Premium,
    /// By application, for read-heavy integrations.
    Builder,
}

impl Tier {
    /// The weight the tier may spend a minute on reads, on `network`.
    #[must_use]
    pub const fn budget(self, network: Network) -> u32 {
        match (self, network) {
            (Tier::Standard, _) => STANDARD_BUDGET,
            (Tier::Plus, Network::Lighter) | (Tier::Premium, _) => 24_000,
            (Tier::Plus, Network::Robinhood) => 120_000,
            (Tier::Builder, Network::Lighter) => 240_000,
            (Tier::Builder, Network::Robinhood) => 150_000,
        }
    }

    /// What one request of `weight` costs against [`Tier::budget`].
    #[must_use]
    pub const fn cost(self, weight: u32) -> u32 {
        match self {
            Tier::Standard if weight < STANDARD_FLOOR => STANDARD_FLOOR,
            _ => weight,
        }
    }

    /// Whether `sendTx` and `sendTxBatch` spend the same minute as reads: on Standard they do.
    #[must_use]
    pub const fn sends_share_reads(self) -> bool {
        matches!(self, Tier::Standard)
    }

    /// The name `changeAccountTier` takes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Tier::Standard => "standard",
            Tier::Plus => "plus",
            Tier::Premium => "premium",
            Tier::Builder => "builder",
        }
    }
}

/// A path's weight on `network`, from the venue's table. The path is the one under `/api/v1/`.
#[must_use]
pub fn weight(network: Network, path: &str) -> u32 {
    match (path, network) {
        ("sendTx" | "sendTxBatch" | "nextNonce", _) => 6,
        ("tv/history" | "tv/streaming", _) => 10,
        ("publicPools" | "publicPoolsMetadata" | "txFromL1TxHash", _) => 50,
        ("accountInactiveOrders" | "accountActiveOrders" | "accountOrders" | "deposit/latest", _) => 100,
        ("exchangeMetrics" | "executeStats", _) => 120,
        ("apikeys", _) | ("account", Network::Robinhood) => 150,
        ("trades", Network::Lighter) => 200,
        ("transferFeeInfo", _) => 500,
        ("recentTrades", _) | ("trades", Network::Robinhood) => 600,
        (
            "changeAccountTier"
            | "tokens/revoke"
            | "setAccountMetadata"
            | "notification/ack"
            | "createIntentAddress"
            | "fastwithdraw"
            | "referral/create"
            | "referral/use"
            | "referral/update"
            | "referral/kickback/update",
            _,
        ) => 3_000,
        ("tokens/create", Network::Lighter) => 10_000,
        ("tokens/create", Network::Robinhood) => 23_000,
        _ => 300,
    }
}

/// Whether a path sends a transaction: on Plus and Premium it is never held back by the read
/// budget, and on every tier it is never retried.
#[must_use]
pub fn is_send(path: &str) -> bool {
    matches!(path, "sendTx" | "sendTxBatch" | "fastwithdraw" | "litLease")
}

/// What went out over the last minute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spent {
    /// The tier's budget, in the units the minute counts.
    pub budget: u32,
    pub minute: u32,
    /// By path, the heaviest first.
    pub kinds: Vec<Kind>,
    /// 429s and 405s over the last minute.
    pub refused: u32,
    /// How long the gate stays closed; 0 when it is open.
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind {
    pub name: String,
    pub count: u32,
    pub weight: u32,
}

/// The last minute of requests: when each went out, its path and cost; and when each refusal came.
#[derive(Debug, Default)]
pub(crate) struct Meter {
    sent: VecDeque<(u64, String, u32)>,
    refused: VecDeque<u64>,
}

impl Meter {
    /// Drops what is a whole window old at `now`.
    fn prune(&mut self, now: u64) {
        let gone = |at: u64| at.saturating_add(WINDOW_MS) <= now;
        while self.sent.front().is_some_and(|(at, _, _)| gone(*at)) {
            self.sent.pop_front();
        }
        while self.refused.front().is_some_and(|at| gone(*at)) {
            self.refused.pop_front();
        }
    }

    pub(crate) fn record(&mut self, now: u64, kind: &str, cost: u32) {
        self.prune(now);
        self.sent.push_back((now, kind.to_owned(), cost));
    }

    pub(crate) fn refuse(&mut self, now: u64) {
        self.prune(now);
        self.refused.push_back(now);
    }

    /// How long a read costing `need` waits at `now`, so the last minute stays within four fifths
    /// of `budget` (the rest is left to sends and to other clients on the same IP): none while it
    /// fits, else until enough has rolled off.
    pub(crate) fn wait_for(&mut self, now: u64, need: u32, budget: u32) -> Option<u64> {
        self.prune(now);
        let soft = budget / 5 * 4;
        let spent = self.sent.iter().fold(0u32, |sum, (_, _, cost)| sum.saturating_add(*cost));
        let mut over = spent.saturating_add(need).checked_sub(soft).filter(|over| *over > 0)?;
        for (at, _, cost) in &self.sent {
            if *cost >= over {
                return Some(at.saturating_add(WINDOW_MS).saturating_sub(now).max(1));
            }
            over -= cost;
        }
        Some(WINDOW_MS)
    }

    /// The last minute at `now`, with no cooldown: the [`Gate`] knows that.
    pub(crate) fn spent(&mut self, now: u64, budget: u32) -> Spent {
        self.prune(now);
        let mut kinds: Vec<Kind> = Vec::new();
        for (_, name, cost) in &self.sent {
            match kinds.iter_mut().find(|kind| kind.name == *name) {
                Some(kind) => {
                    kind.count += 1;
                    kind.weight = kind.weight.saturating_add(*cost);
                }
                None => kinds.push(Kind { name: name.clone(), count: 1, weight: *cost }),
            }
        }
        kinds.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.name.cmp(&b.name)));
        let minute = kinds.iter().fold(0u32, |sum, kind| sum.saturating_add(kind.weight));
        Spent { budget, minute, kinds, refused: u32::try_from(self.refused.len()).unwrap_or(u32::MAX), cooldown_ms: 0 }
    }
}

/// What a request carries out through the [`Gate`]: how many strikes there had been when it went
/// out, so a burst sent before a 429 counts once.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ticket(u64);

/// Closed by a 429 or a 405 for every request, reads and sends alike: the venue limits by IP, and
/// a refusal on REST holds the websocket too.
#[derive(Debug, Default)]
pub(crate) struct Gate {
    until: u64,
    /// Consecutive strikes, a burst counting once.
    strikes: u32,
    /// Strikes ever.
    epoch: u64,
}

impl Gate {
    /// A ticket out, or how long the gate stays closed.
    pub(crate) fn open(&self, now: u64) -> Result<Ticket, u64> {
        match self.until > now {
            true => Err(self.until - now),
            false => Ok(Ticket(self.epoch)),
        }
    }

    /// The request was answered: room again, unless a refusal struck since it went out.
    pub(crate) fn passed(&mut self, ticket: Ticket) {
        if ticket.0 == self.epoch {
            self.strikes = 0;
        }
    }

    /// A 429 came back at `now`. The first of a burst strikes: two seconds, doubling to a minute,
    /// or a longer `Retry-After`. The rest of the burst only moves the gate for a longer
    /// `Retry-After`. Gives the wait.
    pub(crate) fn struck(&mut self, ticket: Ticket, now: u64, retry_after_ms: Option<u64>) -> u64 {
        let wait = match ticket.0 == self.epoch {
            true => {
                self.epoch += 1;
                self.strikes = (self.strikes + 1).min(MAX_STRIKES);
                let backoff = (BASE_COOLDOWN_MS << (self.strikes - 1)).min(MAX_COOLDOWN_MS);
                backoff.max(retry_after_ms.unwrap_or(0))
            }
            false => retry_after_ms.unwrap_or(0),
        };
        self.until = self.until.max(now.saturating_add(wait));
        self.until.saturating_sub(now)
    }

    /// The firewall's 405 at `now`: a whole minute, at once.
    pub(crate) fn blocked(&mut self, ticket: Ticket, now: u64) -> u64 {
        self.struck(ticket, now, Some(MAX_COOLDOWN_MS))
    }

    #[must_use]
    pub(crate) const fn cooldown_ms(&self, now: u64) -> u64 {
        self.until.saturating_sub(now)
    }
}

#[cfg(test)]
#[path = "tests/limit.rs"]
mod tests;
