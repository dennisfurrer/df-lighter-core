//! Signing against lighter-go and go-ethereum: every vector in `vectors/lighter_go.json`
//! (`vectors/gen` made it, with a pinned clock and a recorded random stream) replayed through this
//! crate's public API only. Each transaction is built as a caller builds it, from the Go fields
//! lighter-go was handed, then signed with the very draws lighter-go took: its type, hash and
//! `tx_info` byte for byte, the text the owner signs, the owner's signature and the `tx_info` sent;
//! and lighter-go's own signature checked with the public key alone. Whatever lighter-go refused
//! is refused here before anything is hashed.
//!
//! - `keys`: registering a key (`ChangePubKey`), keys generated from the draws, auth tokens.
//! - `trade`: orders and groups, cancels, modifies, leverage and isolated margin.
//! - `transfer`: transfers and secure withdrawals.
//! - `account`: a sub-account, the trading mode, an asset as margin, integrator approvals.
//! - `earn`: public pools, their shares, LIT staking.
//! - `deploy`: a binary options operator's create, update and settle.
//! - `evm`: the L1 calldata go-ethereum packs and the EIP-1559 transactions it signs.
//!
//! Below, a check that no case on file goes unreplayed.

// A test crate: clippy's allowance for tests covers `#[test]` functions only, not their helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod account;
mod deploy;
mod earn;
mod evm;
mod keys;
mod trade;
mod transfer;
mod vector;

use serde_json::Value;

use crate::support::vectors;

/// Each name in `list`, sorted, after checking the file names none twice.
fn names_on_file(list: &str) -> Vec<String> {
    let mut names: Vec<String> = vectors()[list]
        .as_array()
        .unwrap_or_else(|| panic!("no {list} on file"))
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "{list}: a name on file twice");
    names
}

/// The names the suites replay, sorted.
fn replayed(lists: &[&[&str]]) -> Vec<String> {
    let mut names: Vec<String> = lists.iter().flat_map(|list| list.iter().map(|name| (*name).to_owned())).collect();
    names.sort();
    names
}

#[test]
fn every_transaction_case_on_file_is_replayed_once() {
    let suites = [
        keys::CASES.as_slice(),
        trade::CASES.as_slice(),
        transfer::CASES.as_slice(),
        account::CASES.as_slice(),
        earn::CASES.as_slice(),
        deploy::CASES.as_slice(),
    ];
    let covered = replayed(&suites.concat());
    let on_file = names_on_file("cases");
    assert_eq!(on_file.len(), 76);
    assert_eq!(covered, on_file, "a case on file no test replays, or one replayed that is not on file");
}

#[test]
fn every_token_key_and_evm_case_on_file_is_replayed_once() {
    assert_eq!(replayed(&[&keys::TOKENS]), names_on_file("tokens"));
    assert_eq!(replayed(&[&evm::CASES]), names_on_file("evm"));
    let mut generated: Vec<String> = vectors()["keys"].as_object().unwrap().keys().cloned().collect();
    generated.sort();
    assert_eq!(generated, keys::GENERATED, "the keys lighter-go generated");
    assert_eq!((keys::TOKENS.len(), evm::CASES.len(), keys::GENERATED.len()), (3, 17, 2));
}

#[test]
fn the_file_is_lighter_go_s_at_the_pinned_clock() {
    let file = vectors();
    assert_eq!(file["now_ms"], Value::from(support::NOW));
    assert!(file["source"].as_str().unwrap().starts_with("lighter-go 01933021"));
    let wallet: df_lighter_core::Address = file["wallet"]["address"].as_str().unwrap().parse().unwrap();
    assert_eq!(wallet, support::owner_address(), "the owner's wallet is Hardhat's first account");
}
