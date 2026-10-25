//! Lease terms as both venues list them (recorded 2026-10-25), the fee by the venue's formula, a
//! page of leases made to the docs' shape, and the `litLease` post built, never sent.

use super::*;
use crate::client::Method;
use crate::client::http::limit::is_send;
use crate::test_support::{Recorded, block_on};

const LIGHTER: &str = include_str!("../../../fixtures/lighter/leaseOptions.json");
const ROBINHOOD: &str = include_str!("../../../fixtures/robinhood/leaseOptions.json");

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn both_venues_list_the_same_terms_and_their_own_fee_account() {
    let transport = Recorded::answering(LIGHTER);
    let lighter = block_on(lease_options(&transport)).unwrap();
    assert_eq!(transport.only().url_path(), "/api/v1/leaseOptions");
    assert_eq!(lighter.fee_account.get(), 711_044);
    let terms: Vec<(u32, &str)> =
        lighter.options.iter().map(|term| (term.duration_days, term.annual_rate.as_str())).collect();
    assert_eq!(terms, [(360, "10"), (180, "12.5"), (90, "15"), (30, "18"), (7, "22"), (1, "25")], "longest first");
    assert_eq!(lighter.lasting(30).map(|term| term.annual_rate.as_str()), Some("18"));
    assert_eq!(lighter.lasting(31), None);

    let robinhood: LeaseOptions = serde_json::from_str(ROBINHOOD).unwrap();
    assert_eq!((robinhood.fee_account.get(), robinhood.options.len()), (4, 6));
}

/// 1,000 LIT for 30 days at 18% a year is 15 LIT; at 12.5% for 180 days, 62.5.
#[test]
fn a_lease_fee_is_the_venue_s_integer_formula() {
    let lighter: LeaseOptions = serde_json::from_str(LIGHTER).unwrap();
    let thousand = lease_units(dec("1000")).unwrap();
    assert_eq!(thousand, 100_000_000_000);
    assert_eq!(lighter.lasting(30).and_then(|term| term.fee(thousand)), Some(1_500_000_000));
    assert_eq!(lighter.lasting(180).and_then(|term| term.fee(thousand)), Some(6_250_000_000));
    assert_eq!(lease_fee(1, &lighter.options[5].annual_rate, 1), Some(0), "cut, as integers are");
    assert_eq!(lease_fee(u64::MAX, &lighter.options[5].annual_rate, 10_000), None, "past 64 bits");
    assert!(matches!(lease_units(dec("0.000000001")), Err(BadUnits::TooFine { places: 8, .. })));
}

#[test]
fn a_page_of_leases_reads_each_status_and_keeps_an_unknown_one() {
    let answer = r#"{"code":200,"leases":[
        {"id":7,"master_account_index":10,"lease_amount":100000000000,"fee_amount":1500000000,
         "start":1790000000000,"end":1792592000000,"status":"leased","error":""},
        {"id":6,"master_account_index":10,"lease_amount":5,"fee_amount":0,"start":1,"end":2,
         "status":"canceled","error":"fee not received"},
        {"id":5,"master_account_index":10,"lease_amount":5,"fee_amount":0,"start":1,"end":2,
         "status":"paused","error":""}],"next_cursor":"abc"}"#;
    let transport = Recorded::answering(answer);
    let page = block_on(leases(&transport, AccountIndex::new(10).unwrap(), Some("xyz"), Some(0))).unwrap();
    let call = transport.only();
    assert!(call.auth, "an account's own");
    assert_eq!(
        call.fields,
        [("account_index", "10".to_owned()), ("cursor", "xyz".to_owned()), ("limit", "1".to_owned())]
    );
    assert_eq!(page.next_cursor.as_deref(), Some("abc"));
    let lease = &page.leases[0];
    assert_eq!(
        (lease.status.clone(), lease.lease_amount, lease.fee_amount),
        (LeaseStatus::Leased, 100_000_000_000, 1_500_000_000)
    );
    assert!(lease.is_active(1_791_000_000_000) && !lease.is_active(1_792_592_000_000));
    assert_eq!(
        (page.leases[1].status.clone(), page.leases[1].error.as_str()),
        (LeaseStatus::Canceled, "fee not received")
    );
    assert_eq!(page.leases[2].status, LeaseStatus::Other("paused".to_owned()));
}

#[test]
fn a_lit_lease_is_a_signed_post_built_and_never_sent_here() {
    let call = lit_lease_call(r#"{"FromAccountIndex":10}"#.to_owned(), 100_000_000_000, 30);
    assert_eq!((call.method, call.url_path(), call.auth), (Method::Post, "/api/v1/litLease".to_owned(), false));
    assert_eq!(
        call.fields,
        [
            ("tx_info", r#"{"FromAccountIndex":10}"#.to_owned()),
            ("lease_amount", "100000000000".to_owned()),
            ("duration_days", "30".to_owned()),
        ]
    );
    assert!(is_send(call.path), "paced and never retried as a send");
    assert!(leases_call(AccountIndex::new(10).unwrap(), None, None).fields.len() == 1);
}
