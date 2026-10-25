//! How an account is set up.
//!
//! - [`subaccounts`]: opening a sub-account (`CreateSubAccount`).
//! - [`mode`]: the trading mode, classic or unified (`UpdateAccountConfig`), and the assets that
//!   count as margin (`UpdateAccountAssetConfig`).
//! - [`integrators`]: approving and revoking a partner's fees (`ApproveIntegrator`).
//! - [`tier`]: the account's tier and fee rates (`accountLimits`), changing it (`changeAccountTier`).
//! - [`tokens`]: read-only API tokens and maker-only keys.
//! - [`metadata`]: name, description and colour, and acknowledging notifications.
//! - [`referral`]: referral codes, referred accounts and their stats, kickbacks, points.

pub mod integrators;
pub mod metadata;
pub mod mode;
pub mod referral;
pub mod subaccounts;
pub mod tier;
pub mod tokens;

pub use integrators::{ApproveIntegrator, ApprovedIntegrator};
pub use metadata::{
    AccountMetadata, AccountMetadataPage, SubAccountMetadata, account_metadata, account_metadata_call,
    ack_notification, ack_notification_call, set_account_metadata, set_account_metadata_call,
};
pub use mode::{TradingMode, UpdateAccountAssetConfig, UpdateAccountConfig};
pub use referral::{
    Kickback, L1Metadata, Referral, ReferralPointEntry, ReferralPoints, ReferralQuery, ReferralStat, ReferralTotals,
    TradeStats, UseReferral, UserReferrals, create_referral_code, create_referral_code_call, l1_metadata,
    l1_metadata_call, referral_code, referral_code_call, referral_points, referral_points_call, referral_stats,
    referral_stats_call, update_kickback, update_kickback_call, update_referral_code, update_referral_code_call,
    use_referral_code, use_referral_code_call, user_referrals, user_referrals_call,
};
pub use subaccounts::CreateSubAccount;
#[cfg(feature = "sign")]
pub use subaccounts::create_sub_account;
pub use tier::{
    AccountLimits, account_limits, account_limits_call, change_account_tier, change_account_tier_call, tier_of,
};
pub use tokens::{
    MAX_READ_TOKEN_S, MIN_READ_TOKEN_S, NewReadToken, READ_SCOPE, ReadToken, create_read_token, create_read_token_call,
    maker_only_api_keys, maker_only_api_keys_call, read_tokens, read_tokens_call, revoke_read_token,
    revoke_read_token_call, set_maker_only_api_keys, set_maker_only_api_keys_call,
};
