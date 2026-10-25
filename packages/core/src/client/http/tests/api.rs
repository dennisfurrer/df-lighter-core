//! Every call through the gate to its network: the URL, the query or the form, the token, and
//! what a 429 or a 405 does to the calls after it.

use crate::client::http::limit::{MAX_COOLDOWN_MS, STANDARD_BUDGET};
use crate::client::{Call, Client, Method, Network, Tier, Transport, TransportError};
use crate::test_support::{Wire, block_on, ok, status};

fn clock() -> u64 {
    0
}

#[test]
fn a_get_carries_its_fields_in_the_query_and_a_post_in_its_body() {
    for (network, api) in
        [(Network::Lighter, "https://mainnet.zklighter.elliot.ai"), (Network::Robinhood, "https://api.rh.lighter.xyz")]
    {
        let client = Client::reader(Wire::always("{}"), network, clock);
        block_on(client.call(Call::get("orderBookOrders").field("market_id", 1).field("limit", 20))).unwrap();
        block_on(client.call(Call::get("orderBooks"))).unwrap();
        block_on(client.call(Call::post("sendTx").field("tx_type", 15).field("tx_info", "{\"A\":1}"))).unwrap();
        let sent = client.http().sent();
        assert_eq!(sent[0].url, format!("{api}/api/v1/orderBookOrders?market_id=1&limit=20"));
        assert_eq!((sent[0].method, sent[0].body.len()), (Method::Get, 0));
        assert_eq!(sent[1].url, format!("{api}/api/v1/orderBooks"));
        assert_eq!((sent[2].url.as_str(), sent[2].method), (format!("{api}/api/v1/sendTx").as_str(), Method::Post));
        assert_eq!(String::from_utf8_lossy(&sent[2].body), "tx_type=15&tx_info=%7B%22A%22%3A1%7D");
        assert_eq!(sent[2].header("content-type"), Some("application/x-www-form-urlencoded"));
        assert!(sent.iter().all(|request| request.header("authorization").is_none()), "nothing is the account's own");
        let spent = client.spent();
        assert_eq!((spent.budget, spent.minute), (STANDARD_BUDGET, 3 * 400), "{network}: sixty a minute");
    }
}

/// A reader holds no key: the account's own reads carry the read-only token it was given, or
/// go nowhere.
#[test]
fn the_account_s_own_reads_carry_a_token_or_go_nowhere() {
    let client = Client::reader(Wire::always("{}"), Network::Lighter, clock);
    let error = block_on(client.call(Call::get("accountActiveOrders").authed())).unwrap_err();
    assert_eq!(error, TransportError::NoToken("/api/v1/accountActiveOrders".to_owned()));
    assert!(client.http().sent().is_empty(), "nothing went out");

    let client = Client::reader(Wire::always("{}"), Network::Lighter, clock).with_read_token("ro:10:all:9:abc");
    block_on(client.call(Call::get("accountActiveOrders").authed())).unwrap();
    block_on(client.call(Call::get("orderBooks"))).unwrap();
    let sent = client.http().sent();
    assert_eq!(sent[0].header("Authorization"), Some("ro:10:all:9:abc"));
    assert_eq!(sent[1].header("Authorization"), None, "a public read carries none");
}

/// A 429 closes the gate for everything for two seconds, and nothing goes out meanwhile.
#[test]
fn a_429_closes_the_gate_and_nothing_goes_out_until_it_opens() {
    let client = Client::reader(Wire::answering([status(429, ""), ok("{}")]), Network::Lighter, clock);
    let error = block_on(client.call(Call::get("orderBooks"))).unwrap_err();
    assert_eq!(error, TransportError::RateLimited { retry_after_ms: Some(2_000) });
    let held = block_on(client.call(Call::post("sendTx"))).unwrap_err();
    assert_eq!(held, TransportError::RateLimited { retry_after_ms: Some(2_000) }, "sends wait too");
    assert_eq!(client.http().sent().len(), 1);
    let spent = client.spent();
    assert_eq!((spent.refused, spent.cooldown_ms), (1, 2_000));
}

#[test]
fn a_405_from_the_firewall_closes_the_gate_for_a_minute() {
    let client = Client::reader(Wire::answering([status(405, "")]), Network::Robinhood, clock);
    let error = block_on(client.call(Call::get("orderBooks"))).unwrap_err();
    assert_eq!(error, TransportError::RateLimited { retry_after_ms: Some(MAX_COOLDOWN_MS) });
}

/// Near the budget a read is held back before it goes out; on a paid tier a send never is.
#[test]
fn reads_are_held_near_the_budget_and_paid_tier_sends_are_not() {
    let client = Client::reader(Wire::always("{}"), Network::Lighter, clock);
    for _ in 0..48 {
        block_on(client.call(Call::get("orderBooks"))).unwrap();
    }
    let error = block_on(client.call(Call::get("orderBooks"))).unwrap_err();
    assert!(matches!(error, TransportError::RateLimited { retry_after_ms: Some(_) }), "{error:?}");
    let error = block_on(client.call(Call::post("sendTx"))).unwrap_err();
    assert!(matches!(error, TransportError::RateLimited { .. }), "a Standard send spends the same minute");
    assert_eq!(client.http().sent().len(), 48);

    let client = Client::reader(Wire::always("{}"), Network::Lighter, clock).with_tier(Tier::Premium);
    for _ in 0..64 {
        block_on(client.call(Call::get("orderBooks"))).unwrap();
    }
    assert!(block_on(client.call(Call::get("orderBooks"))).is_err(), "19,200 of 24,000 spent");
    block_on(client.call(Call::post("sendTx"))).unwrap();
    assert_eq!(client.spent().minute, 64 * 300, "a Premium send is not counted against reads");
}

/// A warm is one read of the venue's status, and nothing else.
#[test]
fn a_warm_is_one_status_read() {
    let client = Client::reader(Wire::always("{}"), Network::Lighter, clock);
    block_on(client.warm());
    let sent = client.http().bodies();
    assert_eq!(sent, [("https://mainnet.zklighter.elliot.ai/".to_owned(), String::new())]);
    assert_eq!(client.spent().minute, 400);
}

#[test]
fn an_error_status_comes_back_with_its_body() {
    let client =
        Client::reader(Wire::answering([status(400, r#"{"code":20001,"message":"bad"}"#)]), Network::Lighter, clock);
    let error = block_on(client.call(Call::get("account"))).unwrap_err();
    assert_eq!(error, TransportError::Status { status: 400, body: r#"{"code":20001,"message":"bad"}"#.to_owned() });
    let unreachable = Client::reader(Wire::answering([]), Network::Lighter, clock);
    assert!(matches!(block_on(unreachable.call(Call::get("account"))), Err(TransportError::Unreachable(_))));
}

/// The one JSON post goes out as JSON, with its token.
#[test]
fn a_json_post_goes_out_as_json() {
    let client = Client::reader(Wire::always("{}"), Network::Lighter, clock).with_read_token("ro:t");
    let call = Call::post("setAccountMetadata").with_json(r#"{"a":1}"#.to_owned()).authed();
    block_on(client.call(call)).unwrap();
    let sent = &client.http().sent()[0];
    assert_eq!(
        (sent.header("content-type"), sent.body.as_slice()),
        (Some("application/json"), br#"{"a":1}"#.as_slice())
    );
    assert_eq!(sent.header("authorization"), Some("ro:t"));
}
