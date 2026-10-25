//! A fake Lighter behind the [`Http`] wire, for whole journeys through a [`Client`]:
//!
//! - GETs under `/api/v1` answer from the recording they were given for that path; `nextNonce`
//!   and `apikeys` answer from the venue's own keys, as the venue does.
//! - `sendTx` and `sendTxBatch` check every transaction the way the venue's API does before it
//!   answers: the expiry is at least five seconds away, the key is registered at its slot, the
//!   nonce is the slot's next, and the signature verifies against the registered public key over
//!   the hash rebuilt here from `tx_info` exactly as posted, in lighter-go's field order. A key
//!   registration verifies against the new key and needs the owner's `personal_sign` over the
//!   venue's own text; so does a transfer to another owner. A refusal answers with the venue's
//!   code and leaves the nonce where it was; a taken transaction moves it on.
//! - A refusal, a transaction the sequencer drops before applying it (the nonce does not move),
//!   an HTTP status or no answer at all can stand in for the next answer.
//!
//! Anything it was not given an answer for fails the test, naming the request.
//!
//! [`Client`]: df_lighter_core::Client

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::{Future, ready};

use df_lighter_core::account::{L1Signature, recover};
use df_lighter_core::client::{Http, Method, Network, Request, Response, TransportError};
use df_lighter_core::utils::sign::tx::{Envelope, hex16, tx_hash};
use df_lighter_core::utils::sign::{Attributes, Fp, PublicKey, SelfTrade, SelfTradeScope, Signature, Tx, TxType};
use df_lighter_core::utils::{AccountIndex, Address, ApiKeyIndex, Json, MarketId, base64, hex};

use crate::support::{clock, fixture};

/// What the wire hands back.
pub type Answer = Result<Response, TransportError>;

/// A recorded answer: the path it answers, the one query field it waits for if any, its text.
type Recording = (String, Option<(String, String)>, String);

/// The API refuses a transaction lapsing sooner than this.
const LEAST_LIFE_MS: u64 = 5_000;

/// A transaction the venue took.
#[derive(Debug, Clone)]
pub struct Taken {
    pub tx_type: u8,
    /// `tx_info` as posted.
    pub info: Json,
    /// The hash the venue rebuilt and the key signed, 80 hex digits.
    pub hash: String,
}

impl Taken {
    /// A field of `tx_info` as a number.
    pub fn number(&self, key: &str) -> u64 {
        self.info.get(key).and_then(Json::as_u64).unwrap_or_else(|| panic!("no number {key} in {:?}", self.info))
    }
}

#[derive(Debug, Clone, Copy)]
struct Key {
    public: PublicKey,
    next: u64,
}

pub struct Venue {
    network: Network,
    now: fn() -> u64,
    reads: Vec<Recording>,
    keys: RefCell<HashMap<(u64, u8), Key>>,
    owners: HashMap<u64, Address>,
    refusals: RefCell<VecDeque<(i64, String)>>,
    drops: RefCell<u32>,
    failures: RefCell<VecDeque<(String, Answer)>>,
    sent: RefCell<Vec<Request>>,
    taken: RefCell<Vec<Taken>>,
}

impl Venue {
    pub fn lighter() -> Venue {
        Venue::on(Network::Lighter)
    }

    pub fn on(network: Network) -> Venue {
        Venue {
            network,
            now: clock,
            reads: Vec::new(),
            keys: RefCell::default(),
            owners: HashMap::new(),
            refusals: RefCell::default(),
            drops: RefCell::new(0),
            failures: RefCell::default(),
            sent: RefCell::default(),
            taken: RefCell::default(),
        }
    }

    /// The venue's clock, for expiries; the support's pinned [`clock`] unless given.
    #[must_use]
    pub fn at(mut self, now: fn() -> u64) -> Venue {
        self.now = now;
        self
    }

    /// Answers every GET of `path` (under `/api/v1/`, or from the root when it starts with `/`)
    /// with the recording `file` under `fixtures/`.
    #[must_use]
    pub fn read(self, path: &str, file: &str) -> Venue {
        self.answer(path, None, fixture(file))
    }

    /// Answers a GET of `path` whose `field` is `value` with the recording `file`, ahead of any
    /// answer for the path at large.
    #[must_use]
    pub fn read_where(self, path: &str, field: &str, value: &str, file: &str) -> Venue {
        self.answer(path, Some((field.to_owned(), value.to_owned())), fixture(file))
    }

    /// Answers every GET of `path` with `text`.
    #[must_use]
    pub fn text(self, path: &str, text: &str) -> Venue {
        self.answer(path, None, text.to_owned())
    }

    fn answer(mut self, path: &str, when: Option<(String, String)>, text: String) -> Venue {
        let at = match when.is_some() {
            true => 0,
            false => self.reads.len(),
        };
        self.reads.insert(at, (path.trim_start_matches("/api/v1/").to_owned(), when, text));
        self
    }

    /// `account` is owned by `owner`: its key registrations and transfers to other owners need
    /// that wallet's signature.
    #[must_use]
    pub fn account(mut self, account: u64, owner: Address) -> Venue {
        self.owners.insert(account, owner);
        self
    }

    /// `public` is registered at `slot` of `account`, its next nonce `next`.
    #[must_use]
    pub fn key(self, account: u64, slot: u8, public: PublicKey, next: u64) -> Venue {
        self.keys.borrow_mut().insert((account, slot), Key { public, next });
        self
    }

    /// The next transaction that passes the checks is refused with `code` anyway, as the API
    /// refuses one for margin; its nonce does not move.
    #[must_use]
    pub fn refuse(self, code: i64, message: &str) -> Venue {
        self.refusals.borrow_mut().push_back((code, message.to_owned()));
        self
    }

    /// The next transaction taken is dropped by the sequencer before it applies (its expiry
    /// passed, say): the API answered it, but its nonce does not move.
    #[must_use]
    pub fn drop_next(self) -> Venue {
        *self.drops.borrow_mut() += 1;
        self
    }

    /// The next request to a path ending in `path` is answered HTTP `status` with `body`.
    #[must_use]
    pub fn status(self, path: &str, status: u16, body: &str) -> Venue {
        self.failing(path, Ok(Response { status, headers: Vec::new(), body: body.as_bytes().to_vec() }))
    }

    /// The next request to a path ending in `path` is taken, and checked, but its answer never
    /// comes back.
    #[must_use]
    pub fn unreachable(self, path: &str) -> Venue {
        self.failing(path, Err(TransportError::Unreachable("the connection was reset".to_owned())))
    }

    fn failing(self, path: &str, answer: Answer) -> Venue {
        self.failures.borrow_mut().push_back((path.to_owned(), answer));
        self
    }

    /// Every request, in order.
    pub fn sent(&self) -> Vec<Request> {
        self.sent.borrow().clone()
    }

    /// Each request's path and query, without the host.
    pub fn paths(&self) -> Vec<String> {
        let api = self.network.api_url();
        self.sent().iter().map(|request| request.url.trim_start_matches(api).to_owned()).collect()
    }

    /// The transactions taken, in order.
    pub fn taken(&self) -> Vec<Taken> {
        self.taken.borrow().clone()
    }

    /// The key at `slot` of `account`, and its next nonce, as the venue holds them.
    pub fn registered(&self, account: u64, slot: u8) -> Option<(PublicKey, u64)> {
        self.keys.borrow().get(&(account, slot)).map(|key| (key.public, key.next))
    }

    fn respond(&self, request: &Request) -> Answer {
        let api = self.network.api_url();
        let url = request.url.strip_prefix(api).unwrap_or_else(|| panic!("the venue does not serve {}", request.url));
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        let failure = self.failure(path);
        let answer = match (request.method, path.strip_prefix("/api/v1/")) {
            (Method::Get, Some(path)) => self.get(path, &fields(query)),
            (Method::Get, None) => self.get(path, &fields(query)),
            (Method::Post, Some("sendTx")) => self.send_tx(&fields(&String::from_utf8_lossy(&request.body))),
            (Method::Post, Some("sendTxBatch")) => self.send_batch(&fields(&String::from_utf8_lossy(&request.body))),
            (method, _) => panic!("the venue does not take a {method:?} to {path}"),
        };
        failure.unwrap_or(answer)
    }

    fn failure(&self, path: &str) -> Option<Answer> {
        let mut failures = self.failures.borrow_mut();
        let at = failures.iter().position(|(ending, _)| path.ends_with(ending.as_str()))?;
        failures.remove(at).map(|(_, answer)| answer)
    }

    fn get(&self, path: &str, query: &[(String, String)]) -> Answer {
        let value = |key: &str| query.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
        match path {
            "nextNonce" => {
                let (account, slot) = (number(value("account_index")), number(value("api_key_index")));
                let next = self.keys.borrow().get(&(account, small(slot))).map_or(0, |key| key.next);
                ok(format!(r#"{{"code":200,"nonce":{next}}}"#))
            }
            "apikeys" => self.api_keys(number(value("account_index")), number(value("api_key_index"))),
            "/" => ok(format!(r#"{{"status":200,"network_id":1,"timestamp":{}}}"#, (self.now)() / 1_000)),
            _ => {
                let found = self.reads.iter().find(|(read, when, _)| {
                    read == path && when.as_ref().is_none_or(|(key, wanted)| value(key) == Some(wanted.as_str()))
                });
                match found {
                    Some((_, _, text)) => ok(text.clone()),
                    None => panic!("the venue was given no answer for GET {path}?{query:?}"),
                }
            }
        }
    }

    fn api_keys(&self, account: u64, slot: u64) -> Answer {
        let keys = self.keys.borrow();
        let mut listed: Vec<(u8, Key)> = keys
            .iter()
            .filter(|((owner, at), _)| *owner == account && (slot == 255 || u64::from(*at) == slot))
            .map(|((_, at), key)| (*at, *key))
            .collect();
        listed.sort_by_key(|(at, _)| *at);
        if listed.is_empty() {
            return ok(r#"{"code":21109,"message":"api key not found"}"#.to_owned());
        }
        let rows: Vec<String> = listed
            .iter()
            .map(|(at, key)| {
                format!(
                    r#"{{"account_index":{account},"api_key_index":{at},"nonce":{},"public_key":"{}","transaction_time":0}}"#,
                    key.next,
                    key.public.to_hex().trim_start_matches("0x")
                )
            })
            .collect();
        ok(format!(r#"{{"code":200,"api_keys":[{}]}}"#, rows.join(",")))
    }

    fn send_tx(&self, form: &[(String, String)]) -> Answer {
        let field = |key: &str| form.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
        let tx_type = small(number(field("tx_type")));
        let info = Json::parse(field("tx_info").unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
        match self.check(tx_type, &info).and_then(|hash| self.take(tx_type, info, hash.clone()).map(|()| hash)) {
            Ok(hash) => ok(format!(
                r#"{{"code":200,"message":"{{\"ratelimit\": \"didn't use volume quota\"}}","tx_hash":"{hash}","predicted_execution_time_ms":{}}}"#,
                (self.now)() + 30
            )),
            Err((code, message)) => refused(code, &message),
        }
    }

    fn send_batch(&self, form: &[(String, String)]) -> Answer {
        let field = |key: &str| form.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
        let types = Json::parse(field("tx_types").unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
        let infos = Json::parse(field("tx_infos").unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
        let (types, infos) = (types.as_array().unwrap_or_default(), infos.as_array().unwrap_or_default());
        assert_eq!(types.len(), infos.len(), "one type a transaction");
        let mut hashes = Vec::new();
        for (tx_type, info) in types.iter().zip(infos) {
            let tx_type = small(tx_type.as_u64().unwrap_or(u64::MAX));
            let info = Json::parse(info.as_str().unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
            match self.check(tx_type, &info).and_then(|hash| self.take(tx_type, info, hash.clone()).map(|()| hash)) {
                Ok(hash) => hashes.push(format!("\"{hash}\"")),
                Err((code, message)) => return refused(code, &message),
            }
        }
        ok(format!(r#"{{"code":200,"tx_hash":[{}]}}"#, hashes.join(",")))
    }

    /// The API's checks, in its order; the hash the key signed.
    fn check(&self, tx_type: u8, info: &Json) -> Result<String, (i64, String)> {
        let kind = TxType::from_code(tx_type).ok_or((20001, format!("unknown tx type {tx_type}")))?;
        let account = info.get("AccountIndex").or_else(|| info.get("FromAccountIndex")).and_then(Json::as_u64);
        let account = account.ok_or((20001, "no account".to_owned()))?;
        let slot = small(info.get("ApiKeyIndex").and_then(Json::as_u64).unwrap_or(u64::MAX));
        let nonce = info.get("Nonce").and_then(Json::as_u64).unwrap_or(u64::MAX);
        let expired_at = info.get("ExpiredAt").and_then(Json::as_u64).unwrap_or(0);
        if expired_at < (self.now)() + LEAST_LIFE_MS {
            return Err((20011, "invalid expired time".to_owned()));
        }
        let registered = self.keys.borrow().get(&(account, slot)).copied();
        let (public, next) = match kind {
            TxType::ChangePubKey => {
                let bytes = base64::decode(info.get("PubKey").and_then(Json::as_str).unwrap_or_default());
                let key = bytes.ok().and_then(|bytes| PublicKey::from_bytes(bytes.try_into().ok()?).ok());
                (key.ok_or((21108, "invalid public key".to_owned()))?, registered.map_or(0, |key| key.next))
            }
            _ => {
                let key = registered.ok_or((21109, "api key not found".to_owned()))?;
                (key.public, key.next)
            }
        };
        if nonce != next {
            return Err((21104, "invalid nonce".to_owned()));
        }
        let posted = Posted::read(kind, info);
        let envelope = Envelope {
            account: AccountIndex::new(account).map_err(|error| (20001, error.to_string()))?,
            api_key: ApiKeyIndex::new(slot).map_err(|error| (20001, error.to_string()))?,
            nonce,
            expired_at,
        };
        let hash = tx_hash(&posted, self.network.chain_id(), &envelope);
        let signature = base64::decode(info.get("Sig").and_then(Json::as_str).unwrap_or_default())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .map(Signature::from_bytes);
        if !signature.is_some_and(|signature| public.verifies(hash.element(), &signature)) {
            return Err((21120, "invalid signature".to_owned()));
        }
        if let Some(message) = self.owner_message(kind, info, &envelope) {
            let owner = self.owners.get(&account).ok_or((21100, "account not found".to_owned()))?;
            let l1 = info.get("L1Sig").and_then(Json::as_str).unwrap_or_default();
            let signer = L1Signature::from_hex(l1).ok().and_then(|signature| recover(&message, &signature).ok());
            if signer != Some(*owner) {
                return Err((21120, "invalid l1 signature".to_owned()));
            }
        }
        if let Some((code, message)) = self.refusals.borrow_mut().pop_front() {
            return Err((code, message));
        }
        Ok(hash.to_hex())
    }

    /// The text the owner must have signed, when the venue asks for one: every key
    /// registration, and a transfer to an account of another owner. Rebuilt here from the
    /// venue's own templates.
    fn owner_message(&self, kind: TxType, info: &Json, envelope: &Envelope) -> Option<String> {
        let get = |key: &str| info.get(key).and_then(Json::as_u64).unwrap_or(0);
        match kind {
            TxType::ChangePubKey => {
                let key = base64::decode(info.get("PubKey").and_then(Json::as_str).unwrap_or_default()).ok()?;
                Some(format!(
                    "Register Lighter Account\n\npubkey: 0x{}\nnonce: {}\naccount index: {}\napi key index: {}\nOnly sign this message for a trusted client!",
                    hex::encode(&key),
                    hex16(envelope.nonce),
                    hex16(envelope.account.get()),
                    hex16(u64::from(envelope.api_key.get())),
                ))
            }
            TxType::Transfer => {
                let to = get("ToAccountIndex");
                let same =
                    self.owners.get(&to).is_some_and(|owner| self.owners.get(&envelope.account.get()) == Some(owner));
                if same || to == envelope.account.get() {
                    return None;
                }
                let memo: Vec<u8> = info
                    .get("Memo")
                    .and_then(Json::as_array)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|byte| byte.as_u64().and_then(|byte| u8::try_from(byte).ok()))
                    .collect();
                Some(format!(
                    "Transfer\n\nnonce: {}\nfrom: {} (route {})\napi key: {}\nto: {} (route {})\nasset: {}\namount: {}\nfee: {}\nchainId: {}\nmemo: {}\nOnly sign this message for a trusted client!",
                    hex16(envelope.nonce),
                    hex16(envelope.account.get()),
                    hex16(get("FromRouteType")),
                    hex16(u64::from(envelope.api_key.get())),
                    hex16(to),
                    hex16(get("ToRouteType")),
                    hex16(get("AssetIndex")),
                    hex16(get("Amount")),
                    hex16(get("USDCFee")),
                    hex16(u64::from(self.network.chain_id())),
                    hex::encode(&memo),
                ))
            }
            _ => None,
        }
    }

    /// Moves the slot's nonce on, unless the sequencer drops this one; a registration puts its
    /// key at the slot.
    fn take(&self, tx_type: u8, info: Json, hash: String) -> Result<(), (i64, String)> {
        let account = info.get("AccountIndex").or_else(|| info.get("FromAccountIndex")).and_then(Json::as_u64);
        let (account, slot) =
            (account.unwrap_or(0), small(info.get("ApiKeyIndex").and_then(Json::as_u64).unwrap_or(0)));
        let nonce = info.get("Nonce").and_then(Json::as_u64).unwrap_or(0);
        let dropped = {
            let mut drops = self.drops.borrow_mut();
            let dropped = *drops > 0;
            *drops = drops.saturating_sub(1);
            dropped
        };
        if !dropped {
            let mut keys = self.keys.borrow_mut();
            match TxType::from_code(tx_type) {
                Some(TxType::ChangePubKey) => {
                    let bytes = base64::decode(info.get("PubKey").and_then(Json::as_str).unwrap_or_default());
                    let public = bytes.ok().and_then(|bytes| PublicKey::from_bytes(bytes.try_into().ok()?).ok());
                    let public = public.ok_or((21108, "invalid public key".to_owned()))?;
                    keys.insert((account, slot), Key { public, next: nonce + 1 });
                }
                _ => {
                    if let Some(key) = keys.get_mut(&(account, slot)) {
                        key.next = nonce + 1;
                    }
                }
            }
        }
        self.taken.borrow_mut().push(Taken { tx_type, info, hash });
        Ok(())
    }
}

impl Http for Venue {
    fn send(&self, request: Request) -> impl Future<Output = Answer> {
        let answer = self.respond(&request);
        self.sent.borrow_mut().push(request);
        ready(answer)
    }
}

/// A transaction as posted, hashed the venue's way: every number between `ApiKeyIndex` and
/// `ExpiredAt` in order, the amounts lighter-go splits into their low and high 32 bits, a public
/// key as its five limbs, and nothing else (the memo and the owner's signature are not hashed).
struct Posted {
    kind: TxType,
    fields: Vec<Fp>,
    attributes: Attributes,
}

impl Posted {
    fn read(kind: TxType, info: &Json) -> Posted {
        let entries = info.as_object().unwrap_or_default();
        let from = entries.iter().position(|(key, _)| key == "ApiKeyIndex").map_or(0, |at| at + 1);
        let until = entries.iter().position(|(key, _)| key == "ExpiredAt").unwrap_or(entries.len());
        let mut fields = Vec::new();
        for (key, value) in &entries[from..until] {
            match (key.as_str(), value) {
                ("L1Sig" | "Memo", _) => {}
                ("PubKey", value) => {
                    let bytes: [u8; 40] = base64::decode(value.as_str().unwrap_or_default())
                        .ok()
                        .and_then(|bytes| bytes.try_into().ok())
                        .unwrap_or([0; 40]);
                    let key = PublicKey::from_bytes(bytes).unwrap_or_else(|error| panic!("{error}"));
                    fields.extend(key.limbs().0);
                }
                (key, value) => {
                    let number = value.as_u64().unwrap_or_else(|| panic!("the venue cannot hash {key} of {kind:?}"));
                    match split(kind, key) {
                        true => fields.extend([Fp::new(number & 0xffff_ffff), Fp::new(number >> 32)]),
                        false => fields.push(Fp::new(number)),
                    }
                }
            }
        }
        Posted { kind, fields, attributes: attributes(info.get("L2TxAttributes")) }
    }
}

impl Tx for Posted {
    fn tx_type(&self) -> TxType {
        self.kind
    }

    fn hash_fields(&self) -> Vec<Fp> {
        self.fields.clone()
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        Vec::new()
    }

    fn attributes(&self) -> Attributes {
        self.attributes
    }
}

/// The amounts lighter-go hashes as two 32-bit halves.
fn split(kind: TxType, key: &str) -> bool {
    matches!(
        (kind, key),
        (TxType::Transfer, "Amount" | "USDCFee") | (TxType::Withdraw, "Amount") | (TxType::UpdateMargin, "USDCAmount")
    )
}

/// `L2TxAttributes` back into the builder that wrote them.
fn attributes(posted: Option<&Json>) -> Attributes {
    let get = |key: &str| posted.and_then(|json| json.get(key)).and_then(Json::as_u64);
    let mut attributes = Attributes::NONE;
    if let Some(account) = get("1") {
        let fee = |key: &str| u32::try_from(get(key).unwrap_or(0)).unwrap_or(u32::MAX);
        attributes = attributes.integrator(AccountIndex::new(account).unwrap(), fee("2"), fee("3"));
    }
    if get("4").is_some() {
        attributes = attributes.skip_nonce();
    }
    if let Some(market) = get("5") {
        attributes = attributes.cancel_market(MarketId::new(market).unwrap());
    }
    if let (Some(behavior), Some(scope)) = (get("6"), get("7")) {
        let behavior = [SelfTrade::ExpireMaker, SelfTrade::ExpireTaker, SelfTrade::ExpireBoth, SelfTrade::Reduce]
            [usize::try_from(behavior).unwrap()];
        let scope = [SelfTradeScope::Account, SelfTradeScope::Owner][usize::try_from(scope).unwrap()];
        attributes = attributes.self_trade(behavior, scope);
    }
    if let Some(version) = get("8") {
        attributes = attributes.order_version(version);
    }
    attributes
}

/// `a=1&b=2`, percent-decoded.
fn fields(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => {
                out.push(u8::from_str_radix(&text[at + 1..at + 3], 16).unwrap());
                at += 3;
            }
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}

fn number(value: Option<&str>) -> u64 {
    value.and_then(|value| value.parse().ok()).unwrap_or_else(|| panic!("not a number: {value:?}"))
}

fn small(value: u64) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

fn ok(text: String) -> Answer {
    Ok(Response { status: 200, headers: Vec::new(), body: text.into_bytes() })
}

fn refused(code: i64, message: &str) -> Answer {
    ok(format!(r#"{{"code":{code},"message":"{message}"}}"#))
}
