//! The money histories: the LLP's transfers as the venue answered them, deposits and withdrawals
//! in the shape the docs give (they need the owner's token), each page's cursor, and words this
//! build does not know read without failing the page.

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn a_pool_s_transfers_read_from_the_venue_s_answer() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/transfer_history.json"));
    let page = block_on(transfer_history(&transport, AccountIndex::LLP, TransferFilter::All, None)).unwrap();
    assert_eq!(page.rows.len(), 4);
    assert_eq!(page.cursor.as_deref(), Some("eyJpbmRleCI6MTM5MDMyNzI1OTIzfQ=="));
    let first = &page.rows[0];
    assert_eq!(
        (first.kind, first.asset, first.from_account, first.to_account.get()),
        (TransferKind::L2BurnSharesOutflow, AssetId::COLLATERAL, AccountIndex::LLP, 34_403)
    );
    assert_eq!(
        (first.amount, first.fee, first.timestamp),
        (Dec::parse("1299.997816").unwrap(), Dec::ZERO, 1_792_921_117_660)
    );
    assert_eq!(
        (first.from_route, first.to_route, first.from_l1_address),
        (Some(Route::Perps), Some(Route::Perps), Address::ZERO)
    );
    assert_eq!(first.tx_hash.len(), 80);
    assert_eq!(page.rows[2].kind, TransferKind::L2MintSharesInflow);
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("transfer/history", true));
    assert_eq!(call.fields, [("account_index", "281474976710654".to_owned())]);
    let next = transfer_history_call(AccountIndex::LLP, TransferFilter::L2Transfer, page.cursor.as_deref());
    assert_eq!(
        next.fields,
        [
            ("account_index", "281474976710654".to_owned()),
            ("type", "L2Transfer".to_owned()),
            ("cursor", "eyJpbmRleCI6MTM5MDMyNzI1OTIzfQ==".to_owned()),
        ]
    );
}

#[test]
fn deposits_and_withdrawals_read_with_their_status() {
    let deposits = r#"{"code":200,"deposits":[{"id":"7","amount":"25.500000","timestamp":1792921117660,"status":"completed","l1_tx_hash":"0x6936ceea7f4827630678f71e669a592b2965d06411e35d7df26880fd22abb098","asset_id":3},{"id":"8","amount":"1.000000","timestamp":1792921117661,"status":"bridging","l1_tx_hash":"","asset_id":3}],"cursor":""}"#;
    let transport = Recorded::answering(deposits);
    let owner: Address = "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap();
    let account = AccountIndex::new(10).unwrap();
    let page = block_on(deposit_history(&transport, account, owner, HistoryFilter::All, None)).unwrap();
    assert_eq!((page.rows[0].status, page.rows[0].amount), (DepositStatus::Completed, Dec::parse("25.5").unwrap()));
    assert_eq!(page.rows[1].status, DepositStatus::Other, "a new word reads, the page with it");
    assert_eq!(page.cursor, None, "the last page");
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("deposit/history", true));
    assert_eq!(
        call.fields,
        [
            ("account_index", "10".to_owned()),
            ("l1_address", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned()),
            ("filter", "all".to_owned()),
        ]
    );

    let withdrawals = r#"{"code":200,"withdraws":[{"id":"9","amount":"10.000000","timestamp":1792921117660,"status":"claimable","type":"secure","l1_tx_hash":"","asset_id":3},{"id":"10","amount":"50.000000","timestamp":1792921117661,"status":"completed","type":"fast","l1_tx_hash":"0x01","asset_id":3}],"cursor":"abc"}"#;
    let transport = Recorded::answering(withdrawals);
    let page = block_on(withdraw_history(&transport, account, HistoryFilter::Claimable, Some("xyz"))).unwrap();
    assert_eq!((page.rows[0].status, page.rows[0].kind), (WithdrawalStatus::Claimable, WithdrawalKind::Secure));
    assert_eq!((page.rows[1].status, page.rows[1].kind), (WithdrawalStatus::Completed, WithdrawalKind::Fast));
    assert_eq!(page.cursor.as_deref(), Some("abc"));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("withdraw/history", true));
    assert_eq!(
        call.fields,
        [("account_index", "10".to_owned()), ("filter", "claimable".to_owned()), ("cursor", "xyz".to_owned())]
    );
    assert_eq!(HistoryFilter::Pending.name(), "pending");
}

#[test]
fn a_refusal_comes_back_with_its_code() {
    let transport = Recorded::answering(include_str!("../../../fixtures/lighter/refused_no_token.json"));
    let refused = block_on(withdraw_history(&transport, AccountIndex::new(10).unwrap(), HistoryFilter::All, None));
    assert_eq!(refused.unwrap_err().code(), Some(20001));
}
