//! `notification` (liquidations, deleverages and announcements for an account), `rfq` (the open
//! requests for a quote) and `live_points` (Robinhood Chain's points). Each needs the account's
//! auth token.

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::live::frame::fields::{blank, raw_text};
use crate::utils::ids::whole;
use crate::utils::{AccountIndex, Dec, Figure, MarketId};

/// A position of the account's that the venue liquidated.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LiquidationNote {
    #[serde(default)]
    pub id: String,
    pub is_ask: bool,
    /// In the collateral.
    pub usdc_amount: Dec,
    pub size: Dec,
    pub market_index: MarketId,
    pub price: Dec,
    /// Unix seconds.
    #[serde(default, deserialize_with = "whole")]
    pub timestamp: u64,
    #[serde(default, deserialize_with = "blank")]
    pub avg_price: Option<Dec>,
}

/// A position of the account's that the venue closed against a liquidated one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeleverageNote {
    #[serde(default)]
    pub id: String,
    /// In the collateral.
    pub usdc_amount: Dec,
    pub size: Dec,
    pub market_index: MarketId,
    pub settlement_price: Dec,
    /// Unix seconds.
    #[serde(default, deserialize_with = "whole")]
    pub timestamp: u64,
}

/// A word from the venue.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Announcement {
    pub title: String,
    pub content: String,
    /// Unix seconds.
    #[serde(default, deserialize_with = "whole")]
    pub created_at: u64,
}

/// What a notification says, by its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteContent {
    Liquidation(LiquidationNote),
    Deleverage(DeleverageNote),
    Announcement(Announcement),
    /// A kind this build does not read, or content that did not read as its kind: as written.
    Other(String),
}

/// One notification.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "RawNote")]
pub struct Notification {
    pub id: String,
    /// As the venue writes it: `2024-01-15T10:30:00Z`.
    pub created_at: String,
    pub updated_at: String,
    /// `liquidation`, `deleverage`, `announcement`.
    pub kind: String,
    pub account_index: AccountIndex,
    pub content: NoteContent,
    /// Whether the account acknowledged it (`notification/ack`).
    pub ack: bool,
    pub acked_at: Option<String>,
}

#[derive(Deserialize)]
struct RawNote {
    #[serde(default)]
    id: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
    kind: String,
    account_index: AccountIndex,
    content: Box<RawValue>,
    #[serde(default)]
    ack: bool,
    #[serde(default)]
    acked_at: Option<String>,
}

impl From<RawNote> for Notification {
    fn from(raw: RawNote) -> Self {
        let text = raw.content.get();
        let content = match raw.kind.as_str() {
            "liquidation" => serde_json::from_str(text).map(NoteContent::Liquidation).ok(),
            "deleverage" => serde_json::from_str(text).map(NoteContent::Deleverage).ok(),
            "announcement" => serde_json::from_str(text).map(NoteContent::Announcement).ok(),
            _ => None,
        };
        Notification {
            id: raw.id,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
            content: content.unwrap_or_else(|| NoteContent::Other(text.to_owned())),
            kind: raw.kind,
            account_index: raw.account_index,
            ack: raw.ack,
            acked_at: raw.acked_at,
        }
    }
}

/// `notification`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notifications {
    /// From the channel.
    pub account: AccountIndex,
    pub notes: Vec<Notification>,
}

/// One open request for a quote. A request not the account's own shows its market and size only:
/// its direction reads `-1`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rfq {
    /// What a response names.
    #[serde(deserialize_with = "whole")]
    pub id: u64,
    pub account_index: AccountIndex,
    pub market_index: MarketId,
    pub direction: i8,
    #[serde(default, deserialize_with = "blank")]
    pub base_amount: Option<Dec>,
    #[serde(default, deserialize_with = "blank")]
    pub quote_amount: Option<Dec>,
    /// `opened` and on.
    pub status: String,
    /// As written.
    #[serde(default, deserialize_with = "raw_text")]
    pub metadata: String,
    /// As written.
    #[serde(default, deserialize_with = "raw_text")]
    pub responses: String,
    /// Unix milliseconds.
    #[serde(default, deserialize_with = "whole")]
    pub created_at: u64,
    #[serde(default, deserialize_with = "whole")]
    pub updated_at: u64,
}

/// `live_points`: the account's points so far. Robinhood Chain only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePoints {
    /// From the channel.
    pub account: AccountIndex,
    pub total: Figure,
}
