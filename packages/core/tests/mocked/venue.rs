//! The fake venue itself, held to lighter-go: every vector of a kind it checks is taken, with
//! the hash lighter-go signed, and anything changed after signing is refused.

use df_lighter_core::client::{Http, Network, Request};
use df_lighter_core::utils::Json;
use df_lighter_core::utils::sign::SecretKey;
use serde_json::Value;

use crate::support::venue::Venue;
use crate::support::{owner_address, vectors};

/// The kinds the venue rebuilds the hash of.
const CHECKED: [&str; 11] = [
    "ChangePubKey",
    "CreateSubAccount",
    "Transfer",
    "Withdraw",
    "CreateOrder",
    "CancelOrder",
    "CancelAllOrders",
    "ModifyOrder",
    "MintShares",
    "BurnShares",
    "UpdateLeverage",
];

fn post(venue: &Venue, network: Network, tx_type: u64, tx_info: &str) -> Value {
    let url = format!("{}/api/v1/sendTx", network.api_url());
    let request = Request::post_form(url, &[("tx_type", tx_type.to_string()), ("tx_info", tx_info.to_owned())]);
    let answer = futures_lite(venue.send(request));
    serde_json::from_slice(&answer.unwrap().body).unwrap()
}

/// The venue answers at once.
fn futures_lite<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("the venue never waits"),
    }
}

/// A venue that holds the case's key at its slot, at the case's nonce, and knows the owner.
fn venue_for(case: &Value, network: Network) -> Venue {
    let vectors = vectors();
    let signer = &case["signer"];
    let account = signer["account_index"].as_u64().unwrap();
    let slot = u8::try_from(signer["api_key_index"].as_u64().unwrap()).unwrap();
    let secret =
        SecretKey::from_hex(vectors["keys"][signer["key"].as_str().unwrap()]["private_key"].as_str().unwrap()).unwrap();
    let nonce = case["nonce"].as_u64().unwrap();
    let venue = Venue::on(network).account(account, owner_address());
    match case["kind"] == "ChangePubKey" {
        true => venue,
        false => venue.key(account, slot, secret.public_key(), nonce),
    }
}

#[test]
fn every_vector_of_a_checked_kind_is_taken_with_lighter_go_s_hash() {
    let mut checked = 0;
    for case in vectors()["cases"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        if !CHECKED.contains(&kind) || case.get("out").is_none() {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let network = case["signer"]["network"].as_str().unwrap().parse::<Network>().unwrap();
        let out = &case["out"];
        let info =
            out.get("signed_tx_info").and_then(Value::as_str).unwrap_or_else(|| out["tx_info"].as_str().unwrap());
        let venue = venue_for(case, network);
        let answer = post(&venue, network, out["tx_type"].as_u64().unwrap(), info);
        assert_eq!(answer["code"], 200, "{name}: {answer}");
        assert_eq!(answer["tx_hash"], out["tx_hash"], "{name}");
        checked += 1;
    }
    assert!(checked >= 40, "{checked} cases");
}

/// A field changed after signing is a different hash; a registration or a transfer to another
/// owner without the owner's signature is refused.
#[test]
fn what_was_not_signed_is_refused() {
    let case =
        vectors()["cases"].as_array().unwrap().iter().find(|case| case["name"] == "limit-buy-gtt").unwrap().clone();
    let venue = venue_for(&case, Network::Lighter);
    let info = case["out"]["tx_info"].as_str().unwrap();
    let price = Json::parse(info).unwrap().get("Price").and_then(Json::as_u64).unwrap();
    let changed = info.replace(&format!("\"Price\":{price}"), &format!("\"Price\":{}", price + 1));
    assert_eq!(post(&venue, Network::Lighter, 14, &changed)["code"], 21120);
    let robinhood = venue_for(&case, Network::Robinhood);
    assert_eq!(post(&robinhood, Network::Robinhood, 14, info)["code"], 21120, "signed for the other chain");

    for name in ["register-key-a", "transfer-to-another-owner"] {
        let case = vectors()["cases"].as_array().unwrap().iter().find(|case| case["name"] == name).unwrap().clone();
        let venue = venue_for(&case, Network::Lighter)
            .account(713_888, "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf".parse().unwrap());
        let unsigned = case["out"]["tx_info"].as_str().unwrap();
        assert_eq!(
            post(&venue, Network::Lighter, case["out"]["tx_type"].as_u64().unwrap(), unsigned)["code"],
            21120,
            "{name}"
        );
    }
}
