//! A transfer's fee and the withdrawal delay, read from the venue's own answers.

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn the_fee_is_the_account_s_own_read_in_the_collateral_s_millionths() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/transferFeeInfo.json"));
    let account = AccountIndex::new(10).unwrap();
    let fee = block_on(transfer_fee(&transport, account, Some(AccountIndex::new(713_888).unwrap())));
    assert_eq!(fee, Ok(1_000_000), "one USDC");
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("transferFeeInfo", true));
    assert_eq!(call.fields, [("account_index", "10".to_owned()), ("to_account_index", "713888".to_owned())]);
    assert_eq!(transfer_fee_call(account, None).fields.len(), 1, "any account: the venue's default");
}

#[test]
fn each_venue_answers_its_own_withdrawal_delay_in_seconds() {
    for (text, seconds) in [
        (include_str!("../../../fixtures/lighter/withdrawalDelay.json"), 1_060),
        (include_str!("../../../fixtures/robinhood/withdrawalDelay.json"), 376),
    ] {
        let transport = Recorded::answering(text);
        assert_eq!(block_on(withdrawal_delay(&transport)), Ok(seconds));
        assert_eq!((transport.only().url_path().as_str(), transport.only().auth), ("/api/v1/withdrawalDelay", false));
    }
    let refused = Recorded::answering(include_str!("../../../fixtures/lighter/refused_no_token.json"));
    let account = AccountIndex::new(10).unwrap();
    assert_eq!(block_on(transfer_fee(&refused, account, None)).unwrap_err().code(), Some(20001));
}
