//! `CreateGroupedOrders` (28): up to three orders on one market, signed and counted as one
//! transaction.
//!
//! - OTO: an entry and one child, a stop or a take-profit the venue places once the entry fills.
//! - OTOCO: an entry and two children, a take-profit and a stop; one filling cancels the other.
//! - OCO: a take-profit and a stop on an open position, both reduce-only; one filling cancels the
//!   other. With no size they cover the whole position, whatever it grows or shrinks to.
//!
//! The entry is a market order or a limit. A child is a stop or take-profit (at market or as a
//! limit) on the other side, with no size of its own: it takes the entry's. The children of one
//! group lapse together, and with the entry when it rests.

use crate::trade::error::TradeError;
use crate::trade::orders::intent::{Exit, OrderIntent, Size};
use crate::trade::orders::{Fit, Order};
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::poseidon2::{hash_no_pad, hash_two_to_one};
use crate::utils::sign::tx::{Attributes, BadTx, Tx, TxType, element, number};
use crate::utils::{Dec, Grid, OrderKind};

/// The most orders in one group.
pub const MOST_IN_GROUP: usize = 3;

/// How the orders of a group hang together (lighter-go's `GroupingType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grouping {
    /// One triggers the other: an entry, then one child once it fills.
    Oto,
    /// One cancels the other: a take-profit and a stop, the first to fill cancelling the other.
    Oco,
    /// One triggers a one-cancels-the-other: an entry, then a take-profit and a stop.
    Otoco,
}

impl Grouping {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Grouping::Oto => 1,
            Grouping::Oco => 2,
            Grouping::Otoco => 3,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<Grouping> {
        match code {
            1 => Some(Grouping::Oto),
            2 => Some(Grouping::Oco),
            3 => Some(Grouping::Otoco),
            _ => None,
        }
    }
}

/// `CreateGroupedOrders` (28): the orders in the order the grouping reads them, and the
/// attributes they all carry (an integrator and its fees, say).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateGroupedOrders {
    pub grouping: Grouping,
    pub orders: Vec<Order>,
    pub attributes: Attributes,
}

impl CreateGroupedOrders {
    /// An entry and one child placed once it fills. The child's size is cleared: it takes the
    /// entry's.
    #[must_use]
    pub fn oto(entry: Order, child: Order) -> CreateGroupedOrders {
        CreateGroupedOrders::new(Grouping::Oto, vec![entry, sizeless(child)])
    }

    /// An entry, then a take-profit and a stop once it fills, one cancelling the other. Their
    /// sizes are cleared: they take the entry's.
    #[must_use]
    pub fn otoco(entry: Order, take_profit: Order, stop_loss: Order) -> CreateGroupedOrders {
        CreateGroupedOrders::new(Grouping::Otoco, vec![entry, sizeless(take_profit), sizeless(stop_loss)])
    }

    /// A take-profit and a stop on an open position, one cancelling the other: both reduce-only,
    /// on the same side, of the same size (none for the whole position) and expiry.
    #[must_use]
    pub fn oco(take_profit: Order, stop_loss: Order) -> CreateGroupedOrders {
        CreateGroupedOrders::new(Grouping::Oco, vec![take_profit, stop_loss])
    }

    fn new(grouping: Grouping, orders: Vec<Order>) -> CreateGroupedOrders {
        CreateGroupedOrders { grouping, orders, attributes: Attributes::NONE }
    }

    /// The same group carrying `attributes`.
    #[must_use]
    pub fn with(mut self, attributes: Attributes) -> CreateGroupedOrders {
        self.attributes = attributes;
        self
    }

    /// `entry` with a take-profit at `take_profit` and a stop at `stop_loss` (one at least), each
    /// firing at market on the other side, reduce-only, filling no worse than 2% past its
    /// trigger: OTO with one, OTOCO with both. The children lapse with a resting entry, else 28
    /// days from `now_ms`. Triggers go on `grid` as `fit` says. An entry that is reduce-only has
    /// no position to protect and is refused.
    pub fn bracket(
        entry: Order,
        take_profit: Option<Dec>,
        stop_loss: Option<Dec>,
        grid: Grid,
        now_ms: u64,
        fit: Fit,
    ) -> Result<CreateGroupedOrders, TradeError> {
        if entry.reduce_only {
            return Err(TradeError::NotFor(
                "a take-profit and a stop go with an entry that opens, not a reduce-only one",
            ));
        }
        let child = |exit: Exit, trigger: Dec| -> Result<Order, TradeError> {
            let intent = OrderIntent::trigger(entry.market, entry.side.opposite(), Size::Position, exit, trigger, None)
                .reduce_only()
                .fit(fit);
            let order = intent.build(grid, now_ms)?;
            Ok(Order { expiry: if entry.expiry != 0 { entry.expiry } else { order.expiry }, ..order })
        };
        let group = match (take_profit, stop_loss) {
            (Some(take_profit), Some(stop_loss)) => CreateGroupedOrders::otoco(
                entry,
                child(Exit::TakeProfit, take_profit)?,
                child(Exit::StopLoss, stop_loss)?,
            ),
            (Some(take_profit), None) => CreateGroupedOrders::oto(entry, child(Exit::TakeProfit, take_profit)?),
            (None, Some(stop_loss)) => CreateGroupedOrders::oto(entry, child(Exit::StopLoss, stop_loss)?),
            (None, None) => return Err(TradeError::NotFor("a bracket needs a take-profit or a stop")),
        };
        group.check()?;
        Ok(group)
    }

    /// One cancels the other.
    fn check_oco(&self) -> Result<(), BadTx> {
        let [first, second] = self.orders.as_slice() else {
            return Err(size_invalid());
        };
        if first.base_amount != second.base_amount {
            return Err(BadTx::new("BaseAmount", "both orders of an OCO have the same size"));
        }
        if first.side != second.side {
            return Err(BadTx::new("IsAsk", "both orders of an OCO are on the same side"));
        }
        if !first.reduce_only || !second.reduce_only {
            return Err(BadTx::new("ReduceOnly", "both orders of an OCO are reduce-only"));
        }
        if first.expiry != second.expiry {
            return Err(BadTx::new("OrderExpiry", "both orders of an OCO lapse together"));
        }
        siblings(first, second)
    }

    /// One triggers the other.
    fn check_oto(&self) -> Result<(), BadTx> {
        let [entry, child] = self.orders.as_slice() else {
            return Err(size_invalid());
        };
        if child.base_amount != 0 {
            return Err(BadTx::new("BaseAmount", "a child has no size: it takes the entry's"));
        }
        if entry.side == child.side {
            return Err(BadTx::new("IsAsk", "a child is on the other side of its entry"));
        }
        if entry.expiry != 0 && entry.expiry != child.expiry {
            return Err(BadTx::new("OrderExpiry", "a child lapses with a resting entry"));
        }
        parent(entry)?;
        child_rules(child)
    }

    /// One triggers a one-cancels-the-other.
    fn check_otoco(&self) -> Result<(), BadTx> {
        let [entry, first, second] = self.orders.as_slice() else {
            return Err(size_invalid());
        };
        if first.base_amount != 0 || second.base_amount != 0 {
            return Err(BadTx::new("BaseAmount", "a child has no size: it takes the entry's"));
        }
        if entry.side == first.side || entry.side == second.side {
            return Err(BadTx::new("IsAsk", "a child is on the other side of its entry"));
        }
        if first.expiry != second.expiry {
            return Err(BadTx::new("OrderExpiry", "the children lapse together"));
        }
        if entry.expiry != 0 && entry.expiry != first.expiry {
            return Err(BadTx::new("OrderExpiry", "the children lapse with a resting entry"));
        }
        parent(entry)?;
        siblings(first, second)
    }
}

/// A child with its size cleared.
const fn sizeless(child: Order) -> Order {
    Order { base_amount: 0, ..child }
}

fn size_invalid() -> BadTx {
    BadTx::new("Orders", "the grouping does not take this many orders")
}

/// An entry: a market order or a limit, by a lone order's rules.
fn parent(order: &Order) -> Result<(), BadTx> {
    match order.kind {
        OrderKind::Market | OrderKind::Limit => order.check(),
        _ => Err(BadTx::new("Type", "an entry is a market order or a limit")),
    }
}

/// A child: a stop or a take-profit, at market or as a limit.
fn child_rules(order: &Order) -> Result<(), BadTx> {
    match order.kind.is_trigger() {
        true => order.check_trigger(),
        false => Err(BadTx::new("Type", "a child is a stop or a take-profit")),
    }
}

/// A take-profit and a stop, one of each.
fn siblings(first: &Order, second: &Order) -> Result<(), BadTx> {
    child_rules(first)?;
    child_rules(second)?;
    let stop = |order: &Order| matches!(order.kind, OrderKind::StopLoss | OrderKind::StopLossLimit);
    let profit = |order: &Order| matches!(order.kind, OrderKind::TakeProfit | OrderKind::TakeProfitLimit);
    match (stop(first) || stop(second)) && (profit(first) || profit(second)) {
        true => Ok(()),
        false => Err(BadTx::new("Type", "the two are a take-profit and a stop")),
    }
}

impl Tx for CreateGroupedOrders {
    fn tx_type(&self) -> TxType {
        TxType::CreateGroupedOrders
    }

    /// The grouping, then the orders folded into one four-element hash: the first order's own,
    /// each next one hashed in with the running one.
    fn hash_fields(&self) -> Vec<Fp> {
        let folded = self
            .orders
            .iter()
            .map(|order| hash_no_pad(&order.hash_fields()))
            .reduce(hash_two_to_one)
            .unwrap_or([Fp::ZERO; 4]);
        let mut fields = vec![element(self.grouping.code())];
        fields.extend(folded);
        fields
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        let orders = self
            .orders
            .iter()
            .map(|order| Json::Object(order.info().into_iter().map(|(key, value)| (key.to_owned(), value)).collect()))
            .collect();
        vec![("GroupingType", number(self.grouping.code())), ("Orders", Json::Array(orders))]
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }

    /// lighter-go's `L2CreateGroupedOrdersTxInfo.Validate`: the attributes; one to three orders on
    /// one market, client indexes unique, each order's own fields; then the grouping's rules.
    fn check(&self) -> Result<(), BadTx> {
        self.attributes.check()?;
        let Some(first) = self.orders.first().filter(|_| self.orders.len() <= MOST_IN_GROUP) else {
            return Err(size_invalid());
        };
        let mut indexes = Vec::with_capacity(self.orders.len());
        for order in &self.orders {
            if order.market != first.market {
                return Err(BadTx::new("MarketIndex", "every order of a group is on one market"));
            }
            if let Some(index) = order.client_index {
                if indexes.contains(&index) {
                    return Err(BadTx::new("ClientOrderIndex", "each order of a group has its own client index"));
                }
                indexes.push(index);
            }
            order.check_fields()?;
        }
        match self.grouping {
            Grouping::Oco => self.check_oco(),
            Grouping::Oto => self.check_oto(),
            Grouping::Otoco => self.check_otoco(),
        }
    }
}

#[cfg(test)]
#[path = "tests/grouped.rs"]
mod tests;
