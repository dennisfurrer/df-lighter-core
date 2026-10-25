//! Read-only API tokens and maker-only keys.
//!
//! A read-only token (`ro:{account}:{single|all}:{expiry}:{hex}`) reads what an auth token reads
//! and can never sign: for a dashboard, a tax tool or an agent, handed to a client with
//! [`crate::Client::with_read_token`]. `single` covers one account, `all` the master and its
//! sub-accounts. It lives from five minutes to ten years. `tokens/create` is heavy (weight 10,000
//! on Lighter, 23,000 on Robinhood Chain): make one and keep it.
//!
//! Maker-only keys (Premium): the slots listed take only the venue's zero-latency transactions
//! (post-only orders, cancels) on its fast path. `setMakerOnlyApiKeys` replaces the whole list;
//! slots 0 to 3 cannot be listed, and a change waits an hour after the last.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::lenient::{Done, list, text};
use crate::utils::{AccountIndex, ApiKeyIndex};

/// The scope every read-only token has.
pub const READ_SCOPE: &str = "read.*";

/// The shortest life a read-only token takes, seconds: five minutes.
pub const MIN_READ_TOKEN_S: u64 = 5 * 60;

/// The longest, seconds: ten years.
pub const MAX_READ_TOKEN_S: u64 = 10 * 365 * 24 * 60 * 60;

/// One read-only token, as `tokens` lists it and `tokens/create` answers.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ReadToken {
    #[serde(deserialize_with = "whole")]
    pub token_id: u64,
    /// The token itself, `ro:...`; a secret.
    #[serde(default, deserialize_with = "text")]
    pub api_token: String,
    #[serde(default, deserialize_with = "text")]
    pub name: String,
    pub account_index: AccountIndex,
    /// Unix seconds.
    #[serde(default, deserialize_with = "whole")]
    pub expiry: u64,
    /// Whether it reads the master's sub-accounts too.
    #[serde(default)]
    pub sub_account_access: bool,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default, deserialize_with = "text")]
    pub scopes: String,
}

/// Everything but the token itself.
impl std::fmt::Debug for ReadToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadToken")
            .field("token_id", &self.token_id)
            .field("name", &self.name)
            .field("account_index", &self.account_index)
            .field("expiry", &self.expiry)
            .field("sub_account_access", &self.sub_account_access)
            .field("revoked", &self.revoked)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

impl ReadToken {
    /// Whether it still reads at `now_s`, unix seconds: not revoked, not lapsed.
    #[must_use]
    pub const fn is_live(&self, now_s: u64) -> bool {
        !self.revoked && self.expiry > now_s
    }
}

/// What `tokens/create` posts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReadToken {
    pub account: AccountIndex,
    pub name: String,
    /// Unix seconds, from five minutes to ten years ahead.
    pub expiry: u64,
    /// Whether it reads the sub-accounts too (an `all` token).
    pub sub_account_access: bool,
}

#[derive(Deserialize)]
struct Tokens {
    #[serde(default, deserialize_with = "list")]
    api_tokens: Vec<ReadToken>,
}

#[derive(Deserialize)]
struct Revoked {
    #[serde(default)]
    revoked: bool,
}

#[derive(Deserialize)]
struct MakerOnly {
    #[serde(default, deserialize_with = "list")]
    api_key_indexes: Vec<u64>,
}

/// What `tokens` asks: the account's read-only tokens.
#[must_use]
pub fn read_tokens_call(account: AccountIndex) -> Call {
    Call::get("tokens").field("account_index", account).authed()
}

pub async fn read_tokens(transport: &impl Transport, account: AccountIndex) -> Result<Vec<ReadToken>, ApiError> {
    Ok(read::<Tokens>(transport, read_tokens_call(account)).await?.api_tokens)
}

/// What `tokens/create` posts, with the read scope.
#[must_use]
pub fn create_read_token_call(token: &NewReadToken) -> Call {
    Call::post("tokens/create")
        .field("name", &token.name)
        .field("account_index", token.account)
        .field("expiry", token.expiry)
        .field("sub_account_access", token.sub_account_access)
        .field("scopes", READ_SCOPE)
        .authed()
}

/// Makes a read-only token; the answer carries the token, shown once.
pub async fn create_read_token(transport: &impl Transport, token: &NewReadToken) -> Result<ReadToken, ApiError> {
    read(transport, create_read_token_call(token)).await
}

/// What `tokens/revoke` posts.
#[must_use]
pub fn revoke_read_token_call(account: AccountIndex, token_id: u64) -> Call {
    Call::post("tokens/revoke").field("token_id", token_id).field("account_index", account).authed()
}

/// Revokes a read-only token; whether the venue says it is.
pub async fn revoke_read_token(
    transport: &impl Transport,
    account: AccountIndex,
    token_id: u64,
) -> Result<bool, ApiError> {
    Ok(read::<Revoked>(transport, revoke_read_token_call(account, token_id)).await?.revoked)
}

/// What `getMakerOnlyApiKeys` asks.
#[must_use]
pub fn maker_only_api_keys_call(account: AccountIndex) -> Call {
    Call::get("getMakerOnlyApiKeys").field("account_index", account).authed()
}

/// The slots marked maker-only; a slot this build cannot name is left out.
pub async fn maker_only_api_keys(
    transport: &impl Transport,
    account: AccountIndex,
) -> Result<Vec<ApiKeyIndex>, ApiError> {
    let listed = read::<MakerOnly>(transport, maker_only_api_keys_call(account)).await?.api_key_indexes;
    Ok(listed
        .into_iter()
        .filter_map(|slot| u8::try_from(slot).ok())
        .filter_map(|slot| ApiKeyIndex::new(slot).ok())
        .collect())
}

/// What `setMakerOnlyApiKeys` posts: the whole list, as a JSON array (`[4,5]`; `[]` clears it).
#[must_use]
pub fn set_maker_only_api_keys_call(account: AccountIndex, slots: &[ApiKeyIndex]) -> Call {
    let listed: Vec<u8> = slots.iter().map(|slot| slot.get()).collect();
    Call::post("setMakerOnlyApiKeys")
        .field("account_index", account)
        .field("api_key_indexes", serde_json::to_string(&listed).unwrap_or_else(|_| "[]".to_owned()))
        .authed()
}

/// Replaces the maker-only list with `slots`.
pub async fn set_maker_only_api_keys(
    transport: &impl Transport,
    account: AccountIndex,
    slots: &[ApiKeyIndex],
) -> Result<(), ApiError> {
    read::<Done>(transport, set_maker_only_api_keys_call(account, slots)).await.map(drop)
}

#[cfg(test)]
#[path = "tests/tokens.rs"]
mod tests;
