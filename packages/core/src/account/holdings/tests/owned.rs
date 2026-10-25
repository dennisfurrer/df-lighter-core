//! The accounts an address owns, recorded on both venues: account 10's owner on Lighter (a master
//! and eight sub-accounts), and the venue's "account not found" on each for an address with none.

use super::*;
use crate::test_support::{Recorded, block_on};

const OWNED: &str = include_str!("../../../../fixtures/lighter/accountsByL1Address.json");

fn owner() -> Address {
    "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap()
}

#[test]
fn an_owner_s_accounts_read_with_the_master_first_of_its_kind() {
    let transport = Recorded::answering(OWNED);
    let owned = block_on(accounts_by_l1_address(&transport, owner(), None)).unwrap();
    assert_eq!((owned.l1_address, owned.accounts.len()), (Some(owner()), 9));
    let master = owned.master().unwrap();
    assert_eq!((master.index.get(), master.kind, master.is_master()), (10, AccountKind::Master, true));
    assert_eq!(
        (master.collateral.to_string(), master.available_balance),
        ("1945.003244".to_owned(), None),
        "blank is none"
    );
    assert!(master.is_funded());
    let subs: Vec<&OwnedAccount> = owned.sub_accounts().collect();
    assert_eq!(subs.len(), 8);
    assert!(subs.iter().all(|sub| sub.kind == AccountKind::SubAccount && !sub.index.is_master()));
    assert_eq!(subs[1].trading_mode, Some(TradingMode::Unified));
    assert!(subs[0].is_funded() && !subs[1].is_funded());
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("accountsByL1Address", false));
    assert_eq!(call.fields, [("l1_address", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned())]);
}

#[test]
fn the_master_is_found_and_an_owner_without_accounts_has_none_on_both_venues() {
    let transport = Recorded::answering(OWNED);
    let master = block_on(master_account(&transport, owner())).unwrap().unwrap();
    assert_eq!(master.index.get(), 10);
    for none in [
        include_str!("../../../../fixtures/lighter/accountsByL1Address_none.json"),
        include_str!("../../../../fixtures/robinhood/accountsByL1Address_none.json"),
    ] {
        let transport = Recorded::answering(none);
        let owned = block_on(accounts_by_l1_address(&transport, owner(), Some("next"))).unwrap();
        assert!(owned.is_empty() && owned.master().is_none());
        assert_eq!(owned.l1_address, Some(owner()));
        assert_eq!(transport.only().value("cursor"), Some("next"));
        assert_eq!(block_on(master_account(&transport, owner())), Ok(None));
    }
}

#[test]
fn without_a_master_row_the_lowest_index_is_the_master() {
    let text = r#"{"code":200,"sub_accounts":[
        {"account_type":1,"index":281474976699107,"l1_address":"0x478686468057D9873E8718083D40d2927b12F91A","collateral":"0"},
        {"account_type":1,"index":281474976699106,"l1_address":"0x478686468057D9873E8718083D40d2927b12F91A","collateral":"5"}]}"#;
    let owned: OwnedAccounts = serde_json::from_str(text).unwrap();
    assert_eq!(owned.master().map(|master| master.index.get()), Some(281_474_976_699_106));
    assert_eq!(owned.sub_accounts().count(), 1);
    let other_refusal = Recorded::answering(r#"{"code":20001,"message":"invalid param "}"#);
    assert_eq!(block_on(accounts_by_l1_address(&other_refusal, owner(), None)).unwrap_err().code(), Some(20_001));
}
