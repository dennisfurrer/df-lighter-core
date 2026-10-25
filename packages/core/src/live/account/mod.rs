//! What the account channels send, typed. Positions, balances, pool shares, orders and trades
//! are the rows the reads write ([`crate::account::Position`], [`crate::account::Asset`],
//! [`crate::account::PoolShare`], [`crate::account::Order`], [`crate::markets::Trade`]), and so
//! are a binary option position ([`crate::account::holdings::BoPosition`]), a funding payment
//! ([`crate::account::history::PositionFunding`]), a transaction
//! ([`crate::account::history::Transaction`]) and a pool's terms ([`crate::earn::pools::PoolInfo`]);
//! the socket's own rows are here. Where the venue sends a map by market or asset, its rows come in
//! one list in the order sent: each row names its own market or asset.
//!
//! - [`holdings`]: `account_all`, `account_all_positions`, `account_all_assets` and
//!   `account_spot_avg_entry_prices`.
//! - [`orders`]: `account_market`, `account_orders`, `account_all_orders`, `account_all_trades`.
//! - [`stats`]: what `user_stats` sends.
//! - [`tx`]: what `account_tx` sends.
//! - [`pool`]: `pool_data` and `pool_info`.
//! - [`notes`]: `notification`, `rfq` and `live_points`.

pub mod holdings;
pub mod notes;
pub mod orders;
pub mod pool;
pub mod stats;
pub mod tx;

pub use holdings::{AccountAll, AccountAllAssets, AccountAllPositions, AvgEntryPrice, AvgEntryPrices, Volumes};
pub use notes::{
    Announcement, DeleverageNote, LiquidationNote, LivePoints, NoteContent, Notification, Notifications, Rfq,
};
pub use orders::{AccountAllOrders, AccountAllTrades, AccountMarket, AccountOrders};
pub use pool::PoolData;
pub use stats::{Stats, UserStats};
pub use tx::AccountTx;

#[cfg(test)]
#[path = "tests/account.rs"]
mod tests;
