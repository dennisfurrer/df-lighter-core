//! What an account did.
//!
//! - [`orders`]: an order as the account's reads and channels write it.
//! - [`order_reads`]: its working orders, its ended ones, and orders by client index; what an
//!   order implies (limit price, trigger, when placed).
//! - [`fills`]: its own trades, and each as its fill: side, maker or taker, fee, realized profit.
//! - [`txs`]: transactions by hash, by sequence or by L1 hash, and the account's own.
//! - [`pnl`](mod@pnl): its PnL chart, the funding its positions paid, the PnL leaderboard.
//! - [`liquidations`](mod@liquidations): its liquidations and deleverages.
//! - [`export`](mod@export): whole histories as a file, and an integrator's stats.
//! - [`points`]: points and their leaderboard, on Robinhood Chain.

pub mod export;
pub mod fills;
pub mod liquidations;
pub mod order_reads;
pub mod orders;
pub mod pnl;
pub mod points;
pub mod txs;

pub use export::{ExportKind, ExportQuery, PartnerStats, export, export_call, partner_stats, partner_stats_call};
pub use fills::{
    Fill, MOST_TRADES, Role, TradePage, TradeQuery, TradeSort, account_trades, account_trades_call, fee, fills_since,
    realized,
};
pub use liquidations::{
    Liquidation, LiquidationKind, LiquidationPage, LiquidationState, LiquidationTrade, RiskInfo, RiskParameters,
    liquidations, liquidations_call,
};
pub use order_reads::{
    InactiveQuery, MOST_CLIENT_INDEXES, MOST_INACTIVE, OrderPage, active_orders, active_orders_call, inactive_orders,
    inactive_orders_call, kinds_filter, orders_by_client_index, orders_by_client_index_call,
};
pub use orders::{Order, OrderStatus, TriggerStatus};
pub use pnl::{
    FundingPage, FundingQuery, LeaderboardEntry, PnlChart, PnlLeaderboard, PnlPoint, PnlQuery, PositionFunding, RankBy,
    Resolution, Window, pnl, pnl_call, pnl_leaderboard, pnl_leaderboard_call, position_funding, position_funding_call,
};
pub use points::{Board, PointsEntry, live_points, live_points_call, points_leaderboard, points_leaderboard_call};
pub use txs::{
    MOST_TXS, Transaction, TxQuery, TxStatus, account_txs, account_txs_call, tx, tx_by_sequence, tx_by_sequence_call,
    tx_call, tx_from_l1_tx_hash, tx_from_l1_tx_hash_call,
};
