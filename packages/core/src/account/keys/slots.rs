//! An account's API key slots: which key sits at each (`apikeys`), each slot's next nonce
//! (`nextNonce`), and which slot a new key takes. Both reads are public: anyone may read which
//! keys an account has registered.
//!
//! Slots 0 to 3 belong to Lighter's own web and mobile apps (and 157 on Robinhood Chain), and
//! registering over a slot someone else uses revokes their key. [`pick_slot`] keeps a key's own
//! slot while the venue still shows it, or nothing, there; otherwise it takes the lowest empty
//! slot from 4 to 254.

use serde::{Deserialize, Deserializer};

use crate::client::Network;
use crate::client::http::{ApiError, Call, Transport, codes, read};
use crate::utils::ids::whole;
use crate::utils::sign::PublicKey;
use crate::utils::{AccountIndex, ApiKeyIndex};

/// One slot as the venue lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Slot {
    #[serde(rename = "account_index")]
    pub account: AccountIndex,
    #[serde(rename = "api_key_index")]
    pub slot: ApiKeyIndex,
    /// The next nonce the slot's key signs at.
    #[serde(deserialize_with = "whole")]
    pub nonce: u64,
    /// `None` for a slot no key was ever registered at, which the venue lists as eighty zeros.
    #[serde(deserialize_with = "registered")]
    pub public_key: Option<PublicKey>,
    /// When the slot's key last signed, unix microseconds; 0 when it never has.
    #[serde(deserialize_with = "whole")]
    pub transaction_time: u64,
}

impl Slot {
    /// Whether `key` is the one registered here.
    #[must_use]
    pub fn holds(&self, key: &PublicKey) -> bool {
        self.public_key.as_ref() == Some(key)
    }
}

fn registered<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<PublicKey>, D::Error> {
    let text = String::deserialize(deserializer)?;
    match text.trim_start_matches("0x").bytes().all(|digit| digit == b'0') {
        true => Ok(None),
        false => PublicKey::from_hex(&text).map(Some).map_err(serde::de::Error::custom),
    }
}

#[derive(Deserialize)]
struct Slots {
    api_keys: Vec<Slot>,
}

#[derive(Deserialize)]
struct NextNonce {
    #[serde(deserialize_with = "whole")]
    nonce: u64,
}

/// What `apikeys` asks: one slot, or every slot the account has used when `slot` is `None`.
#[must_use]
pub fn api_keys_call(account: AccountIndex, slot: Option<ApiKeyIndex>) -> Call {
    Call::get("apikeys").field("account_index", account).field("api_key_index", slot.map_or(255, ApiKeyIndex::get))
}

/// Every slot the account has used: some hold no key. None at all reads as an empty list (the
/// venue's 21109).
pub async fn api_keys(transport: &impl Transport, account: AccountIndex) -> Result<Vec<Slot>, ApiError> {
    listed(transport, api_keys_call(account, None)).await
}

/// One slot; `None` when the account never used it.
pub async fn api_key(
    transport: &impl Transport,
    account: AccountIndex,
    slot: ApiKeyIndex,
) -> Result<Option<Slot>, ApiError> {
    let slots = listed(transport, api_keys_call(account, Some(slot))).await?;
    Ok(slots.into_iter().find(|listed| listed.slot == slot))
}

async fn listed(transport: &impl Transport, call: Call) -> Result<Vec<Slot>, ApiError> {
    match read::<Slots>(transport, call).await {
        Ok(slots) => Ok(slots.api_keys),
        Err(error) if error.code() == Some(codes::API_KEY_NOT_FOUND) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// The highest slot a key may take.
const LAST_SLOT: u8 = 254;

/// The slot to register a key into, given what the venue lists for the account: `preferred`
/// (the slot this key was registered at, and the key) while the venue shows that key or nothing
/// there, else the lowest empty slot from 4 to 254 that is not the venue's own. `None` when
/// every slot is taken.
#[must_use]
pub fn pick_slot(
    network: Network,
    listed: &[Slot],
    preferred: Option<(ApiKeyIndex, &PublicKey)>,
) -> Option<ApiKeyIndex> {
    let holder = |slot: ApiKeyIndex| listed.iter().find(|one| one.slot == slot).and_then(|one| one.public_key);
    let free = |slot: ApiKeyIndex| !network.reserves_api_key(slot);
    if let Some((slot, key)) = preferred
        && free(slot)
        && holder(slot).is_none_or(|held| held == *key)
    {
        return Some(slot);
    }
    (0..=LAST_SLOT).filter_map(|index| ApiKeyIndex::new(index).ok()).find(|slot| free(*slot) && holder(*slot).is_none())
}

/// What `nextNonce` asks.
#[must_use]
pub fn next_nonce_call(account: AccountIndex, slot: ApiKeyIndex) -> Call {
    Call::get("nextNonce").field("account_index", account).field("api_key_index", slot)
}

/// The nonce the slot's next transaction signs at.
pub async fn next_nonce(transport: &impl Transport, account: AccountIndex, slot: ApiKeyIndex) -> Result<u64, ApiError> {
    Ok(read::<NextNonce>(transport, next_nonce_call(account, slot)).await?.nonce)
}

#[cfg(test)]
#[path = "tests/slots.rs"]
mod tests;
