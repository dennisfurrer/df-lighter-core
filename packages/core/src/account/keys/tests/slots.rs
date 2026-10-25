//! An account's slots and nonces, read from the venue's own answers.

use super::*;
use crate::test_support::{Recorded, block_on};

const APIKEYS: &str = include_str!("../../../../fixtures/lighter/apikeys_10.json");

#[test]
fn every_slot_reads_with_its_key_or_none() {
    let transport = Recorded::answering(APIKEYS);
    let account = AccountIndex::new(10).unwrap();
    let slots = block_on(api_keys(&transport, account)).unwrap();
    assert_eq!(slots.len(), 8);
    let first = &slots[0];
    assert_eq!((first.account, first.slot.get(), first.nonce), (account, 0, 1_787_851_175_902));
    assert_eq!(
        first.public_key.map(|key| key.to_hex()).as_deref(),
        Some("0x844880b6fa93ea0c7962810d75ca1c5fca4d7a556908f7f13a2ab21c47bd2896a8c31a2b72a7e720")
    );
    assert!(first.holds(&first.public_key.unwrap()));
    assert_eq!((slots[1].public_key, slots[1].transaction_time), (None, 0), "eighty zeros is no key");
    let call = transport.only();
    assert_eq!(call.url_path(), "/api/v1/apikeys");
    assert_eq!(call.fields, [("account_index", "10".to_owned()), ("api_key_index", "255".to_owned())]);
}

#[test]
fn one_slot_is_found_or_none() {
    let transport = Recorded::answering(APIKEYS);
    let account = AccountIndex::new(10).unwrap();
    let slot = block_on(api_key(&transport, account, ApiKeyIndex::new(2).unwrap())).unwrap().unwrap();
    assert_eq!((slot.slot.get(), slot.nonce), (2, 4));
    assert_eq!(transport.only().value("api_key_index"), Some("2"));
    let unused = block_on(api_key(&transport, account, ApiKeyIndex::new(200).unwrap())).unwrap();
    assert_eq!(unused, None);
}

#[test]
fn the_next_nonce_is_the_venue_s_count() {
    let transport = Recorded::answering(include_str!("../../../../fixtures/lighter/nextNonce_10.json"));
    let nonce = block_on(next_nonce(&transport, AccountIndex::new(10).unwrap(), ApiKeyIndex::new(4).unwrap()));
    assert_eq!(nonce, Ok(4));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("nextNonce", false));
    assert_eq!(call.fields, [("account_index", "10".to_owned()), ("api_key_index", "4".to_owned())]);
}

#[test]
fn an_account_with_no_keys_lists_none() {
    let transport = Recorded::answering(r#"{"code":21109,"message":"api key not found"}"#);
    let account = AccountIndex::new(10).unwrap();
    assert_eq!(block_on(api_keys(&transport, account)), Ok(Vec::new()));
    assert_eq!(block_on(api_key(&transport, account, ApiKeyIndex::new(4).unwrap())), Ok(None));
}

/// A key keeps its slot while the venue shows it or nothing there; otherwise the lowest empty
/// slot that is not the venue's own.
#[test]
fn a_new_key_takes_its_own_slot_or_the_lowest_free_one() {
    let slots = block_on(api_keys(&Recorded::answering(APIKEYS), AccountIndex::new(10).unwrap())).unwrap();
    let slot = |index| ApiKeyIndex::new(index).unwrap();
    let held = |index: u8| slots.iter().find(|one| one.slot.get() == index).and_then(|one| one.public_key);
    let listed: Vec<u8> = slots.iter().filter(|one| one.public_key.is_some()).map(|one| one.slot.get()).collect();
    let lowest = (4..=254).find(|index| !listed.contains(index)).unwrap();
    assert_eq!(pick_slot(Network::Lighter, &slots, None), Some(slot(lowest)));
    let theirs = *listed.iter().find(|index| **index >= 4).unwrap();
    let mine = held(0).unwrap();
    assert_eq!(pick_slot(Network::Lighter, &slots, Some((slot(theirs), &mine))), Some(slot(lowest)), "someone else's");
    let own = held(theirs).unwrap();
    assert_eq!(pick_slot(Network::Lighter, &slots, Some((slot(theirs), &own))), Some(slot(theirs)), "still mine");
    assert_eq!(pick_slot(Network::Lighter, &slots, Some((slot(2), &own))), Some(slot(lowest)), "the app's own");
    assert_eq!(pick_slot(Network::Lighter, &[], None), Some(slot(4)));
    assert_eq!(pick_slot(Network::Robinhood, &[], Some((slot(157), &own))), Some(slot(4)), "the app's on Robinhood");
}
