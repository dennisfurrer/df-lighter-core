//! An order in decimals, as a trader asks for it, and the [`Order`] it signs as on a market's grid:
//!
//! - limit: good-till-time for 28 days unless it says when it lapses; post-only on request;
//!   immediate-or-cancel with no expiry.
//! - market: immediate-or-cancel, no expiry; its price is the worst it fills at, the reference
//!   (the book's best price on the other side, say) moved 2% against it unless it asks for other
//!   slippage, at most 10%. Or a worst price of its own, held within 10% of a reference if given.
//! - stop-loss and take-profit: fire when the mark crosses the trigger, as a market order bounded
//!   2% past the trigger (immediate-or-cancel), or as a limit (good-till-time); 28-day expiry.
//! - TWAP: market slices every 30 seconds until its end, 5 minutes to 24 hours away, none worse
//!   than its worst price.
//!
//! Sizes are in the base asset, or in the quote (sized at the order's own price, else its
//! trigger, else the reference, and cut down), or the whole position for a reduce-only order. The
//! market's least size and value ([`OrderIntent::minimums`]) bind resting orders only, so an order
//! that takes liquidity (closing a dust position, say) is not held to them. Nothing reads a clock:
//! the caller passes now.

use crate::trade::error::TradeError;
use crate::trade::orders::fit::{Fit, worst_price};
use crate::trade::orders::{MOST_EXPIRY, Order};
use crate::utils::time::{DAY_MS, MAX_ORDER_EXPIRY_MS, MIN_ORDER_EXPIRY_MS, MINUTE_MS, RESTING_MS};
use crate::utils::{ClientOrderIndex, Dec, Grid, MarketId, OrderKind, Side, TimeInForce};

/// A market order's slippage when it asks for none: 2%.
pub const DEFAULT_SLIPPAGE_BPS: u32 = 200;
/// The most slippage a market order takes: 10%, Lighter's own client limit.
pub const MAX_SLIPPAGE_BPS: u32 = 1_000;
/// How far past its trigger a stop or take-profit that fires at market may fill: 2%.
pub const TRIGGER_SLIPPAGE_BPS: u32 = 200;
/// A TWAP runs at least 5 minutes...
pub const TWAP_LEAST_MS: u64 = 5 * MINUTE_MS;
/// ...and at most 24 hours.
pub const TWAP_MOST_MS: u64 = DAY_MS;

/// How much an order is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// In the base asset: 0.01 BTC.
    Base(Dec),
    /// In the collateral: 500 USDC, sized at the order's price and cut down onto the grid.
    Quote(Dec),
    /// The whole position, whatever it is when the order fills: a reduce-only order with no size
    /// (`base_amount` 0).
    Position,
}

impl From<Dec> for Size {
    fn from(base: Dec) -> Size {
        Size::Base(base)
    }
}

/// Which way a trigger order exits a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Exit {
    /// Fires when the mark moves against the position: a sell's below, a buy's above.
    StopLoss,
    /// Fires when the mark moves for it.
    TakeProfit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pricing {
    Limit(Dec),
    /// The worst price comes from the reference and the slippage.
    Market,
    MarketAt(Dec),
    Trigger {
        exit: Exit,
        trigger: Dec,
        limit: Option<Dec>,
    },
    Twap {
        worst: Dec,
        until: u64,
    },
}

/// An order before it is on the grid. Build one with a constructor, adjust it, then
/// [`OrderIntent::build`] it for a market's [`Grid`] at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderIntent {
    market: MarketId,
    side: Side,
    size: Size,
    pricing: Pricing,
    tif: Option<TimeInForce>,
    expiry: Option<u64>,
    slippage_bps: Option<u32>,
    reference: Option<Dec>,
    reduce_only: bool,
    client_index: Option<ClientOrderIndex>,
    fit: Fit,
    least: Option<(Dec, Dec)>,
}

impl OrderIntent {
    fn new(market: MarketId, side: Side, size: Size, pricing: Pricing) -> OrderIntent {
        OrderIntent {
            market,
            side,
            size,
            pricing,
            tif: None,
            expiry: None,
            slippage_bps: None,
            reference: None,
            reduce_only: false,
            client_index: None,
            fit: Fit::Exact,
            least: None,
        }
    }

    /// A limit at `price`, resting good-till-time for 28 days.
    pub fn limit(market: MarketId, side: Side, size: impl Into<Size>, price: Dec) -> OrderIntent {
        OrderIntent::new(market, side, size.into(), Pricing::Limit(price))
    }

    /// A market order bounded [`DEFAULT_SLIPPAGE_BPS`] past `reference` (the best price on the
    /// other side of the book, or the mark) unless [`OrderIntent::slippage_bps`] says otherwise.
    pub fn market(market: MarketId, side: Side, size: impl Into<Size>, reference: Dec) -> OrderIntent {
        let mut intent = OrderIntent::new(market, side, size.into(), Pricing::Market);
        intent.reference = Some(reference);
        intent
    }

    /// A market order that fills no worse than `worst`.
    pub fn market_at(market: MarketId, side: Side, size: impl Into<Size>, worst: Dec) -> OrderIntent {
        OrderIntent::new(market, side, size.into(), Pricing::MarketAt(worst))
    }

    /// A stop that fires at market when the mark crosses `trigger`, filling no worse than
    /// [`TRIGGER_SLIPPAGE_BPS`] past it.
    pub fn stop_loss(market: MarketId, side: Side, size: impl Into<Size>, trigger: Dec) -> OrderIntent {
        OrderIntent::trigger(market, side, size.into(), Exit::StopLoss, trigger, None)
    }

    /// A stop that places a limit at `limit` when the mark crosses `trigger`.
    pub fn stop_loss_limit(
        market: MarketId,
        side: Side,
        size: impl Into<Size>,
        trigger: Dec,
        limit: Dec,
    ) -> OrderIntent {
        OrderIntent::trigger(market, side, size.into(), Exit::StopLoss, trigger, Some(limit))
    }

    /// A take-profit that fires at market when the mark crosses `trigger`.
    pub fn take_profit(market: MarketId, side: Side, size: impl Into<Size>, trigger: Dec) -> OrderIntent {
        OrderIntent::trigger(market, side, size.into(), Exit::TakeProfit, trigger, None)
    }

    /// A take-profit that places a limit at `limit` when the mark crosses `trigger`.
    pub fn take_profit_limit(
        market: MarketId,
        side: Side,
        size: impl Into<Size>,
        trigger: Dec,
        limit: Dec,
    ) -> OrderIntent {
        OrderIntent::trigger(market, side, size.into(), Exit::TakeProfit, trigger, Some(limit))
    }

    /// A stop or take-profit, at market (`limit` `None`) or as a limit.
    pub fn trigger(
        market: MarketId,
        side: Side,
        size: impl Into<Size>,
        exit: Exit,
        trigger: Dec,
        limit: Option<Dec>,
    ) -> OrderIntent {
        OrderIntent::new(market, side, size.into(), Pricing::Trigger { exit, trigger, limit })
    }

    /// A TWAP: market slices every 30 seconds until `until` (unix milliseconds, 5 minutes to 24
    /// hours away), none filling worse than `worst`.
    pub fn twap(market: MarketId, side: Side, size: impl Into<Size>, worst: Dec, until: u64) -> OrderIntent {
        OrderIntent::new(market, side, size.into(), Pricing::Twap { worst, until })
    }

    /// Rests, and is cancelled rather than take liquidity. For a limit (or a stop's or
    /// take-profit's limit).
    #[must_use]
    pub const fn post_only(mut self) -> OrderIntent {
        self.tif = Some(TimeInForce::PostOnly);
        self
    }

    /// Fills what it can at once and cancels the rest, with no expiry. For a limit.
    #[must_use]
    pub const fn ioc(mut self) -> OrderIntent {
        self.tif = Some(TimeInForce::ImmediateOrCancel);
        self
    }

    /// Rests until filled, cancelled or its expiry: a limit's default.
    #[must_use]
    pub const fn good_till_time(mut self) -> OrderIntent {
        self.tif = Some(TimeInForce::GoodTillTime);
        self
    }

    /// Lapses at `at`, unix milliseconds 5 minutes to 30 days after the time it is built at,
    /// rather than 28 days after it. Not for an order that fills at once.
    #[must_use]
    pub const fn expires_at(mut self, at: u64) -> OrderIntent {
        self.expiry = Some(at);
        self
    }

    /// Only ever shrinks the position.
    #[must_use]
    pub const fn reduce_only(mut self) -> OrderIntent {
        self.reduce_only = true;
        self
    }

    /// Placed under `index`, to cancel or modify it by ([`super::ClientIndices`] hands them out).
    #[must_use]
    pub const fn client_index(mut self, index: ClientOrderIndex) -> OrderIntent {
        self.client_index = Some(index);
        self
    }

    /// How far past its reference (a market order) or its trigger (a stop or take-profit at
    /// market) it may fill, in basis points; at most [`MAX_SLIPPAGE_BPS`].
    #[must_use]
    pub const fn slippage_bps(mut self, bps: u32) -> OrderIntent {
        self.slippage_bps = Some(bps);
        self
    }

    /// A price to hold the order against: a market order's worst price must be within 10% of it,
    /// and a quote-sized order with no price of its own is sized at it.
    #[must_use]
    pub const fn reference(mut self, reference: Dec) -> OrderIntent {
        self.reference = Some(reference);
        self
    }

    /// How a size or price finer than the grid is put on it: [`Fit::Exact`] (refused) unless set.
    #[must_use]
    pub const fn fit(mut self, fit: Fit) -> OrderIntent {
        self.fit = fit;
        self
    }

    /// The market's least size and least value (`min_base_amount`, `min_quote_amount` in
    /// `orderBookDetails`), which a resting order must meet.
    #[must_use]
    pub const fn minimums(mut self, least_size: Dec, least_value: Dec) -> OrderIntent {
        self.least = Some((least_size, least_value));
        self
    }

    #[must_use]
    pub const fn market_id(&self) -> MarketId {
        self.market
    }

    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }

    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The order on `grid` at `now_ms`, checked as lighter-go checks it.
    pub fn build(&self, grid: Grid, now_ms: u64) -> Result<Order, TradeError> {
        let (kind, time_in_force) = self.kind()?;
        self.check_slippage()?;
        let fit = self.fit;
        let (price, trigger) = match self.pricing {
            Pricing::Limit(price) => (fit.price(grid, price, self.side)?, None),
            Pricing::Market => {
                let reference = self.reference.ok_or(TradeError::NoPrice("a market order"))?;
                let bps = self.slippage_bps.unwrap_or(DEFAULT_SLIPPAGE_BPS);
                (fit_bound(grid, self.side, reference, bps)?, None)
            }
            Pricing::MarketAt(worst) => {
                let price = fit.price(grid, worst, self.side)?;
                if let Some(reference) = self.reference {
                    let most = worst_price(grid, self.side, reference, MAX_SLIPPAGE_BPS)?;
                    let worst = grid.price_of(price);
                    let too_far = match self.side {
                        Side::Bid => worst > most,
                        Side::Ask => worst < most,
                    };
                    if too_far {
                        return Err(TradeError::TooFar { worst, reference });
                    }
                }
                (price, None)
            }
            Pricing::Trigger { trigger, limit, .. } => {
                let trigger = fit.trigger(grid, trigger)?;
                let price = match limit {
                    Some(limit) => fit.price(grid, limit, self.side)?,
                    None => {
                        let bps = self.slippage_bps.unwrap_or(TRIGGER_SLIPPAGE_BPS);
                        fit_bound(grid, self.side, grid.price_of(trigger), bps)?
                    }
                };
                (price, Some(trigger))
            }
            Pricing::Twap { worst, .. } => (fit.price(grid, worst, self.side)?, None),
        };
        let base_amount = self.base_amount(grid, price, trigger)?;
        let order = Order {
            market: self.market,
            client_index: self.client_index,
            base_amount,
            price,
            side: self.side,
            kind,
            time_in_force,
            reduce_only: self.reduce_only,
            trigger_price: trigger.unwrap_or(0),
            expiry: 0,
        };
        let order = Order { expiry: self.expiry(&order, now_ms)?, ..order };
        self.check_minimums(&order, grid)?;
        order.check()?;
        Ok(order)
    }

    /// The kind and time in force; a time in force the kind does not take is refused.
    fn kind(&self) -> Result<(OrderKind, TimeInForce), TradeError> {
        let only = |tif: TimeInForce, why: &'static str| match self.tif {
            None => Ok(tif),
            Some(asked) if asked == tif => Ok(tif),
            Some(_) => Err(TradeError::NotFor(why)),
        };
        Ok(match self.pricing {
            Pricing::Limit(_) => (OrderKind::Limit, self.tif.unwrap_or(TimeInForce::GoodTillTime)),
            Pricing::Market | Pricing::MarketAt(_) => {
                (OrderKind::Market, only(TimeInForce::ImmediateOrCancel, "a market order is immediate-or-cancel")?)
            }
            Pricing::Trigger { exit, limit: None, .. } => {
                let kind = match exit {
                    Exit::StopLoss => OrderKind::StopLoss,
                    Exit::TakeProfit => OrderKind::TakeProfit,
                };
                (kind, only(TimeInForce::ImmediateOrCancel, "a stop or take-profit at market is immediate-or-cancel")?)
            }
            Pricing::Trigger { exit, limit: Some(_), .. } => {
                let kind = match exit {
                    Exit::StopLoss => OrderKind::StopLossLimit,
                    Exit::TakeProfit => OrderKind::TakeProfitLimit,
                };
                (kind, self.tif.unwrap_or(TimeInForce::GoodTillTime))
            }
            Pricing::Twap { .. } => (OrderKind::Twap, only(TimeInForce::GoodTillTime, "a TWAP is good-till-time")?),
        })
    }

    /// A slippage only where there is something to slip from, and at most 10%.
    fn check_slippage(&self) -> Result<(), TradeError> {
        let Some(bps) = self.slippage_bps else { return Ok(()) };
        match self.pricing {
            Pricing::Market | Pricing::Trigger { limit: None, .. } => {}
            _ => return Err(TradeError::NotFor("slippage is for a market order or a stop or take-profit at market")),
        }
        match bps > MAX_SLIPPAGE_BPS {
            true => Err(TradeError::Slippage(bps)),
            false => Ok(()),
        }
    }

    /// `base_amount`: the size on the grid, a quote sized at the order's price (else its trigger,
    /// else the reference), or none for the whole position.
    fn base_amount(&self, grid: Grid, price: u32, trigger: Option<u32>) -> Result<u64, TradeError> {
        match self.size {
            Size::Base(size) => self.fit.size(grid, size),
            Size::Quote(quote) => {
                let at = match self.pricing {
                    Pricing::Market => self.reference.ok_or(TradeError::NoPrice("a quote-sized market order"))?,
                    Pricing::Trigger { limit: None, .. } => grid.price_of(trigger.unwrap_or(price)),
                    _ => grid.price_of(price),
                };
                self.fit.size_for(grid, quote, at)
            }
            Size::Position if self.reduce_only => Ok(0),
            Size::Position => Err(TradeError::NotFor("the whole position is a size for a reduce-only order")),
        }
    }

    /// The expiry its kind takes: none for an order that fills at once, the TWAP's end, the asked
    /// one inside the venue's window, else 28 days from now.
    fn expiry(&self, order: &Order, now_ms: u64) -> Result<u64, TradeError> {
        let immediate = matches!(order.kind, OrderKind::Market)
            || matches!((order.kind, order.time_in_force), (OrderKind::Limit, TimeInForce::ImmediateOrCancel));
        if immediate {
            return match self.expiry {
                None => Ok(0),
                Some(_) => Err(TradeError::NotFor("an order that fills at once has no expiry")),
            };
        }
        if let Pricing::Twap { until, .. } = self.pricing {
            if self.expiry.is_some() {
                return Err(TradeError::NotFor("a TWAP lapses when it ends"));
            }
            return within("OrderExpiry", "a TWAP's end", until, now_ms, TWAP_LEAST_MS, TWAP_MOST_MS);
        }
        match self.expiry {
            Some(at) => within("OrderExpiry", "an expiry", at, now_ms, MIN_ORDER_EXPIRY_MS, MAX_ORDER_EXPIRY_MS),
            None => Ok(now_ms.saturating_add(RESTING_MS)),
        }
    }

    /// A resting order's size and value against the market's least.
    fn check_minimums(&self, order: &Order, grid: Grid) -> Result<(), TradeError> {
        let Some((least_size, least_value)) = self.least else { return Ok(()) };
        if order.takes() || order.base_amount == 0 {
            return Ok(());
        }
        let size = order.size(grid);
        if size < least_size {
            return Err(TradeError::BelowLeastSize { size, least: least_size });
        }
        let value = size.checked_mul(order.limit(grid)).ok_or(TradeError::NoPrice("the order's value"))?;
        if value < least_value {
            return Err(TradeError::BelowLeastValue { value, least: least_value });
        }
        Ok(())
    }
}

/// A worst price off `reference`, as the integer it signs as.
fn fit_bound(grid: Grid, side: Side, reference: Dec, bps: u32) -> Result<u32, TradeError> {
    let bound = worst_price(grid, side, reference, bps)?;
    crate::trade::error::units("Price", grid.price_units(bound))
}

/// `at` when it is `least_ms` to `most_ms` after `now`; else refused, naming `field`.
pub(crate) fn within(
    field: &'static str,
    what: &'static str,
    at: u64,
    now: u64,
    least_ms: u64,
    most_ms: u64,
) -> Result<u64, TradeError> {
    match at.checked_sub(now) {
        Some(ahead) if (least_ms..=most_ms).contains(&ahead) && at <= MOST_EXPIRY => Ok(at),
        _ => Err(TradeError::Expiry { field, what, at, now, least_ms, most_ms }),
    }
}

#[cfg(test)]
#[path = "tests/intent.rs"]
mod tests;
