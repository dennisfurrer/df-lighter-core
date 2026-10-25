//! Points, on Lighter on Robinhood Chain only: the points leaderboard (`leaderboard`, weekly, all
//! time or one competition's) and an account's live points (`livePoints/total`). Points are the
//! venue's floats, kept as written.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::lenient::{list, text};
use crate::utils::{AccountIndex, Address, Figure};

/// Which leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Board {
    Weekly,
    AllTime,
    /// One competition, by the venue's id.
    Competition(String),
}

impl Board {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Board::Weekly => "weekly",
            Board::AllTime => "all",
            Board::Competition(_) => "competition",
        }
    }
}

/// One address on the points leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PointsEntry {
    pub l1_address: Address,
    pub points: Figure,
    #[serde(default)]
    pub entry: i64,
    #[serde(default, rename = "entryId")]
    pub entry_id: i64,
    #[serde(default, deserialize_with = "text")]
    pub metadata: String,
}

#[derive(Deserialize)]
struct Entries {
    #[serde(default, deserialize_with = "list")]
    entries: Vec<PointsEntry>,
}

#[derive(Deserialize)]
struct LivePoints {
    total_live_points: Figure,
}

/// What `leaderboard` asks; `owner` narrows it to one address.
#[must_use]
pub fn points_leaderboard_call(board: &Board, owner: Option<Address>) -> Call {
    let competition = match board {
        Board::Competition(id) => Some(id.as_str()),
        _ => None,
    };
    Call::get("leaderboard")
        .field("type", board.name())
        .maybe("l1_address", owner.map(Address::to_hex))
        .maybe("competition_id", competition)
}

pub async fn points_leaderboard(
    transport: &impl Transport,
    board: &Board,
    owner: Option<Address>,
) -> Result<Vec<PointsEntry>, ApiError> {
    Ok(read::<Entries>(transport, points_leaderboard_call(board, owner)).await?.entries)
}

/// What `livePoints/total` asks, with the token.
#[must_use]
pub fn live_points_call(account: AccountIndex) -> Call {
    Call::get("livePoints/total").field("account_index", account).authed()
}

/// The account's live points total.
pub async fn live_points(transport: &impl Transport, account: AccountIndex) -> Result<Figure, ApiError> {
    Ok(read::<LivePoints>(transport, live_points_call(account)).await?.total_live_points)
}

#[cfg(test)]
#[path = "tests/points.rs"]
mod tests;
