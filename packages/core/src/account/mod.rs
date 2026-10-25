//! What an account holds, what it did, how it is set up, and who may sign for it.
//!
//! - [`holdings`]: the whole account, the accounts an address owns, positions, balances, pool
//!   shares, and what a position implies (leverage, margin).
//! - [`history`]: orders, fills, transactions, PnL, funding, liquidations, exports, points.
//! - [`settings`]: sub-accounts, the trading mode and margin assets, integrator approvals, the
//!   tier, read-only tokens and maker-only keys, metadata, referrals.
//! - [`keys`]: API keys, auth tokens, the owner's wallet, registering a key.
//!
//! Every read of the account's own (orders, fills, PnL, settings) carries an auth token: give the
//! client an API key, or a read-only token. The account itself and the accounts an address owns
//! are public.

pub mod history;
pub mod holdings;
pub mod keys;
pub mod settings;

pub use history::{
    Fill, InactiveQuery, Order, OrderPage, OrderStatus, PnlQuery, TradePage, TradeQuery, Transaction, TriggerStatus,
    TxStatus, active_orders, fills_since, inactive_orders, orders_by_client_index, tx,
};
pub use holdings::{
    Account, AccountKind, Asset, Lookup, OwnedAccount, OwnedAccounts, PerpsBalance, PoolShare, Position, Setting,
    account, accounts_by_l1_address, master_account,
};
pub use keys::{
    ApiKey, AuthToken, ChangePubKey, L1Signature, Slot, Wallet, WalletError, api_key, api_keys, next_nonce,
};
#[cfg(feature = "sign")]
pub use keys::{LocalWallet, recover, verify};
#[cfg(feature = "sign")]
pub use settings::create_sub_account;
pub use settings::{
    AccountLimits, ApproveIntegrator, ApprovedIntegrator, CreateSubAccount, TradingMode, UpdateAccountAssetConfig,
    UpdateAccountConfig,
};
