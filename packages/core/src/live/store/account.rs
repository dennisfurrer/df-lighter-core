//! What the store holds of one account, from its channels: positions, balances, pool shares,
//! live orders, recent trades and funding, executed transactions and its stats. A channel's
//! snapshot replaces what it covers; an update merges in, row by row, and is dropped until a
//! snapshot has been held. An order that is no longer live leaves; a trade already held is not
//! added twice.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};

use crate::account::history::{PositionFunding, Transaction};
use crate::account::holdings::BoPosition;
use crate::account::{Asset, Order, PoolShare, Position};
use crate::live::account::{UserStats, Volumes};
use crate::live::store::Folded;
use crate::live::store::stamp::{Stamped, put};
use crate::live::{Data, Frame};
use crate::markets::Trade;
use crate::utils::{AccountIndex, AssetId, MarketId};

/// Trades, funding payments and transactions kept an account, newest first.
pub const KEEP: usize = 1_000;

/// One account as the venue last told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    index: AccountIndex,
    positions: Option<Stamped<BTreeMap<MarketId, Position>>>,
    bo_positions: Option<Stamped<BTreeMap<MarketId, BoPosition>>>,
    assets: Option<Stamped<BTreeMap<AssetId, Asset>>>,
    shares: Option<Stamped<BTreeMap<AccountIndex, PoolShare>>>,
    /// Live orders, by the venue's index.
    orders: Option<Stamped<BTreeMap<u64, Order>>>,
    trades: Option<Stamped<Vec<Trade>>>,
    fundings: Option<Stamped<Vec<PositionFunding>>>,
    txs: Option<Stamped<Vec<Transaction>>>,
    stats: Option<Stamped<UserStats>>,
    volumes: Option<Stamped<Volumes>>,
}

impl Account {
    #[must_use]
    pub const fn new(index: AccountIndex) -> Account {
        Account {
            index,
            positions: None,
            bo_positions: None,
            assets: None,
            shares: None,
            orders: None,
            trades: None,
            fundings: None,
            txs: None,
            stats: None,
            volumes: None,
        }
    }

    #[must_use]
    pub const fn index(&self) -> AccountIndex {
        self.index
    }

    /// Every market's position the venue listed, flat ones included; `None` until a snapshot.
    #[must_use]
    pub const fn positions(&self) -> Option<&Stamped<BTreeMap<MarketId, Position>>> {
        self.positions.as_ref()
    }

    /// The open positions alone.
    pub fn open_positions(&self) -> impl Iterator<Item = &Position> {
        self.positions.iter().flat_map(|held| held.values()).filter(|position| position.is_open())
    }

    #[must_use]
    pub const fn bo_positions(&self) -> Option<&Stamped<BTreeMap<MarketId, BoPosition>>> {
        self.bo_positions.as_ref()
    }

    #[must_use]
    pub const fn assets(&self) -> Option<&Stamped<BTreeMap<AssetId, Asset>>> {
        self.assets.as_ref()
    }

    /// Pool shares, by pool.
    #[must_use]
    pub const fn shares(&self) -> Option<&Stamped<BTreeMap<AccountIndex, PoolShare>>> {
        self.shares.as_ref()
    }

    /// Live orders on every market followed, by the venue's index.
    #[must_use]
    pub const fn orders(&self) -> Option<&Stamped<BTreeMap<u64, Order>>> {
        self.orders.as_ref()
    }

    /// Newest first, at most [`KEEP`].
    #[must_use]
    pub const fn trades(&self) -> Option<&Stamped<Vec<Trade>>> {
        self.trades.as_ref()
    }

    /// Newest first, at most [`KEEP`].
    #[must_use]
    pub const fn fundings(&self) -> Option<&Stamped<Vec<PositionFunding>>> {
        self.fundings.as_ref()
    }

    /// Executed transactions, newest first, at most [`KEEP`].
    #[must_use]
    pub const fn txs(&self) -> Option<&Stamped<Vec<Transaction>>> {
        self.txs.as_ref()
    }

    #[must_use]
    pub const fn stats(&self) -> Option<&Stamped<UserStats>> {
        self.stats.as_ref()
    }

    #[must_use]
    pub const fn volumes(&self) -> Option<&Stamped<Volumes>> {
        self.volumes.as_ref()
    }

    /// One of this account's frames, at `now`. Dropped when it is not one (another account's, a
    /// channel not kept), or when it updates what no snapshot has brought.
    pub fn apply(&mut self, now: u64, frame: &Frame) -> Folded {
        let Some(data) = frame.data() else { return Folded::Dropped };
        let fold = Fold::of(frame.is_snapshot());
        let start = match fold {
            Fold::Add => Fold::Add,
            _ => Fold::Start,
        };
        let held = match data {
            Data::AccountAll(_) | Data::AccountAllPositions(_) => self.positions.is_some(),
            Data::AccountAllAssets(_) => self.assets.is_some(),
            Data::AccountAllOrders(_) | Data::AccountOrders(_) => self.orders.is_some(),
            Data::AccountMarket(_) => self.orders.is_some() || self.positions.is_some(),
            Data::AccountAllTrades(_) => self.trades.is_some(),
            _ => true,
        };
        if fold == Fold::Add && !held {
            return Folded::Dropped;
        }
        let changed = match data {
            Data::AccountAll(all) if all.account == self.index => {
                let mut changed = self.fold_positions(now, &all.positions, &all.bo_positions, fold, None);
                changed |= self.fold_assets(now, &all.assets, fold);
                changed |= self.fold_shares(now, &all.shares, fold);
                changed |= self.fold_trades(now, &all.trades, fold);
                changed |= fold_rows(&mut self.fundings, now, &all.fundings, fold, funding_order);
                changed | self.fold_volumes(now, all.volumes.as_ref())
            }
            Data::AccountAllPositions(all) if all.account == self.index => {
                let changed = self.fold_positions(now, &all.positions, &all.bo_positions, fold, None);
                changed | self.fold_shares(now, &all.shares, fold)
            }
            Data::AccountAllAssets(all) if all.account == self.index => self.fold_assets(now, &all.assets, fold),
            Data::AccountAllOrders(all) if all.account == self.index => self.fold_orders(now, None, &all.orders, fold),
            Data::AccountOrders(one) if one.account == self.index => {
                self.fold_orders(now, Some(one.market), &one.orders, fold)
            }
            Data::AccountMarket(one) if one.account == self.index => {
                let positions: Vec<Position> = one.position.iter().cloned().collect();
                let bo_positions: Vec<BoPosition> = one.bo_position.iter().cloned().collect();
                let mut changed = self.fold_positions(now, &positions, &bo_positions, fold, Some(one.market));
                changed |= self.fold_assets(now, &one.assets, Fold::Add);
                changed |= self.fold_orders(now, Some(one.market), &one.orders, fold);
                changed |= self.fold_trades(now, &one.trades, start);
                changed | fold_rows(&mut self.fundings, now, &one.fundings, start, funding_order)
            }
            Data::AccountAllTrades(all) if all.account == self.index => {
                let changed = self.fold_trades(now, &all.trades, start);
                changed | self.fold_volumes(now, all.volumes.as_ref())
            }
            Data::AccountTx(txs) if txs.account == self.index => {
                fold_rows(&mut self.txs, now, &txs.txs, start, |tx| (tx.queued_at, tx.sequence_index))
            }
            Data::UserStats(stats) if stats.account == self.index => put(&mut self.stats, now, (**stats).clone()),
            _ => return Folded::Dropped,
        };
        Folded::of(changed)
    }

    /// Positions, and binary option positions; a snapshot of one market replaces that market's.
    fn fold_positions(
        &mut self,
        now: u64,
        positions: &[Position],
        bo: &[BoPosition],
        fold: Fold,
        market: Option<MarketId>,
    ) -> bool {
        let rows = positions.iter().map(|position| (position.market_id, position.clone()));
        let covered = |position: &Position| market.is_none_or(|market| position.market_id == market);
        let changed = fold_map(&mut self.positions, now, rows, fold, Some(&covered));
        let rows = bo.iter().map(|position| (position.market_id, position.clone()));
        let covered = |position: &BoPosition| market.is_none_or(|market| position.market_id == market);
        changed | fold_map(&mut self.bo_positions, now, rows, fold, Some(&covered))
    }

    fn fold_assets(&mut self, now: u64, assets: &[Asset], fold: Fold) -> bool {
        let rows = assets.iter().map(|asset| (asset.asset_id, asset.clone()));
        fold_map(&mut self.assets, now, rows, fold, None)
    }

    fn fold_shares(&mut self, now: u64, shares: &[PoolShare], fold: Fold) -> bool {
        let rows = shares.iter().map(|share| (share.pool, share.clone()));
        fold_map(&mut self.shares, now, rows, fold, None)
    }

    /// Live orders in, ended ones out. A snapshot of one market replaces that market's alone.
    fn fold_orders(&mut self, now: u64, market: Option<MarketId>, orders: &[Order], fold: Fold) -> bool {
        let rows = orders.iter().map(|order| (order.order_index, order.clone()));
        let covered = |order: &Order| market.is_none_or(|market| order.market_index == market);
        let changed = fold_map(&mut self.orders, now, rows, fold, Some(&covered));
        let Some(held) = &mut self.orders else { return changed };
        let before = held.len();
        held.value.retain(|_, order| order.status.is_live());
        let dropped = held.len() != before;
        held.touch(now, dropped);
        changed | dropped
    }

    fn fold_trades(&mut self, now: u64, trades: &[Trade], fold: Fold) -> bool {
        fold_rows(&mut self.trades, now, trades, fold, |trade| (trade.timestamp, trade.trade_id))
    }

    fn fold_volumes(&mut self, now: u64, volumes: Option<&Volumes>) -> bool {
        volumes.is_some_and(|volumes| put(&mut self.volumes, now, volumes.clone()))
    }
}

/// A funding payment's place, newest first, and what tells two apart.
fn funding_order(funding: &PositionFunding) -> (u64, u64, MarketId) {
    (funding.timestamp, funding.funding_id, funding.market_id)
}

/// How rows meet what is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fold {
    /// A snapshot: the rows are the whole.
    Replace,
    /// A snapshot that brings no rows of its own: start from what is held, or nothing.
    Start,
    /// An update: merged into what is held, dropped while nothing is.
    Add,
}

impl Fold {
    const fn of(whole: bool) -> Fold {
        match whole {
            true => Fold::Replace,
            false => Fold::Add,
        }
    }
}

/// Rows keyed by `K` into a slot. A snapshot replaces what it covers: everything, or the rows
/// `covered` says (one market's). An update merges in. Says whether anything changed.
fn fold_map<K: Ord + Clone, V: Clone + PartialEq>(
    slot: &mut Option<Stamped<BTreeMap<K, V>>>,
    now: u64,
    rows: impl Iterator<Item = (K, V)>,
    fold: Fold,
    covered: Option<&dyn Fn(&V) -> bool>,
) -> bool {
    if fold == Fold::Add {
        let Some(held) = slot else { return false };
        let mut changed = false;
        for (key, value) in rows {
            if held.get(&key) != Some(&value) {
                held.value.insert(key, value);
                changed = true;
            }
        }
        held.touch(now, changed);
        return changed;
    }
    let mut map: BTreeMap<K, V> = match (fold, covered, slot.as_ref()) {
        (Fold::Start, _, Some(held)) => held.value.clone(),
        (_, Some(covered), Some(held)) => {
            held.iter().filter(|(_, value)| !covered(value)).map(|(key, value)| (key.clone(), value.clone())).collect()
        }
        _ => BTreeMap::new(),
    };
    map.extend(rows);
    put(slot, now, map)
}

/// Rows into a list kept newest first by `order`, at most [`KEEP`]; one already held (the same
/// key) is not added again.
fn fold_rows<T: Clone + PartialEq, K: Ord + Copy + std::hash::Hash>(
    slot: &mut Option<Stamped<Vec<T>>>,
    now: u64,
    rows: &[T],
    fold: Fold,
    order: impl Fn(&T) -> K,
) -> bool {
    let mut list = match (fold, slot.as_ref()) {
        (Fold::Add, None) => return false,
        (Fold::Replace, _) | (Fold::Start, None) => Vec::new(),
        (_, Some(held)) => held.value.clone(),
    };
    let mut seen: HashSet<K> = list.iter().map(&order).collect();
    list.extend(rows.iter().filter(|row| seen.insert(order(row))).cloned());
    list.sort_by_key(|row| Reverse(order(row)));
    list.truncate(KEEP);
    put(slot, now, list)
}
