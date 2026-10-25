//! `CancelAllOrders` (16): every open order of the account, at once or at a time.
//!
//! - Now, on every market (perps, spot and binary options alike), or on one market alone: the
//!   market rides as attribute 5, and only an immediate cancel-all may carry it.
//! - At a time, unix milliseconds 5 minutes to 15 days ahead: a dead man's switch, cancelling
//!   everything if the program that scheduled it stops renewing it. The venue keeps one queued
//!   at a time (code 21712 answers another while one is), so a renewal aborts the queued one
//!   first.
//! - Abort: the queued one is dropped.

use crate::trade::error::TradeError;
use crate::trade::orders::intent::within;
use crate::utils::MarketId;
use crate::utils::ids::NO_MARKET;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::time::{DAY_MS, MINUTE_MS};

/// A scheduled cancel-all is at least 5 minutes ahead...
pub const CANCEL_ALL_LEAST_MS: u64 = 5 * MINUTE_MS;
/// ...and at most 15 days.
pub const CANCEL_ALL_MOST_MS: u64 = 15 * DAY_MS;

/// The latest time lighter-go takes: an `int64` of milliseconds.
const MOST_TIME: u64 = i64::MAX.unsigned_abs();

/// The attribute that limits a cancel-all to one market.
const CANCEL_ALL_MARKET: u8 = 5;

/// When a cancel-all runs (lighter-go's cancel-all `TimeInForce` and `Time`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancelAllTime {
    /// Now.
    Now,
    /// At this unix millisecond, unless aborted or replaced first.
    At(u64),
    /// Drops the scheduled one.
    Abort,
}

impl CancelAllTime {
    /// `TimeInForce` as the transaction signs it.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            CancelAllTime::Now => 0,
            CancelAllTime::At(_) => 1,
            CancelAllTime::Abort => 2,
        }
    }

    /// `Time` as the transaction signs it: 0 but for a scheduled one.
    #[must_use]
    pub const fn time(self) -> u64 {
        match self {
            CancelAllTime::At(at) => at,
            CancelAllTime::Now | CancelAllTime::Abort => 0,
        }
    }
}

/// `CancelAllOrders` (16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelAllOrders {
    pub when: CancelAllTime,
    pub attributes: Attributes,
}

impl CancelAllOrders {
    /// Every open order on every market, now.
    #[must_use]
    pub const fn now() -> CancelAllOrders {
        CancelAllOrders { when: CancelAllTime::Now, attributes: Attributes::NONE }
    }

    /// Every open order on `market`, now. A program that trades one kind of market (perps, say)
    /// cancels its orders with one of these for each market it has orders on, never touching the
    /// others ([`CancelAllOrders::in_markets`]).
    #[must_use]
    pub const fn in_market(market: MarketId) -> CancelAllOrders {
        CancelAllOrders { when: CancelAllTime::Now, attributes: Attributes::NONE.cancel_market(market) }
    }

    /// One [`CancelAllOrders::in_market`] for each of `markets`, each once: what `sendTxBatch`
    /// takes (at most fifty) to cancel every order of one kind of market and nothing else.
    #[must_use]
    pub fn in_markets(markets: &[MarketId]) -> Vec<CancelAllOrders> {
        let mut seen: Vec<MarketId> = Vec::with_capacity(markets.len());
        for market in markets {
            if !seen.contains(market) {
                seen.push(*market);
            }
        }
        seen.into_iter().map(CancelAllOrders::in_market).collect()
    }

    /// Every open order at `at`, unix milliseconds, as signed: the venue takes 5 minutes to 15
    /// days ahead ([`CancelAllOrders::scheduled`] holds it to that).
    #[must_use]
    pub const fn at(at: u64) -> CancelAllOrders {
        CancelAllOrders { when: CancelAllTime::At(at), attributes: Attributes::NONE }
    }

    /// Every open order at `at`, refused unless it is 5 minutes to 15 days after `now_ms`.
    pub fn scheduled(at: u64, now_ms: u64) -> Result<CancelAllOrders, TradeError> {
        within("Time", "a scheduled cancel-all", at, now_ms, CANCEL_ALL_LEAST_MS, CANCEL_ALL_MOST_MS)
            .map(CancelAllOrders::at)
    }

    /// Drops the scheduled cancel-all.
    #[must_use]
    pub const fn abort() -> CancelAllOrders {
        CancelAllOrders { when: CancelAllTime::Abort, attributes: Attributes::NONE }
    }

    /// The same cancel-all carrying `attributes`. A market it is limited to rides here too, so
    /// add to [`CancelAllOrders::in_market`]'s rather than replace them.
    #[must_use]
    pub const fn with(mut self, attributes: Attributes) -> CancelAllOrders {
        self.attributes = attributes;
        self
    }

    /// The market it is limited to; `None` for every market.
    #[must_use]
    pub fn market(&self) -> Option<MarketId> {
        self.attributes
            .entries()
            .into_iter()
            .find(|(kind, value)| *kind == CANCEL_ALL_MARKET && *value != NO_MARKET)
            .and_then(|(_, value)| MarketId::new(value).ok())
    }
}

impl Tx for CancelAllOrders {
    fn tx_type(&self) -> TxType {
        TxType::CancelAllOrders
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.when.code()), element(self.when.time())]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("TimeInForce", number(self.when.code())), ("Time", number(self.when.time()))]
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2CancelAllOrdersTxInfo.Validate`: the attributes; a market only on an
    /// immediate one; a scheduled one's time a positive `int64`.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()?;
        if self.market().is_some() && !matches!(self.when, CancelAllTime::Now) {
            return Err(BadTx::new(
                "TimeInForce",
                "a cancel-all limited to one market runs now; it can't be scheduled or abort one",
            ));
        }
        if let CancelAllTime::At(at) = self.when
            && (at == 0 || at > MOST_TIME)
        {
            return Err(BadTx::new("Time", "a scheduled cancel-all's time is unix milliseconds above 0"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/cancel_all.rs"]
mod tests;
