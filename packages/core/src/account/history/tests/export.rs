//! Exports and partner stats: the recorded `partnerStats` of an account with none (public), and an
//! export's link shaped from the docs' `ExportData` (it needs a token).

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn an_export_is_asked_with_the_token_and_answers_a_link() {
    let transport = Recorded::answering(r#"{"code":200,"data_url":"https://exports.example/100001-trades.csv"}"#);
    let query = ExportQuery {
        from_ms: Some(1_790_000_000_000),
        side: Some(Side::Bid),
        role: Some(Role::Maker),
        trade_kind: Some(TradeKind::Trade),
        ..ExportQuery::new(AccountIndex::new(100_001).unwrap(), ExportKind::Trades)
    };
    let link = block_on(export(&transport, &query)).unwrap();
    assert_eq!(link, "https://exports.example/100001-trades.csv");
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("export", true));
    assert_eq!(
        call.fields,
        [
            ("account_index", "100001".to_owned()),
            ("type", "trade".to_owned()),
            ("start_timestamp", "1790000000000".to_owned()),
            ("side", "long".to_owned()),
            ("role", "maker".to_owned()),
            ("trade_type", "trade".to_owned()),
        ]
    );
    let funding = export_call(&ExportQuery::new(AccountIndex::new(1).unwrap(), ExportKind::Funding));
    assert_eq!(funding.fields, [("account_index", "1".to_owned()), ("type", "funding".to_owned())]);
}

#[test]
fn recorded_partner_stats_read_and_are_asked_in_public() {
    let transport = Recorded::answering(include_str!("../../../../fixtures/lighter/partnerStats.json"));
    let stats = block_on(partner_stats(&transport, AccountIndex::new(713_888).unwrap(), None, None)).unwrap();
    assert_eq!((stats.total_fees_earned, stats.total_trades, stats.unique_clients), (Dec::ZERO, 0, 0));
    let call = transport.only();
    assert_eq!((call.path, call.auth, call.fields.len()), ("partnerStats", false, 1));
    let windowed = partner_stats_call(AccountIndex::new(713_888).unwrap(), Some(1), Some(2), Some("daily"));
    assert_eq!(windowed.value("period"), Some("daily"));
}
