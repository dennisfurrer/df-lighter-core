//! Each convenience call against a wire that answers from a queue: the nonce asked once, the
//! transaction posted to `sendTx` (or `sendTxBatch`) with the type and fields it signs, read back
//! field for field; and a refusal before signing posting nothing.

use serde_json::Value;

use super::*;
use crate::client::{Network, Request, Response, TransportError};
use crate::test_support::{Wire, block_on, ok, vector_key};
use crate::trade::manage::ModifyIntent;
use crate::trade::read_back::read_back;
use crate::trade::support::{case, d, form_field, order};
use crate::utils::sign::BadTx;
use crate::utils::sign::tx::{Envelope, tx_hash};
use crate::utils::{AccountIndex, ApiKeyIndex, OrderIndex, Side};

const NOW: u64 = 1_790_000_000_000;
const ACCOUNT: u64 = 100_001;

fn clock() -> u64 {
    NOW
}

fn keyed(answers: impl IntoIterator<Item = Result<Response, TransportError>>) -> Client<Wire, ApiKey> {
    let key =
        ApiKey::new(AccountIndex::new(ACCOUNT).unwrap(), ApiKeyIndex::of::<4>(), vector_key(&case("limit-buy-gtt")));
    Client::new(Wire::answering(answers), Network::Lighter, clock, key)
}

fn next_nonce(nonce: u64) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"code":200,"nonce":{nonce}}}"#))
}

fn taken() -> Result<Response, TransportError> {
    ok(r#"{"code":200,"tx_hash":"aa"}"#)
}

fn path(request: &Request) -> String {
    request.url.replace("https://mainnet.zklighter.elliot.ai", "")
}

/// The one `sendTx` a client posted after asking the nonce: its type and its `tx_info`.
fn posted(client: &Client<Wire, ApiKey>) -> (u8, String) {
    let sent = client.http().sent();
    let paths: Vec<String> = sent.iter().map(path).collect();
    assert_eq!(paths, ["/api/v1/nextNonce?account_index=100001&api_key_index=4", "/api/v1/sendTx"]);
    let tx_type = form_field(&sent[1].body, "tx_type").parse().unwrap();
    (tx_type, form_field(&sent[1].body, "tx_info"))
}

/// What was posted is `tx`, at `nonce`, lapsing ten minutes less a second from now, signed.
fn is(tx: &(impl Tx + ?Sized), client: &Client<Wire, ApiKey>, nonce: u64) -> Value {
    let (tx_type, info) = posted(client);
    assert_eq!(tx_type, tx.tx_type().code());
    let envelope = Envelope {
        account: AccountIndex::new(ACCOUNT).unwrap(),
        api_key: ApiKeyIndex::of::<4>(),
        nonce,
        expired_at: NOW + 599_000,
    };
    let hash = tx_hash(tx, Network::Lighter.chain_id(), &envelope).to_hex();
    assert_eq!(read_back(tx, Network::Lighter.chain_id(), &envelope, &info, &hash), Ok(()));
    serde_json::from_str(&info).unwrap()
}

fn refused_before_signing(answer: Result<Sent, SendError>, field: &str) {
    match answer {
        Err(SendError::Sign(SignTxError::Bad(BadTx { field: refused, .. }))) => assert_eq!(refused, field),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_order_is_signed_at_the_next_nonce_and_posted_as_create_order() {
    let client = keyed([next_nonce(45), taken()]);
    let order = order(&case("limit-buy-gtt")["fields"]);
    let sent = block_on(place(&client, &order)).unwrap();
    assert_eq!(sent.tx_hash, "aa");
    let info = is(&CreateOrder::new(order), &client, 45);
    assert_eq!((info["Price"].as_u64(), info["ClientOrderIndex"].as_u64()), (Some(834_530), Some(1_790_000_000_000)));
}

#[test]
fn a_group_is_posted_as_one_transaction() {
    let client = keyed([next_nonce(3), taken()]);
    let fields = &case("oto-entry-and-stop")["fields"];
    let orders: Vec<Order> = fields["Orders"].as_array().unwrap().iter().map(order).collect();
    let group = CreateGroupedOrders::oto(orders[0], orders[1]);
    block_on(place_group(&client, &group)).unwrap();
    let info = is(&group, &client, 3);
    assert_eq!(info["Orders"].as_array().map(Vec::len), Some(2));
}

#[test]
fn a_cancel_names_the_order_by_either_index() {
    let client = keyed([next_nonce(7), taken()]);
    let venue = OrderIndex::new(26_177_173_101_967_466).unwrap();
    block_on(cancel(&client, MarketId::of::<92>(), venue)).unwrap();
    let info = is(&CancelOrder::new(MarketId::of::<92>(), venue), &client, 7);
    assert_eq!(info["Index"].as_u64(), Some(26_177_173_101_967_466), "exact past 2^53");
}

#[test]
fn cancel_all_runs_now_everywhere_or_in_one_market() {
    let client = keyed([next_nonce(1), taken()]);
    block_on(cancel_all(&client)).unwrap();
    let info = is(&CancelAllOrders::now(), &client, 1);
    assert_eq!(info["L2TxAttributes"], Value::Null);

    let client = keyed([next_nonce(2), taken()]);
    block_on(cancel_all_in(&client, MarketId::of::<1>())).unwrap();
    let info = is(&CancelAllOrders::in_market(MarketId::of::<1>()), &client, 2);
    assert_eq!(info["L2TxAttributes"]["5"].as_u64(), Some(1));
}

#[test]
fn one_kind_of_market_is_cancelled_as_one_batch_at_consecutive_nonces() {
    let client = keyed([next_nonce(10), ok(r#"{"code":200,"tx_hash":["aa","bb"]}"#)]);
    let markets = [MarketId::of::<0>(), MarketId::of::<1>(), MarketId::of::<0>()];
    let sent = block_on(cancel_all_in_markets(&client, &markets)).unwrap();
    assert_eq!(sent.tx_hashes, ["aa", "bb"]);
    let requests = client.http().sent();
    assert_eq!(path(&requests[1]), "/api/v1/sendTxBatch");
    assert_eq!(form_field(&requests[1].body, "tx_types"), "[16,16]");
    let infos: Vec<String> = serde_json::from_str(&form_field(&requests[1].body, "tx_infos")).unwrap();
    let infos: Vec<Value> = infos.iter().map(|info| serde_json::from_str(info).unwrap()).collect();
    let read: Vec<(Option<u64>, Option<u64>)> =
        infos.iter().map(|info| (info["Nonce"].as_u64(), info["L2TxAttributes"]["5"].as_u64())).collect();
    assert_eq!(read, [(Some(10), Some(0)), (Some(11), Some(1))]);
}

#[test]
fn a_scheduled_cancel_all_is_held_to_the_venue_s_window_by_the_client_s_clock() {
    let client = keyed([next_nonce(4), taken()]);
    refused_before_signing(block_on(schedule_cancel_all(&client, NOW + 60_000)), "Time");
    assert!(client.http().sent().is_empty(), "nothing asked, nothing posted");
    block_on(schedule_cancel_all(&client, NOW + 3_600_000)).unwrap();
    let info = is(&CancelAllOrders::at(NOW + 3_600_000), &client, 4);
    assert_eq!((info["TimeInForce"].as_u64(), info["Time"].as_u64()), (Some(1), Some(NOW + 3_600_000)));

    let client = keyed([next_nonce(5), taken()]);
    block_on(abort_cancel_all(&client)).unwrap();
    assert_eq!(is(&CancelAllOrders::abort(), &client, 5)["TimeInForce"].as_u64(), Some(2));
}

#[test]
fn a_modify_is_posted_with_its_version() {
    let client = keyed([next_nonce(8), taken()]);
    let change = ModifyIntent::new(
        MarketId::of::<1>(),
        OrderIndex::new(562_953_635_421_737).unwrap(),
        Side::Bid,
        d("0.02"),
        d("83400"),
    )
    .build(crate::utils::Grid::new(5, 1))
    .unwrap()
    .version(7);
    block_on(modify(&client, &change)).unwrap();
    let info = is(&change, &client, 8);
    assert_eq!(info["L2TxAttributes"]["8"].as_u64(), Some(7));
}

#[test]
fn leverage_and_margin_are_refused_before_signing_or_posted_exactly() {
    let client = keyed([next_nonce(20), taken(), taken()]);
    refused_before_signing(
        block_on(set_leverage(&client, MarketId::of::<1>(), 0, MarginMode::Cross)),
        "InitialMarginFraction",
    );
    refused_before_signing(block_on(add_margin(&client, MarketId::of::<0>(), d("0.0000001"))), "USDCAmount");
    assert!(client.http().sent().is_empty());

    block_on(set_leverage(&client, MarketId::of::<1>(), 10, MarginMode::Isolated)).unwrap();
    let info = is(&UpdateLeverage::new(MarketId::of::<1>(), 10, MarginMode::Isolated).unwrap(), &client, 20);
    assert_eq!((info["InitialMarginFraction"].as_u64(), info["MarginMode"].as_u64()), (Some(1_000), Some(1)));

    let client = keyed([next_nonce(30), taken()]);
    block_on(add_margin(&client, MarketId::of::<0>(), d("12345678.901234"))).unwrap();
    let info = is(&UpdateMargin::add(MarketId::of::<0>(), d("12345678.901234")).unwrap(), &client, 30);
    assert_eq!((info["USDCAmount"].as_u64(), info["Direction"].as_u64()), (Some(12_345_678_901_234), Some(1)));

    let client = keyed([next_nonce(31), taken()]);
    block_on(remove_margin(&client, MarketId::of::<0>(), d("5"))).unwrap();
    assert_eq!(
        is(&UpdateMargin::remove(MarketId::of::<0>(), d("5")).unwrap(), &client, 31)["Direction"].as_u64(),
        Some(0)
    );
}
