//! A client with a key against a wire that answers from a queue: the nonce asked once and
//! counted, given back or forgotten by what each answer says, the auth token minted once and
//! kept, and the owner's signature checked before anything goes out.

use std::future::{Future, ready};

use serde_json::Value;

use super::*;
use crate::account::keys::{LocalWallet, recover};
use crate::test_support::{Wire, block_on, ok, vector, vector_key, vector_wallet_key};
use crate::utils::Json;
use crate::utils::sign::Fp;
use crate::utils::sign::tx::{TxType, number};
use crate::utils::time::HOUR_MS;
use crate::utils::{AccountIndex, Address, ApiKeyIndex};

const NOW: u64 = 1_790_000_000_000;

fn clock() -> u64 {
    NOW
}

/// A cancel of order `index` on market 1: the lightest transaction there is.
struct Cancel(u64);

impl Tx for Cancel {
    fn tx_type(&self) -> TxType {
        TxType::CancelOrder
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![Fp::new(1), Fp::new(self.0)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("MarketIndex", number(1)), ("Index", number(self.0))]
    }
}

fn key(slot: u8) -> ApiKey {
    let secret = vector_key(&vector("register-key-a"));
    ApiKey::new(AccountIndex::new(100_001).unwrap(), ApiKeyIndex::new(slot).unwrap(), secret)
}

fn client(answers: impl IntoIterator<Item = Result<Response, TransportError>>) -> Client<Wire, ApiKey> {
    Client::new(Wire::answering(answers), Network::Lighter, clock, key(4))
}

fn refused(code: i64) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"code":{code},"message":"no"}}"#))
}

fn sent_tx(hash: &str) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"code":200,"tx_hash":"{hash}"}}"#))
}

/// The form a request posted, decoded.
fn form_fields(request: &Request) -> Vec<(String, String)> {
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
    String::from_utf8_lossy(&request.body)
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

/// The `tx_info` a `sendTx` carried.
fn tx_info(request: &Request) -> Value {
    let info = form_fields(request).into_iter().find(|(key, _)| key == "tx_info").unwrap().1;
    serde_json::from_str(&info).unwrap()
}

fn paths(client: &Client<Wire, ApiKey>) -> Vec<String> {
    client.http().sent().iter().map(|request| request.url.replace("https://mainnet.zklighter.elliot.ai", "")).collect()
}

#[test]
fn the_nonce_is_asked_once_and_counted_from_there() {
    let client = client([ok(r#"{"code":200,"nonce":45}"#), sent_tx("aa"), sent_tx("bb")]);
    let first = block_on(client.send(&Cancel(7))).unwrap();
    let second = block_on(client.send(&Cancel(8))).unwrap();
    assert_eq!((first.tx_hash.as_str(), second.tx_hash.as_str()), ("aa", "bb"));
    assert_eq!(
        paths(&client),
        ["/api/v1/nextNonce?account_index=100001&api_key_index=4", "/api/v1/sendTx", "/api/v1/sendTx"]
    );
    let sent = client.http().sent();
    let info = tx_info(&sent[1]);
    assert_eq!((info["Nonce"].as_u64(), info["Index"].as_u64()), (Some(45), Some(7)));
    assert_eq!(info["ExpiredAt"].as_u64(), Some(NOW + 599_000), "ten minutes less a second");
    assert_eq!((info["AccountIndex"].as_u64(), info["ApiKeyIndex"].as_u64()), (Some(100_001), Some(4)));
    assert_eq!(form_fields(&sent[1])[0], ("tx_type".to_owned(), "15".to_owned()));
    assert_eq!(tx_info(&sent[2])["Nonce"].as_u64(), Some(46));
}

/// A refusal with a code was turned away before it counted: the same nonce signs the next one.
/// One that may have landed sends the client back to the venue for the count; a refused nonce
/// does too, and is signed and sent once more at that count.
#[test]
fn each_answer_settles_the_nonce_as_it_says() {
    let client = client([
        ok(r#"{"code":200,"nonce":10}"#),
        refused(21739),
        sent_tx("aa"),
        Err(TransportError::Unreachable("reset".to_owned())),
        ok(r#"{"code":200,"nonce":13}"#),
        refused(21104),
        ok(r#"{"code":200,"nonce":20}"#),
        sent_tx("bb"),
    ]);
    let margin = block_on(client.send(&Cancel(1))).unwrap_err();
    assert_eq!(margin, SendError::Api(ApiError::Refused { code: 21739, message: "no".to_owned() }));
    block_on(client.send(&Cancel(2))).unwrap();
    let lost = block_on(client.send(&Cancel(3))).unwrap_err();
    assert!(matches!(&lost, SendError::Api(error) if error.may_have_landed()), "{lost}");
    let again = block_on(client.send(&Cancel(4))).unwrap();
    assert_eq!(again.tx_hash, "bb", "sent once more at the venue's count");
    let nonces: Vec<Option<u64>> = client
        .http()
        .sent()
        .iter()
        .filter(|request| request.url.ends_with("sendTx"))
        .map(|request| tx_info(request)["Nonce"].as_u64())
        .collect();
    assert_eq!(nonces, [Some(10), Some(10), Some(11), Some(13), Some(20)]);
    assert_eq!(paths(&client).iter().filter(|path| path.contains("nextNonce")).count(), 3);
    assert_eq!(block_on(client.next_nonce()), Ok(21));
}

#[test]
fn a_nonce_refused_twice_comes_back() {
    let client =
        client([ok(r#"{"code":200,"nonce":1}"#), refused(21104), ok(r#"{"code":200,"nonce":2}"#), refused(21105)]);
    let error = block_on(client.send(&Cancel(1))).unwrap_err();
    assert!(matches!(&error, SendError::Api(error) if error.code() == Some(21105)), "{error}");
    assert_eq!(paths(&client).len(), 4);
}

#[test]
fn a_rate_limited_send_keeps_its_nonce() {
    let client = client([ok(r#"{"code":200,"nonce":3}"#), crate::test_support::status(429, ""), sent_tx("aa")]);
    let error = block_on(client.send(&Cancel(1))).unwrap_err();
    assert!(matches!(&error, SendError::Api(error) if error.is_rate_limited()), "{error}");
    assert_eq!(block_on(client.next_nonce()), Ok(3));
}

/// A batch takes consecutive nonces and posts each signed `tx_info` in order.
#[test]
fn a_batch_signs_at_consecutive_nonces() {
    let client = client([ok(r#"{"code":200,"nonce":30}"#), ok(r#"{"code":200,"tx_hash":["aa","bb","cc"]}"#)]);
    let txs: [&dyn Tx; 3] = [&Cancel(1), &Cancel(2), &Cancel(3)];
    let sent = block_on(client.send_batch(&txs)).unwrap();
    assert_eq!(sent.tx_hashes, ["aa", "bb", "cc"]);
    let request = &client.http().sent()[1];
    let fields = form_fields(request);
    assert_eq!(fields[0], ("tx_types".to_owned(), "[15,15,15]".to_owned()));
    let infos: Vec<String> = serde_json::from_str(&fields[1].1).unwrap();
    let nonces: Vec<u64> =
        infos.iter().map(|info| serde_json::from_str::<Value>(info).unwrap()["Nonce"].as_u64().unwrap()).collect();
    assert_eq!(nonces, [30, 31, 32]);
    assert_eq!(block_on(client.next_nonce()), Ok(33));
    let empty: [&dyn Tx; 0] = [];
    assert!(matches!(block_on(client.send_batch(&empty)), Err(SendError::Sign(_))));
}

/// The token is minted once for seven hours and carried by every private read until ten
/// minutes before it lapses.
#[test]
fn the_auth_token_is_minted_once_and_kept() {
    let client = Client::new(Wire::always("{}"), Network::Lighter, clock, key(4));
    let token = client.auth_token().unwrap();
    assert!(token.starts_with(&format!("{}:100001:4:", (NOW + 7 * HOUR_MS) / 1_000)), "{token}");
    block_on(client.call(Call::get("accountActiveOrders").authed())).unwrap();
    block_on(client.call(Call::get("accountActiveOrders").authed())).unwrap();
    let sent = client.http().sent();
    assert!(sent.iter().all(|request| request.header("Authorization") == Some(token.as_str())));
    let shown = format!("{client:?}");
    assert!(!shown.contains(&token) && shown.contains("Lighter"), "{shown}");
}

/// A key registers with the owner's signature, which must recover to the owner.
#[test]
fn a_registration_carries_the_owner_s_signature() {
    let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    let client = client([ok(r#"{"code":200,"nonce":0}"#), sent_tx("aa")]);
    block_on(client.register(&owner)).unwrap();
    let info = tx_info(&client.http().sent()[1]);
    let message = format!(
        "Register Lighter Account\n\npubkey: 0x{}\nnonce: 0x0000000000000000\naccount index: 0x00000000000186a1\napi key index: 0x0000000000000004\nOnly sign this message for a trusted client!",
        client.key().public_key().to_hex().trim_start_matches("0x")
    );
    let signature = L1Signature::from_hex(info["L1Sig"].as_str().unwrap()).unwrap();
    assert_eq!(recover(&message, &signature), Ok(owner.address()));
    assert_eq!(info["PubKey"].as_str().map(str::len), Some(56), "forty bytes of base64");
}

/// Claims one address, signs as another.
struct Impostor {
    claims: Address,
    signs: LocalWallet,
}

impl Wallet for Impostor {
    fn address(&self) -> Address {
        self.claims
    }

    fn sign_message(&self, message: &str) -> impl Future<Output = Result<L1Signature, WalletError>> {
        ready(self.signs.sign_text(message))
    }
}

#[test]
fn an_owner_signing_as_someone_else_sends_nothing() {
    let impostor = Impostor {
        claims: "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf".parse().unwrap(),
        signs: LocalWallet::from_hex(&vector_wallet_key()).unwrap(),
    };
    let client = client([ok(r#"{"code":200,"nonce":5}"#), sent_tx("aa")]);
    let error = block_on(client.register(&impostor)).unwrap_err();
    assert!(matches!(error, SendError::Wallet(WalletError::WrongSigner { .. })), "{error}");
    assert_eq!(paths(&client).len(), 1, "only the nonce was asked");
    assert_eq!(block_on(client.next_nonce()), Ok(5), "and it was not used");
}

#[test]
fn the_venue_s_own_slots_are_never_registered_over() {
    let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    for slot in [0, 3] {
        let client = Client::new(Wire::answering([]), Network::Lighter, clock, key(slot));
        let error = block_on(client.register(&owner)).unwrap_err();
        assert!(matches!(error, SendError::Sign(SignTxError::Bad(_))), "{error}");
        assert!(client.http().sent().is_empty());
    }
}
