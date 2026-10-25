//! Referrals. Every one of these needs a token (`referral/get` is refused without one), so the
//! answers here are shaped from the docs' schemas (`ReferralCode`, `UserReferrals`,
//! `ReferralStats`, `ReferralPoints`, `L1Metadata`), not recorded; the posts are only built and
//! recorded, never sent.

use super::*;
use crate::client::Method;
use crate::client::http::form;
use crate::test_support::{Recorded, block_on};

const STATS: &str = r#"{"count":12,"volume":"25000.500000","web_count":10,"web_volume":"20000.000000",
    "mobile_app_count":2,"mobile_app_volume":"5000.500000","mobile_browser_count":0,
    "mobile_browser_volume":"0.000000","free_maker_volume":"0.000000","free_taker_volume":"0.000000",
    "non_free_maker_volume":"10000.000000","non_free_taker_volume":"15000.500000",
    "maker_fees_paid":"0.400000","taker_fees_paid":"4.200140"}"#;

fn owner() -> Address {
    "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap()
}

fn account() -> AccountIndex {
    AccountIndex::new(10).unwrap()
}

#[test]
fn the_account_s_code_is_read_made_and_renamed_with_the_token() {
    let transport = Recorded::answering(r#"{"code":200,"referral_code":"FURRER"}"#);
    assert_eq!(block_on(referral_code(&transport, account())).as_deref(), Ok("FURRER"));
    assert_eq!(block_on(create_referral_code(&transport, account())).as_deref(), Ok("FURRER"));
    let sent = transport.sent();
    assert_eq!((sent[0].method, sent[0].path, sent[0].auth), (Method::Get, "referral/get", true));
    assert_eq!((sent[1].method, sent[1].path, sent[1].auth), (Method::Post, "referral/create", true));
    assert_eq!(sent[1].fields, [("account_index", "10".to_owned())]);

    let transport = Recorded::answering(r#"{"code":200,"success":true}"#);
    assert_eq!(block_on(update_referral_code(&transport, account(), "DFU")), Ok(true));
    assert_eq!(block_on(update_kickback(&transport, account(), Dec::lit::<1>(125))), Ok(true));
    let sent = transport.sent();
    assert_eq!(form(&sent[0].fields), "account_index=10&new_referral_code=DFU");
    assert_eq!(
        (sent[1].path, form(&sent[1].fields).as_str()),
        ("referral/kickback/update", "account_index=10&kickback_percentage=12.5")
    );
}

#[test]
fn a_code_is_used_with_only_the_fields_given() {
    let transport = Recorded::answering(r#"{"code":200}"#);
    let plain = UseReferral { l1_address: owner(), referral_code: "FURRER".to_owned(), ..UseReferral::default() };
    block_on(use_referral_code(&transport, &plain)).unwrap();
    let social = UseReferral { x: Some("@dfu".to_owned()), source: Some("app".to_owned()), ..plain.clone() };
    block_on(use_referral_code(&transport, &social)).unwrap();
    let sent = transport.sent();
    assert_eq!((sent[0].method, sent[0].path, sent[0].auth), (Method::Post, "referral/use", true));
    assert_eq!(form(&sent[0].fields), "l1_address=0x478686468057d9873e8718083d40d2927b12f91a&referral_code=FURRER");
    assert_eq!(
        (sent[1].value("x"), sent[1].value("source"), sent[1].value("discord")),
        (Some("@dfu"), Some("app"), None)
    );
}

#[test]
fn referrals_and_their_stats_read_exactly() {
    let page = format!(
        r#"{{"code":200,"cursor":"abc","used_code":"","referrals":[{{"l1_address":"0x2Ba553d9F990a3B66b03b2dC0D030dfC1c061036",
        "referral_code":"FURRER","used_at":1782506002,"trade_stats":{STATS},"tier":"premium","source":"web"}}],
        "totals":{{"count":12,"volume":"25000.500000","free_maker_volume":"0","free_taker_volume":"0",
        "non_free_maker_volume":"10000","non_free_taker_volume":"15000.5","maker_fees_paid":"0.4",
        "taker_fees_paid":"4.20014","total_referrals":1,"total_premium_referrals":1}}}}"#
    );
    let transport = Recorded::answering(&page);
    let query = ReferralQuery { limit: Some(500), eligible_only: true, ..ReferralQuery::default() };
    let referrals = block_on(user_referrals(&transport, owner(), &query)).unwrap();
    let first = &referrals.referrals[0];
    assert_eq!((first.tier.as_str(), first.trade_stats.count), ("premium", 12));
    assert_eq!(first.trade_stats.taker_fees_paid.to_string(), "4.20014");
    assert_eq!((referrals.cursor.as_str(), referrals.totals.total_premium_referrals), ("abc", 1));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("referral/userReferrals", true));
    assert_eq!((call.value("limit"), call.value("is_eligible")), (Some("300"), Some("true")), "the limit is capped");

    let transport = Recorded::answering(&format!(
        r#"{{"code":200,"stats":[{{"start_timestamp":1790000000000,"end_timestamp":1790604800000,"trade_stats":{STATS}}}]}}"#
    ));
    let stats = block_on(referral_stats(&transport, owner(), false)).unwrap();
    assert_eq!(
        (stats[0].trade_stats.volume.to_string(), stats[0].end_timestamp),
        ("25000.5".to_owned(), 1_790_604_800_000)
    );
    assert_eq!(transport.only().fields, [("l1_address", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned())]);
}

#[test]
fn points_and_l1_terms_read_as_the_venue_writes_them() {
    let transport = Recorded::answering(
        r#"{"code":200,"referrals":[{"l1_address":"0x2Ba553d9F990a3B66b03b2dC0D030dfC1c061036","total_points":120.5,
        "week_points":10,"total_reward_points":12.05,"week_reward_points":1,"reward_point_multiplier":"0.1"}],
        "user_total_points":3000.25,"user_last_week_points":250,"user_total_referral_reward_points":12.05,
        "user_last_week_referral_reward_points":1,"reward_point_multiplier":"0.1"}"#,
    );
    let points = block_on(referral_points(&transport, account())).unwrap();
    assert_eq!((points.user_total_points.as_str(), points.referrals[0].total_points.as_str()), ("3000.25", "120.5"));
    assert_eq!(transport.only().path, "referral/points");

    let transport = Recorded::answering(
        r#"{"code":200,"l1_address":"0x478686468057D9873E8718083D40d2927b12F91A","can_invite":true,
        "referral_points_percentage":"","referral_program_kickback":"12.5",
        "referral_program_kickback_history":[{"date":1790000000,"percentage":"10"}]}"#,
    );
    let terms = block_on(l1_metadata(&transport, owner())).unwrap();
    assert_eq!((terms.l1_address, terms.referral_points_percentage), (owner(), None));
    assert_eq!(terms.referral_program_kickback.map(|kickback| kickback.to_string()).as_deref(), Some("12.5"));
    assert_eq!(terms.referral_program_kickback_history[0].percentage.map(|p| p.to_string()).as_deref(), Some("10"));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("l1Metadata", true));
}
