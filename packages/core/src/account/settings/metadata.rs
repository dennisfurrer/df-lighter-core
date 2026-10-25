//! What an account says about itself: its name, description and colour (`accountMetadata`,
//! `setAccountMetadata`), what it may do (invite, request quotes, run an agent), and acknowledging
//! a notification the venue sent it (`notification/ack`).
//!
//! `accountMetadata` answers only with an auth token: the venue refuses it without one (code 20001).

use serde::Deserialize;

use crate::account::holdings::account::Lookup;
use crate::client::{ApiError, Call, Transport, read};
use crate::utils::ids::whole;
use crate::utils::lenient::{Done, list, text};
use crate::utils::{AccountIndex, ApiKeyIndex};

/// A sub-account's colour in the venue's app: `metadata` on an account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Deserialize)]
pub struct SubAccountMetadata {
    /// As the app writes it; empty when none was chosen.
    #[serde(default, deserialize_with = "text")]
    pub color: String,
}

/// One account's metadata, as `accountMetadata` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountMetadata {
    pub account_index: AccountIndex,
    #[serde(default, deserialize_with = "text")]
    pub name: String,
    #[serde(default, deserialize_with = "text")]
    pub description: String,
    /// Whether it may hand out a referral code.
    #[serde(default)]
    pub can_invite: bool,
    /// Its share of a referral's points, in percent, as the venue writes it; empty for none.
    #[serde(default, deserialize_with = "text")]
    pub referral_points_percentage: String,
    /// Unix seconds; 0 when the venue does not say.
    #[serde(default, deserialize_with = "whole")]
    pub created_at: u64,
    /// Whether it may ask for quotes (RFQ), and on which markets when only some.
    #[serde(default)]
    pub can_rfq: bool,
    #[serde(default, deserialize_with = "list")]
    pub can_rfq_market_ids: Vec<String>,
    #[serde(default)]
    pub metadata: SubAccountMetadata,
    #[serde(default)]
    pub agent_enabled: bool,
}

/// A page of `accountMetadata`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountMetadataPage {
    #[serde(default, deserialize_with = "list")]
    pub account_metadatas: Vec<AccountMetadata>,
    /// Empty or absent on the last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// What `accountMetadata` asks.
#[must_use]
pub fn account_metadata_call(lookup: Lookup, cursor: Option<&str>) -> Call {
    Call::get("accountMetadata")
        .field("by", lookup.by())
        .field("value", lookup.value())
        .maybe("cursor", cursor)
        .authed()
}

pub async fn account_metadata(
    transport: &impl Transport,
    lookup: Lookup,
    cursor: Option<&str>,
) -> Result<AccountMetadataPage, ApiError> {
    read(transport, account_metadata_call(lookup, cursor)).await
}

/// What `setAccountMetadata` posts: `metadata` (the venue's own text, a JSON object as the app
/// writes it) for `target`, an account of the master `master`, signed off by the key at `slot`.
/// The one post under `/api/v1` with a JSON body rather than a form.
#[must_use]
pub fn set_account_metadata_call(
    master: AccountIndex,
    target: AccountIndex,
    slot: ApiKeyIndex,
    metadata: &str,
) -> Call {
    let body = serde_json::json!({
        "master_account_index": master.get(),
        "target_account_index": target.get(),
        "api_key_index": slot.get(),
        "metadata": metadata,
    });
    Call::post("setAccountMetadata").with_json(body.to_string()).authed()
}

/// Sets an account's metadata. Weight 3,000.
pub async fn set_account_metadata(
    transport: &impl Transport,
    master: AccountIndex,
    target: AccountIndex,
    slot: ApiKeyIndex,
    metadata: &str,
) -> Result<(), ApiError> {
    read::<Done>(transport, set_account_metadata_call(master, target, slot, metadata)).await.map(drop)
}

/// What `notification/ack` posts: the notification's id, as the `notification` channel gave it.
#[must_use]
pub fn ack_notification_call(account: AccountIndex, notification_id: &str) -> Call {
    Call::post("notification/ack").field("notif_id", notification_id).field("account_index", account).authed()
}

/// Marks a notification read. Weight 3,000.
pub async fn ack_notification(
    transport: &impl Transport,
    account: AccountIndex,
    notification_id: &str,
) -> Result<(), ApiError> {
    read::<Done>(transport, ack_notification_call(account, notification_id)).await.map(drop)
}

#[cfg(test)]
#[path = "tests/metadata.rs"]
mod tests;
