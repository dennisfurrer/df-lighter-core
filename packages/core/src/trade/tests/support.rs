//! What the trade tests share: lighter-go's cases (`vectors/lighter_go.json`) read into this
//! module's typed transactions, the check that one signs byte for byte as lighter-go signed it,
//! and the form a send posted, decoded.

use serde_json::Value;

use crate::test_support::{replay, vector_envelope, vector_key, vectors};
use crate::trade::orders::Order;
use crate::utils::sign::tx::{Attributes, SelfTrade, SelfTradeScope, Tx, sign_drawing};
use crate::utils::{AccountIndex, ClientOrderIndex, Dec, MarketId, OrderKind, Side, TimeInForce};

/// The case named `name`.
pub(crate) fn case(name: &str) -> Value {
    vectors()["cases"]
        .as_array()
        .and_then(|cases| cases.iter().find(|case| case["name"] == name))
        .cloned()
        .unwrap_or_else(|| panic!("no case named {name}"))
}

/// Every case of `kind`, signed ones and refused ones.
pub(crate) fn cases(kind: &str) -> Vec<Value> {
    vectors()["cases"].as_array().unwrap().iter().filter(|case| case["kind"] == kind).cloned().collect()
}

/// A field as an integer.
pub(crate) fn int(fields: &Value, key: &str) -> u64 {
    fields[key].as_u64().unwrap_or_else(|| panic!("{key} is not a whole number: {fields}"))
}

pub(crate) fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

/// The attributes a case carries, through the typed setters.
pub(crate) fn attributes(case: &Value) -> Attributes {
    let given = |key: &str| case["attributes"][key].as_u64();
    let mut attributes = Attributes::NONE;
    if let Some(account) = given("1") {
        let fee = |key: &str| u32::try_from(given(key).unwrap_or(0)).unwrap();
        attributes = attributes.integrator(AccountIndex::new(account).unwrap(), fee("2"), fee("3"));
    }
    if given("4").is_some() {
        attributes = attributes.skip_nonce();
    }
    if let Some(market) = given("5") {
        attributes = attributes.cancel_market(MarketId::new(market).unwrap());
    }
    if given("6").is_some() || given("7").is_some() {
        let behavior = [SelfTrade::ExpireMaker, SelfTrade::ExpireTaker, SelfTrade::ExpireBoth, SelfTrade::Reduce]
            [usize::try_from(given("6").unwrap_or(0)).unwrap()];
        let scope = [SelfTradeScope::Account, SelfTradeScope::Owner][usize::try_from(given("7").unwrap_or(0)).unwrap()];
        attributes = attributes.self_trade(behavior, scope);
    }
    if let Some(version) = given("8") {
        attributes = attributes.order_version(version);
    }
    attributes
}

/// An order from lighter-go's field names.
pub(crate) fn order(fields: &Value) -> Order {
    let small = |key: &str| u8::try_from(int(fields, key)).unwrap();
    let client = int(fields, "ClientOrderIndex");
    Order {
        market: MarketId::new(int(fields, "MarketIndex")).unwrap(),
        client_index: (client != 0).then(|| ClientOrderIndex::new(client).unwrap()),
        base_amount: int(fields, "BaseAmount"),
        price: u32::try_from(int(fields, "Price")).unwrap(),
        side: Side::from_is_ask(small("IsAsk") == 1),
        kind: OrderKind::from_code(small("Type")).unwrap(),
        time_in_force: TimeInForce::from_code(small("TimeInForce")).unwrap(),
        reduce_only: small("ReduceOnly") == 1,
        trigger_price: u32::try_from(int(fields, "TriggerPrice")).unwrap(),
        expiry: int(fields, "OrderExpiry"),
    }
}

/// `tx` signed with the case's key, envelope and draws hashes, writes and types as lighter-go's
/// did.
pub(crate) fn signs_as_lighter_go(tx: &(impl Tx + ?Sized), case: &Value) {
    let name = case["name"].as_str().unwrap();
    let (chain_id, envelope) = vector_envelope(case);
    let signed = sign_drawing(tx, chain_id, &envelope, &vector_key(case), replay(case))
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let out = &case["out"];
    assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
    assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
    assert_eq!(u64::from(signed.tx_type().code()), out["tx_type"].as_u64().unwrap(), "{name}: type");
}

/// Whether a case is one lighter-go signed, rather than refused.
pub(crate) fn signed(case: &Value) -> bool {
    case.get("out").is_some()
}

/// The fields a posted form carries, decoded.
#[cfg(feature = "sign")]
pub(crate) fn form_fields(body: &[u8]) -> Vec<(String, String)> {
    let decode = |text: &str| {
        let bytes = text.replace('+', " ").into_bytes();
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'%' => {
                    out.push(u8::from_str_radix(std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap(), 16).unwrap());
                    at += 3;
                }
                byte => {
                    out.push(byte);
                    at += 1;
                }
            }
        }
        String::from_utf8(out).unwrap()
    };
    String::from_utf8_lossy(body)
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

/// The named field of a posted form.
#[cfg(feature = "sign")]
pub(crate) fn form_field(body: &[u8], key: &str) -> String {
    form_fields(body).into_iter().find(|(name, _)| name == key).unwrap_or_else(|| panic!("no {key}")).1
}
