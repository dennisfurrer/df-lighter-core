//! The account frames carry the rows the reads answer: a binary option position, a funding
//! payment, a transaction, each read as the docs write it.

use super::*;
use crate::account::TxStatus;
use crate::live::fixtures::FUNDING;
use crate::live::{Data, decode};
use crate::utils::{AccountIndex, Dec, MarketId, Side};

fn data(text: &str) -> Data {
    decode(text).unwrap().data().cloned().unwrap()
}

#[test]
fn an_account_frame_carries_its_binary_option_positions_and_funding() {
    let text = format!(
        r#"{{"account":10,"channel":"account_all:10","bo_positions":{{"5212":{{"market_id":5212,"entry_quote":"4.5",
        "avg_entry_price":"0.45","sign":1,"size":"10"}}}},"funding_histories":{{"0":[{FUNDING}]}},
        "transaction_time":1,"type":"update/account_all"}}"#
    );
    let Data::AccountAll(all) = data(&text) else { panic!() };
    let position = &all.bo_positions[0];
    assert_eq!(
        (position.market_id, position.size, position.is_up()),
        (MarketId::new(5212).unwrap(), Dec::from_int(10), true)
    );
    let funding = &all.fundings[0];
    assert_eq!(
        (funding.market_id, funding.side(), funding.discount),
        (MarketId::new(0).unwrap(), Some(Side::Bid), Dec::ZERO)
    );
    assert_eq!((funding.timestamp, funding.change), (1_773_850_000, Dec::parse("0.001").unwrap()));
}

#[test]
fn a_transaction_that_never_lapses_reads_its_times_as_zero() {
    let tx = r#"{"hash":"ab","type":14,"status":2,"account_index":3,"expire_at":-1,"queued_at":-1,"executed_at":1700000000005}"#;
    let Data::AccountTx(txs) =
        data(&format!(r#"{{"channel":"account_tx:3","txs":[{tx}],"type":"update/account_tx"}}"#))
    else {
        panic!()
    };
    let AccountTx { account, txs } = txs;
    let tx = &txs[0];
    assert_eq!(
        (account, tx.account_index, tx.status),
        (AccountIndex::new(3).unwrap(), AccountIndex::new(3).ok(), TxStatus::Executed)
    );
    assert_eq!((tx.expire_at, tx.queued_at, tx.executed_at, tx.transaction_time), (0, 0, 1_700_000_000_005, 0));
}
