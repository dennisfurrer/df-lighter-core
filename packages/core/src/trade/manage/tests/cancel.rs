//! `CancelOrder`, held to lighter-go: by client index, by the venue's index, and by one past what
//! a JS number holds.

use super::*;
use crate::trade::support::{case, cases, int};

#[test]
fn every_cancel_lighter_go_signs_signs_the_same_here() {
    let mut names = Vec::new();
    for case in cases("CancelOrder") {
        let fields = &case["fields"];
        let market = MarketId::new(int(fields, "MarketIndex")).unwrap();
        let tx = CancelOrder::new(market, OrderRef::from_index(int(fields, "Index")).unwrap());
        crate::trade::support::signs_as_lighter_go(&tx, &case);
        names.push(case["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(names, ["cancel-by-client-index", "cancel-by-order-index", "cancel-order-index-above-2^53"]);
}

#[test]
fn a_client_index_and_a_venue_index_both_name_an_order() {
    let by_client = CancelOrder::new(MarketId::of::<1>(), crate::utils::ClientOrderIndex::of::<1_790_000_000_000>());
    let fields = &case("cancel-by-client-index")["fields"];
    assert_eq!(by_client.order.index(), int(fields, "Index"));
    let venue = crate::utils::OrderIndex::new(562_953_635_421_737).unwrap();
    assert_eq!(CancelOrder::new(MarketId::of::<1>(), venue).order, OrderRef::Venue(venue));
    assert!(OrderRef::from_index(0).is_err() && OrderRef::from_index(1 << 60).is_err());
    let skipping = by_client.with(Attributes::NONE.skip_nonce());
    assert_eq!(skipping.attributes().entries(), [(4, 1)]);
    assert_eq!(skipping.check(), Ok(()));
}
