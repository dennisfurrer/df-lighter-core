//! The envelope, attributes, hash and `tx_info` every transaction shares, held to lighter-go's
//! orders and cancels (`vectors/lighter_go.json`) through a bare transaction written here. Every
//! type the crate signs is held to the whole file by the conformance suite.

use std::cell::RefCell;
use std::collections::VecDeque;

use serde_json::Value;

use super::*;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::schnorr::SecretKey;

fn vectors() -> Value {
    serde_json::from_str(include_str!("../../../../vectors/lighter_go.json")).unwrap()
}

/// A transaction from the vector's own fields: integers in lighter-go's order.
struct Raw {
    tx_type: TxType,
    fields: Vec<(&'static str, i64)>,
    attributes: Attributes,
}

impl Tx for Raw {
    fn tx_type(&self) -> TxType {
        self.tx_type
    }

    fn hash_fields(&self) -> Vec<Fp> {
        self.fields.iter().map(|(_, value)| Fp::new(u64::try_from(*value).unwrap())).collect()
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        self.fields.iter().map(|(key, value)| (*key, number(*value))).collect()
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }
}

const ORDER: [&str; 10] = [
    "MarketIndex",
    "ClientOrderIndex",
    "BaseAmount",
    "Price",
    "IsAsk",
    "Type",
    "TimeInForce",
    "ReduceOnly",
    "TriggerPrice",
    "OrderExpiry",
];

fn raw(case: &Value) -> Option<Raw> {
    let fields = &case["fields"];
    let (tx_type, keys): (TxType, &[&'static str]) = match case["kind"].as_str()? {
        "CreateOrder" => (TxType::CreateOrder, &ORDER),
        "CancelOrder" => (TxType::CancelOrder, &["MarketIndex", "Index"]),
        "UpdateLeverage" => (TxType::UpdateLeverage, &["MarketIndex", "InitialMarginFraction", "MarginMode"]),
        _ => return None,
    };
    let mut attributes = Attributes::NONE;
    let attribute = |key: &str| case["attributes"][key].as_u64();
    if let Some(index) = attribute("1") {
        let fee = |key: &str| u32::try_from(attribute(key).unwrap_or(0)).unwrap();
        attributes = attributes.integrator(AccountIndex::new(index).unwrap(), fee("2"), fee("3"));
    }
    if attribute("4").is_some() {
        attributes = attributes.skip_nonce();
    }
    if let (Some(behavior), Some(scope)) = (attribute("6"), attribute("7")) {
        let behavior = [SelfTrade::ExpireMaker, SelfTrade::ExpireTaker, SelfTrade::ExpireBoth, SelfTrade::Reduce]
            [usize::try_from(behavior).unwrap()];
        let scope = [SelfTradeScope::Account, SelfTradeScope::Owner][usize::try_from(scope).unwrap()];
        attributes = attributes.self_trade(behavior, scope);
    }
    let fields = keys.iter().map(|key| (*key, fields[*key].as_i64().unwrap())).collect();
    Some(Raw { tx_type, fields, attributes })
}

fn envelope(case: &Value) -> (u32, Envelope) {
    let signer = &case["signer"];
    let chain_id = u32::try_from(signer["chain_id"].as_u64().unwrap()).unwrap();
    let envelope = Envelope {
        account: AccountIndex::new(signer["account_index"].as_u64().unwrap()).unwrap(),
        api_key: ApiKeyIndex::new(u8::try_from(signer["api_key_index"].as_u64().unwrap()).unwrap()).unwrap(),
        nonce: case["nonce"].as_u64().unwrap(),
        expired_at: case["expired_at"].as_u64().unwrap(),
    };
    (chain_id, envelope)
}

/// Hands out the recorded draws, in order.
fn replay(draws: &[Value]) -> impl FnMut(&mut [u8; 40]) -> Result<(), String> + use<> {
    let draws: RefCell<VecDeque<Vec<u8>>> =
        RefCell::new(draws.iter().map(|draw| crate::utils::hex::decode(draw.as_str().unwrap()).unwrap()).collect());
    move |buffer: &mut [u8; 40]| {
        let next = draws.borrow_mut().pop_front().ok_or_else(|| "no draws left".to_owned())?;
        buffer.copy_from_slice(&next);
        Ok(())
    }
}

#[test]
fn orders_cancels_and_leverage_hash_sign_and_write_as_lighter_go_does() {
    let vectors = vectors();
    let mut checked = 0;
    for case in vectors["cases"].as_array().unwrap().iter().filter(|case| case.get("out").is_some()) {
        let Some(tx) = raw(case) else { continue };
        let name = case["name"].as_str().unwrap();
        let (chain_id, envelope) = envelope(case);
        let key = SecretKey::from_hex(
            vectors["keys"][case["signer"]["key"].as_str().unwrap()]["private_key"].as_str().unwrap(),
        )
        .unwrap();
        let signed = sign_drawing(&tx, chain_id, &envelope, &key, replay(case["draws"].as_array().unwrap()))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(signed.hash().to_hex(), case["out"]["tx_hash"].as_str().unwrap(), "{name}: hash");
        assert_eq!(signed.tx_info(), case["out"]["tx_info"].as_str().unwrap(), "{name}: tx_info");
        assert_eq!(signed.tx_type().code(), u8::try_from(case["out"]["tx_type"].as_u64().unwrap()).unwrap());
        checked += 1;
    }
    assert_eq!(checked, 25);
}

#[test]
fn public_keys_are_what_lighter_go_derives() {
    let vectors = vectors();
    for name in ["a", "b"] {
        let key = &vectors["keys"][name];
        let secret = SecretKey::from_hex(key["private_key"].as_str().unwrap()).unwrap();
        assert_eq!(secret.public_key().to_hex(), key["public_key"].as_str().unwrap(), "{name}");
        assert_eq!(secret.to_hex().as_str(), key["private_key"].as_str().unwrap(), "{name}");
        let generated = SecretKey::generate_drawing(replay(key["draws"].as_array().unwrap())).unwrap();
        assert_eq!(generated, secret, "{name}: drawn as lighter-go draws");
    }
}

#[test]
fn a_type_is_its_number_and_back() {
    for tx_type in TxType::ALL {
        assert_eq!(TxType::from_code(tx_type.code()), Some(tx_type));
    }
    assert_eq!(TxType::from_code(7), None, "an L1 order is the contract's, not ours");
    assert!(TxType::CreateOrder.spends_quota() && !TxType::CancelOrder.spends_quota());
}

#[test]
fn attributes_keep_only_what_is_not_their_default() {
    let integrator = AccountIndex::new(713_888).unwrap();
    assert!(Attributes::NONE.is_none());
    assert_eq!(Attributes::NONE.json(), Json::Null);
    let tracked = Attributes::NONE.integrator(integrator, 0, 0);
    assert_eq!(tracked.entries(), [(1, 713_888)]);
    assert_eq!(tracked.json().compact(), r#"{"1":713888}"#);
    let charged = Attributes::NONE.integrator(integrator, 500, 200).skip_nonce();
    assert_eq!(charged.json().compact(), r#"{"1":713888,"2":500,"3":200,"4":1}"#);
    assert_eq!(charged.check(), Ok(()));
    let five = charged.order_version(3);
    assert!(five.check().is_err(), "at most four ride on one transaction");
    let both =
        Attributes::NONE.integrator(integrator, 1, 0).self_trade(SelfTrade::ExpireTaker, SelfTradeScope::Account);
    assert!(both.check().is_err(), "self-trade handling and fees do not mix");
    let unowned = Attributes::NONE.integrator(AccountIndex::new(0).unwrap(), 5, 0);
    assert!(unowned.check().is_err(), "fees need an integrator");
    let reduce = Attributes::NONE.self_trade(SelfTrade::Reduce, SelfTradeScope::Owner);
    assert!(reduce.check().is_err());
    assert_eq!(Attributes::NONE.self_trade(SelfTrade::ExpireMaker, SelfTradeScope::Account), Attributes::NONE);
}

#[test]
fn a_signature_that_does_not_verify_is_never_returned() {
    let key = SecretKey::from_le_bytes(&[7; 40]);
    let tx = Raw {
        tx_type: TxType::CancelOrder,
        fields: vec![("MarketIndex", 1), ("Index", 9)],
        attributes: Attributes::NONE,
    };
    let envelope = Envelope {
        account: AccountIndex::new(1).unwrap(),
        api_key: ApiKeyIndex::new(4).unwrap(),
        nonce: 0,
        expired_at: 1,
    };
    let signed = sign_drawing(&tx, 304, &envelope, &key, |draw: &mut [u8; 40]| {
        draw.fill(3);
        Ok::<(), String>(())
    })
    .unwrap();
    let signature = signed.signature().unwrap();
    assert!(key.verifies(signed.hash().element(), &signature));
    assert!(!SecretKey::from_le_bytes(&[8; 40]).verifies(signed.hash().element(), &signature), "another key's");
    let mut bytes = *signature.bytes();
    bytes[3] ^= 1;
    assert!(!key.verifies(signed.hash().element(), &Signature::from_bytes(bytes)), "a flipped bit");
    let late = Envelope { expired_at: 1 << 48, ..envelope };
    assert!(matches!(
        sign_drawing(&tx, 304, &late, &key, |_: &mut [u8; 40]| Ok::<(), String>(())),
        Err(SignTxError::Bad(_))
    ));
}
