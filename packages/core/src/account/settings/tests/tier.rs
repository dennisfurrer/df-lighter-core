//! The tier and its limits. `accountLimits` answers only with a token, so its answer here is
//! shaped from the docs' schema (`AccountLimits`), not recorded; `changeAccountTier` is only built
//! and recorded, never sent.

use super::*;
use crate::client::Method;
use crate::client::http::form;
use crate::test_support::{Recorded, block_on};

/// `accountLimits` in the documented shape, a Premium account staking LIT.
const LIMITS: &str = r#"{"code":200,"max_llp_percentage":25,"user_tier":"premium","can_create_public_pool":true,
    "max_llp_amount":"1000000.000000","current_maker_fee_tick":40,"current_taker_fee_tick":280,
    "effective_lit_stakes":"1500.00000000","leased_lit":"500.00000000","user_tier_name":"Premium 1",
    "user_tier_last_update":1790000000000,"bo_user_tier_name":"Tier 0","bo_fee_rate":0.72}"#;

fn account() -> AccountIndex {
    AccountIndex::new(100_001).unwrap()
}

#[test]
fn the_limits_read_with_the_tier_named() {
    let transport = Recorded::answering(LIMITS);
    let limits = block_on(account_limits(&transport, account())).unwrap();
    assert_eq!((limits.tier(), limits.user_tier_name.as_str()), (Some(Tier::Premium), "Premium 1"));
    assert_eq!((limits.current_maker_fee_tick, limits.current_taker_fee_tick), (40, 280));
    assert_eq!(limits.effective_lit_stakes.map(|lit| lit.to_string()).as_deref(), Some("1500"));
    assert_eq!(limits.bo_fee_rate.as_ref().map(Figure::as_str), Some("0.72"));
    assert_eq!((limits.max_llp_percentage, limits.user_tier_last_update), (25, 1_790_000_000_000));
    let call = transport.only();
    assert_eq!((call.method, call.url_path(), call.auth), (Method::Get, "/api/v1/accountLimits".to_owned(), true));
    assert_eq!(call.fields, [("account_index", "100001".to_owned())]);
}

#[test]
fn a_tier_word_reads_whatever_its_case_and_an_unknown_one_is_none() {
    for (word, tier) in [
        ("standard", Some(Tier::Standard)),
        ("STD", Some(Tier::Standard)),
        ("Plus", Some(Tier::Plus)),
        ("premium", Some(Tier::Premium)),
        ("advanced", Some(Tier::Premium)),
        ("builder", Some(Tier::Builder)),
        ("gold", None),
    ] {
        assert_eq!(tier_of(word), tier, "{word}");
    }
    let sparse: AccountLimits = serde_json::from_str(r#"{"code":200,"user_tier":"plus"}"#).unwrap();
    assert_eq!((sparse.tier(), sparse.bo_fee_rate, sparse.leased_lit), (Some(Tier::Plus), None, None));
}

#[test]
fn a_tier_change_posts_the_tier_s_name_with_the_token() {
    let transport = Recorded::answering(r#"{"code":200,"message":""}"#);
    block_on(change_account_tier(&transport, account(), Tier::Premium)).unwrap();
    let call = transport.only();
    assert_eq!((call.method, call.path, call.auth), (Method::Post, "changeAccountTier", true));
    assert_eq!(form(&call.fields), "account_index=100001&new_tier=premium");
    let refused = Recorded::answering(r#"{"code":21001,"message":"tier change too soon"}"#);
    let error = block_on(change_account_tier(&refused, account(), Tier::Standard)).unwrap_err();
    assert_eq!(error.code(), Some(21_001));
}
