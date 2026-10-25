//! What the conformance, mocked and live suites share: the recordings on disk, a pinned clock,
//! the vectors' published test keys, and [`venue::Venue`], a fake Lighter that answers from
//! recordings and checks every transaction the way the venue does.

// Each suite uses its own part of this.
#![allow(dead_code)]

pub mod venue;

use std::path::PathBuf;

use df_lighter_core::account::{ApiKey, LocalWallet};
use df_lighter_core::utils::sign::SecretKey;
use df_lighter_core::utils::{AccountIndex, Address, ApiKeyIndex};
use serde_json::Value;

/// 2026-09-21 14:13:20 UTC: the vectors' clock, ten minutes before their transactions lapse.
pub const NOW: u64 = 1_790_000_000_000;

/// A clock pinned at [`NOW`].
pub const fn clock() -> u64 {
    NOW
}

/// The account the vectors sign for on Lighter, and its slot.
pub const ACCOUNT: u64 = 100_001;
pub const SLOT: u8 = 4;

/// Hardhat's account #0, a published test key: the owner's wallet. Only the fake venue ever sees
/// what it signs.
pub fn owner() -> LocalWallet {
    LocalWallet::from_hex(vectors()["wallet"]["private_key"].as_str().unwrap()).unwrap()
}

pub fn owner_address() -> Address {
    owner().address()
}

/// The vectors' API key `name` (`a` or `b`), published with them.
pub fn secret(name: &str) -> SecretKey {
    SecretKey::from_hex(vectors()["keys"][name]["private_key"].as_str().unwrap()).unwrap()
}

/// Key `a` at [`SLOT`] of [`ACCOUNT`].
pub fn api_key() -> ApiKey {
    ApiKey::new(AccountIndex::new(ACCOUNT).unwrap(), ApiKeyIndex::new(SLOT).unwrap(), secret("a"))
}

/// A file under `fixtures/`: what the venue answered, recorded.
pub fn fixture(path: &str) -> String {
    read(&root().join("fixtures").join(path))
}

/// `vectors/lighter_go.json`: what lighter-go and go-ethereum produce.
pub fn vectors() -> Value {
    serde_json::from_str(&read(&root().join("vectors").join("lighter_go.json"))).unwrap()
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
