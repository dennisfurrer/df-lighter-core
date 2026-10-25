//! An account's orders and trades: `account_market` (one market's orders, trades, position and
//! balances), `account_orders` (one market's orders), `account_all_orders` and
//! `account_all_trades`. An order frame carries each order that changed with its status: one no
//! longer live has filled or ended.

use crate::account::history::PositionFunding;
use crate::account::holdings::BoPosition;
use crate::account::{Asset, Order, Position};
use crate::live::account::Volumes;
use crate::markets::Trade;
use crate::utils::{AccountIndex, MarketId};

/// `account_market`: one account on one market. Needs the account's auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountMarket {
    /// From the channel.
    pub market: MarketId,
    pub account: AccountIndex,
    /// A spot market's balances; empty on a perp.
    pub assets: Vec<Asset>,
    /// A perp's funding payments; empty on spot.
    pub fundings: Vec<PositionFunding>,
    pub orders: Vec<Order>,
    /// `None` on a market with no position, and on spot.
    pub position: Option<Position>,
    /// On a binary option.
    pub bo_position: Option<BoPosition>,
    pub trades: Vec<Trade>,
    /// Unix microseconds, when sent.
    pub transaction_time: Option<u64>,
}

/// `account_orders`: one account's orders on one market. Needs the account's auth token. The
/// venue's channel names the market only (`account_orders:1`); the account comes in the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountOrders {
    /// From the channel.
    pub market: MarketId,
    pub account: AccountIndex,
    pub orders: Vec<Order>,
    /// The matching engine's nonce.
    pub nonce: u64,
}

/// `account_all_orders`: one account's orders on every market. Needs the account's auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountAllOrders {
    /// From the channel.
    pub account: AccountIndex,
    pub orders: Vec<Order>,
}

/// `account_all_trades`: one account's trades on every market. On subscribe, its volumes and no
/// trades; after, each trade as it executes. Public: no auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountAllTrades {
    /// From the channel.
    pub account: AccountIndex,
    pub trades: Vec<Trade>,
    /// On subscribe only.
    pub volumes: Option<Volumes>,
}
