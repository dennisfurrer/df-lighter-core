//! A lighter-go case read the way a caller holds one: its fields by their Go names, its signer as
//! an [`ApiKey`] on a [`Network`], its attributes through the typed setters, and the random draws
//! lighter-go took, handed back in order. [`signs_as_lighter_go`] holds one signed transaction to
//! the file; [`every`] runs every case of one transaction type, signed and refused.

use std::cell::RefCell;
use std::collections::VecDeque;

use df_lighter_core::Network;
use df_lighter_core::account::{ApiKey, verify};
use df_lighter_core::utils::sign::{
    Attributes, Fp5, PublicKey, SelfTrade, SelfTradeScope, SignTxError, Signature, Signed, Tx,
};
use df_lighter_core::utils::{AccountIndex, ApiKeyIndex, AssetId, BadId, MarketId, Route, base64, hex};
use serde_json::Value;

use crate::support::{owner, owner_address, secret, vectors};

/// A string at `key`.
pub fn text<'a>(json: &'a Value, key: &str) -> &'a str {
    json[key].as_str().unwrap_or_else(|| panic!("no text at {key}: {json}"))
}

/// A whole number at `key`.
pub fn number(json: &Value, key: &str) -> u64 {
    json[key].as_u64().unwrap_or_else(|| panic!("no whole number at {key}: {json}"))
}

/// The random stream lighter-go read, one 40-byte draw at a time.
pub struct Draws(RefCell<VecDeque<[u8; 40]>>);

impl Draws {
    pub fn of(list: &Value) -> Draws {
        let draws = list.as_array().map(Vec::as_slice).unwrap_or_default();
        Draws(RefCell::new(draws.iter().map(|draw| hex::decode_array::<40>(draw.as_str().unwrap()).unwrap()).collect()))
    }

    /// Hands out the next draw as `crypto/rand.Reader` gave it to lighter-go.
    pub fn draw(&self) -> impl FnMut(&mut [u8; 40]) -> Result<(), String> + '_ {
        move |buffer: &mut [u8; 40]| {
            *buffer = self.0.borrow_mut().pop_front().ok_or_else(|| "lighter-go drew no more".to_owned())?;
            Ok(())
        }
    }

    /// Draws not taken yet.
    pub fn left(&self) -> usize {
        self.0.borrow().len()
    }
}

/// A transaction's fields by their Go names, or one order's inside a group.
#[derive(Clone, Copy)]
pub struct Fields<'a>(&'a Value);

impl<'a> Fields<'a> {
    pub fn int(self, key: &str) -> u64 {
        number(self.0, key)
    }

    /// A field Go keeps narrower than 64 bits.
    pub fn narrow<T: TryFrom<u64>>(self, key: &str) -> T {
        T::try_from(self.int(key)).unwrap_or_else(|_| panic!("{key} does not fit: {}", self.0))
    }

    /// A `uint8` that is 0 or 1.
    pub fn flag(self, key: &str) -> bool {
        match self.int(key) {
            0 => false,
            1 => true,
            other => panic!("{key} is {other}, not a flag"),
        }
    }

    pub fn text(self, key: &str) -> &'a str {
        text(self.0, key)
    }

    pub fn list(self, key: &str) -> Vec<Fields<'a>> {
        self.0[key].as_array().unwrap_or_else(|| panic!("no list at {key}")).iter().map(Fields).collect()
    }

    pub fn is_empty(self) -> bool {
        self.0.as_object().is_some_and(serde_json::Map::is_empty)
    }

    pub fn market(self, key: &str) -> Result<MarketId, BadId> {
        MarketId::new(self.int(key))
    }

    pub fn account(self, key: &str) -> Result<AccountIndex, BadId> {
        AccountIndex::new(self.int(key))
    }

    pub fn asset(self, key: &str) -> Result<AssetId, BadId> {
        AssetId::new(self.narrow(key))
    }

    /// A route type: 0 perps, 1 spot.
    pub fn route(self, key: &str) -> Route {
        let code: u8 = self.narrow(key);
        [Route::Perps, Route::Spot].into_iter().find(|route| route.code() == code).unwrap()
    }
}

/// Who signs a case, and where.
pub struct Signer {
    pub key: ApiKey,
    pub network: Network,
    /// The public key the file publishes with the private one.
    pub public: PublicKey,
}

impl Signer {
    pub fn of(signer: &Value) -> Signer {
        let network = match text(signer, "network") {
            "lighter" => Network::Lighter,
            "robinhood" => Network::Robinhood,
            other => panic!("no network {other}"),
        };
        assert_eq!(
            u64::from(network.chain_id()),
            number(signer, "chain_id"),
            "{network}: the chain lighter-go signed for"
        );
        let name = text(signer, "key");
        let account = AccountIndex::new(number(signer, "account_index")).unwrap();
        let slot = ApiKeyIndex::new(u8::try_from(number(signer, "api_key_index")).unwrap()).unwrap();
        let key = ApiKey::new(account, slot, secret(name));
        let public = PublicKey::from_hex(text(&vectors()["keys"][name], "public_key")).unwrap();
        assert_eq!(key.public_key(), public, "key {name}: the public key on file is the private one's");
        Signer { key, network, public }
    }

    pub fn chain_id(&self) -> u32 {
        self.network.chain_id()
    }
}

/// One entry of the file's `cases`.
pub struct Case(Value);

impl Case {
    pub fn name(&self) -> &str {
        text(&self.0, "name")
    }

    pub fn fields(&self) -> Fields<'_> {
        Fields(&self.0["fields"])
    }

    pub fn signer(&self) -> Signer {
        Signer::of(&self.0["signer"])
    }

    pub fn nonce(&self) -> u64 {
        number(&self.0, "nonce")
    }

    pub fn expired_at(&self) -> u64 {
        number(&self.0, "expired_at")
    }

    pub fn draws(&self) -> Draws {
        Draws::of(&self.0["draws"])
    }

    /// What lighter-go wrote, for a case it signed.
    pub fn out(&self) -> Option<&Value> {
        self.0.get("out")
    }

    /// Why lighter-go refused it, for a case it refused.
    pub fn error(&self) -> Option<&str> {
        self.0.get("error").and_then(Value::as_str)
    }

    /// The attributes lighter-go was handed, each a type and its value, by type.
    pub fn attributes(&self) -> Vec<(u8, u64)> {
        let mut given: Vec<(u8, u64)> = self.0["attributes"]
            .as_object()
            .map(|map| map.iter().map(|(kind, value)| (kind.parse().unwrap(), value.as_u64().unwrap())).collect())
            .unwrap_or_default();
        given.sort_unstable();
        given
    }

    /// Signs `tx` as the case's signer with lighter-go's draws: whatever [`ApiKey::sign_drawing`]
    /// answers.
    pub fn sign(&self, tx: &(impl Tx + ?Sized)) -> (Result<Signed, SignTxError>, usize) {
        let signer = self.signer();
        let draws = self.draws();
        let signed = signer.key.sign_drawing(tx, signer.chain_id(), self.nonce(), self.expired_at(), draws.draw());
        (signed, draws.left())
    }
}

/// Every case of `kind`, in the file's order.
pub fn cases(kind: &str) -> Vec<Case> {
    vectors()["cases"].as_array().unwrap().iter().filter(|case| case["kind"] == kind).cloned().map(Case).collect()
}

/// The attributes a case carries, made through [`Attributes`]' typed setters; they must give back
/// exactly the entries lighter-go was handed.
pub fn attributes(case: &Case) -> Result<Attributes, BadId> {
    let given = case.attributes();
    let value = |kind: u8| given.iter().find(|(entry, _)| *entry == kind).map(|(_, value)| *value);
    let fee = |kind: u8| u32::try_from(value(kind).unwrap_or(0)).unwrap();
    let mut attributes = Attributes::NONE;
    if let Some(integrator) = value(1) {
        attributes = attributes.integrator(AccountIndex::new(integrator)?, fee(2), fee(3));
    }
    if value(4) == Some(1) {
        attributes = attributes.skip_nonce();
    }
    if let Some(market) = value(5) {
        attributes = attributes.cancel_market(MarketId::new(market)?);
    }
    if value(6).is_some() || value(7).is_some() {
        let behavior = [SelfTrade::ExpireMaker, SelfTrade::ExpireTaker, SelfTrade::ExpireBoth, SelfTrade::Reduce]
            .into_iter()
            .find(|behavior| u64::from(behavior.code()) == value(6).unwrap_or(0))
            .unwrap();
        let scope = [SelfTradeScope::Account, SelfTradeScope::Owner]
            .into_iter()
            .find(|scope| u64::from(scope.code()) == value(7).unwrap_or(0))
            .unwrap();
        attributes = attributes.self_trade(behavior, scope);
    }
    if let Some(version) = value(8) {
        attributes = attributes.order_version(version);
    }
    assert_eq!(attributes.entries(), given, "{}: the attributes", case.name());
    Ok(attributes)
}

/// Signs `tx` as the case's signer with lighter-go's draws and holds the type, hash and `tx_info`
/// to the file byte for byte; where lighter-go asked the owner, the text, the owner's signature
/// (Hardhat's first account, `personal_sign`) and the `tx_info` sent too. Every draw lighter-go
/// took is taken, no more, and lighter-go's own signature verifies with the public key alone.
pub fn signs_as_lighter_go(case: &Case, tx: &(impl Tx + ?Sized)) -> Signed {
    let name = case.name();
    let out = case.out().unwrap_or_else(|| panic!("{name}: lighter-go refused it: {:?}", case.error()));
    let (signed, left) = case.sign(tx);
    let signed = signed.unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(left, 0, "{name}: every draw lighter-go took, and no more");
    assert_eq!(u64::from(signed.tx_type().code()), number(out, "tx_type"), "{name}: tx_type");
    assert_eq!(signed.hash().to_hex(), text(out, "tx_hash"), "{name}: tx_hash");
    assert_eq!(signed.tx_info(), text(out, "tx_info"), "{name}: tx_info");
    assert_eq!(signed.nonce(), case.nonce(), "{name}: nonce");
    assert_eq!(signed.l1_message(), out.get("message_to_sign").and_then(Value::as_str), "{name}: the owner's text");
    if let Some(message) = signed.l1_message() {
        let l1 = owner().sign_text(message).unwrap();
        assert_eq!(l1.to_hex(), text(out, "l1_sig"), "{name}: the owner's signature");
        assert_eq!(verify(message, &l1, owner_address()), Ok(()), "{name}: it recovers to the owner");
        let sent = signed.tx_info_with_l1(&l1.to_hex());
        assert_eq!(sent.as_deref(), Some(text(out, "signed_tx_info")), "{name}: the tx_info sent");
    }

    let info: Value = serde_json::from_str(text(out, "tx_info")).unwrap();
    let bytes: [u8; 80] = base64::decode(text(&info, "Sig")).unwrap().try_into().unwrap();
    let signature = Signature::from_bytes(bytes);
    let hash = Fp5::from_le_bytes(&hex::decode_array::<40>(text(out, "tx_hash")).unwrap()).unwrap();
    assert!(case.signer().public.verifies(hash, &signature), "{name}: lighter-go's signature, by the public key alone");
    assert_eq!((signed.hash().element(), signed.signature()), (hash, Some(signature)), "{name}");
    signed
}

/// Why this crate refuses a case lighter-go refused.
#[derive(Debug)]
pub enum Refused {
    /// The id lighter-go was handed is not one: its constructor will not make it.
    Id(BadId),
    /// Signing refuses it before anything is hashed, naming this field.
    Field(&'static str),
}

/// Every case of `kind`: exactly `names`, in the file's order. Each one lighter-go signed is built
/// by `build` from its fields and signed as lighter-go signed it ([`signs_as_lighter_go`]); each
/// one it refused is refused as `refused` says, and nothing is signed.
pub fn every<T: Tx>(
    kind: &str,
    names: &[&str],
    refused: &[(&str, Refused)],
    build: impl Fn(&Case) -> Result<T, BadId>,
) {
    let cases = cases(kind);
    assert_eq!(cases.iter().map(Case::name).collect::<Vec<_>>(), names, "{kind}: every case on file");
    for case in &cases {
        let name = case.name();
        let why = refused.iter().find(|(refused, _)| *refused == name).map(|(_, why)| why);
        match (case.error(), why) {
            (None, None) => {
                let tx = build(case).unwrap_or_else(|error| panic!("{name}: {error}"));
                signs_as_lighter_go(case, &tx);
            }
            (Some(error), Some(Refused::Id(bad))) => {
                assert_eq!(build(case).err().as_ref(), Some(bad), "{name}: lighter-go says {error}");
            }
            (Some(error), Some(Refused::Field(field))) => {
                let tx = build(case).unwrap_or_else(|bad| panic!("{name}: {bad}"));
                match case.sign(&tx).0 {
                    Err(SignTxError::Bad(bad)) => assert_eq!(bad.field, *field, "{name}: lighter-go says {error}"),
                    other => panic!("{name}: lighter-go refused it ({error}), not this crate: {other:?}"),
                }
            }
            (error, why) => panic!("{name}: lighter-go's verdict is {error:?}, the refusal expected {why:?}"),
        }
    }
}
