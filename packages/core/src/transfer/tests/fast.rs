//! A fast withdrawal: the pool and its limits read, the transfer it signs held to lighter-go's
//! `fast-withdraw-memo` case, and the whole flow against a wire that answers from a queue, the
//! post built and never sent anywhere.

use super::*;

const INFO: &str = r#"{"code":200,"to_account_index":281474976710646,"withdraw_limit":"250000.000000","max_withdrawal_amount":"100000.000000"}"#;

#[test]
fn the_pool_and_its_limits_read_from_the_account_s_own_answer() {
    let transport = crate::test_support::Recorded::answering(INFO);
    let info = crate::test_support::block_on(fast_withdraw_info(&transport, AccountIndex::new(10).unwrap())).unwrap();
    assert_eq!(info.pool, AccountIndex::new(281_474_976_710_646).unwrap());
    assert_eq!((info.withdraw_limit, info.max_withdrawal_amount), (Dec::from_int(250_000), Dec::from_int(100_000)));
    let call = transport.only();
    assert_eq!((call.url_path().as_str(), call.auth), ("/api/v1/fastwithdraw/info", true));
    assert_eq!(call.fields, [("account_index", "10".to_owned())]);
    assert_eq!(
        (minimum_fast_withdrawal(Network::Lighter), minimum_fast_withdrawal(Network::Robinhood)),
        (Dec::from_int(4), Dec::ONE)
    );
}

#[test]
fn the_transfer_goes_to_the_pool_with_the_address_in_its_memo() {
    let to: Address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".parse().unwrap();
    let pool = AccountIndex::new(281_474_976_710_646).unwrap();
    let transfer = fast_withdrawal(Network::Lighter, pool, to, Dec::from_int(50), 3_000_000).unwrap();
    assert_eq!((transfer.to, transfer.amount, transfer.fee), (pool, 50_000_000, 3_000_000));
    assert_eq!(transfer.memo.to_hex(), "f39fd6e51aad88f6f4ce6ab8827279cfffb92266000000000000000000000000");
    let call = fast_withdraw_call("{}".to_owned(), to);
    assert_eq!((call.url_path().as_str(), call.auth), ("/api/v1/fastwithdraw", true));
    assert_eq!(
        call.fields,
        [("tx_info", "{}".to_owned()), ("to_address", "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_owned())]
    );
    assert!(fast_withdrawal(Network::Lighter, pool, to, Dec::ZERO, 0).is_err());
}

#[cfg(feature = "sign")]
mod flow {
    use std::future::{Future, ready};

    use super::*;
    use crate::account::keys::{L1Signature, LocalWallet, WalletError};
    use crate::client::{Request, Response, TransportError};
    use crate::test_support::{Wire, block_on, ok, replay, vector, vector_envelope, vector_key, vector_wallet_key};
    use crate::utils::ApiKeyIndex;
    use crate::utils::sign::tx::sign_drawing;

    fn clock() -> u64 {
        1_790_000_000_000
    }

    fn client(answers: impl IntoIterator<Item = Result<Response, TransportError>>) -> Client<Wire, ApiKey> {
        let case = vector("fast-withdraw-memo");
        let key = ApiKey::new(AccountIndex::new(100_001).unwrap(), ApiKeyIndex::new(4).unwrap(), vector_key(&case));
        Client::new(Wire::answering(answers), Network::Lighter, clock, key)
    }

    fn form(request: &Request) -> Vec<(String, String)> {
        let text = String::from_utf8_lossy(&request.body).into_owned();
        text.split('&')
            .filter_map(|pair| pair.split_once('='))
            .map(|(key, value)| {
                let raw = value.replace('+', " ");
                let bytes = raw.as_bytes();
                let mut out = Vec::new();
                let mut at = 0;
                while at < bytes.len() {
                    if bytes[at] == b'%' {
                        out.push(u8::from_str_radix(std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap(), 16).unwrap());
                        at += 3;
                    } else {
                        out.push(bytes[at]);
                        at += 1;
                    }
                }
                (key.to_owned(), String::from_utf8(out).unwrap())
            })
            .collect()
    }

    #[test]
    fn the_signed_transfer_is_lighter_go_s() {
        let case = vector("fast-withdraw-memo");
        let (chain_id, envelope) = vector_envelope(&case);
        let to: Address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".parse().unwrap();
        let pool = AccountIndex::new(case["fields"]["ToAccountIndex"].as_u64().unwrap()).unwrap();
        let transfer = fast_withdrawal(Network::Lighter, pool, to, Dec::from_int(50), 3_000_000).unwrap();
        let signed = sign_drawing(&transfer, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
        assert_eq!(signed.tx_info(), case["out"]["tx_info"].as_str().unwrap());
        assert_eq!(signed.l1_message(), case["out"]["message_to_sign"].as_str());
    }

    #[test]
    fn a_fast_withdrawal_is_posted_once_with_the_owner_s_signature() {
        let client = client([
            ok(INFO),
            ok(r#"{"code":200,"transfer_fee_usdc":3000000}"#),
            ok(r#"{"code":200,"nonce":47}"#),
            ok(r#"{"code":200,"message":"ok","tx_hash":"ff"}"#),
        ]);
        let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
        let to = owner.address();
        let done = block_on(fast_withdraw(&client, &owner, to, Dec::from_int(50))).unwrap();
        assert_eq!(done.tx_hash.as_deref(), Some("ff"));
        let sent = client.http().sent();
        let paths: Vec<&str> = sent.iter().map(|request| request.url.split('?').next().unwrap()).collect();
        assert_eq!(
            paths,
            [
                "https://mainnet.zklighter.elliot.ai/api/v1/fastwithdraw/info",
                "https://mainnet.zklighter.elliot.ai/api/v1/transferFeeInfo",
                "https://mainnet.zklighter.elliot.ai/api/v1/nextNonce",
                "https://mainnet.zklighter.elliot.ai/api/v1/fastwithdraw",
            ]
        );
        assert!(sent[3].is_signed(), "an offline build refuses this post");
        assert!(sent[3].header("authorization").is_some());
        let fields = form(&sent[3]);
        assert_eq!(fields[1], ("to_address".to_owned(), "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_owned()));
        let info: serde_json::Value = serde_json::from_str(&fields[0].1).unwrap();
        let case = vector("fast-withdraw-memo");
        assert_eq!(info["L1Sig"], case["out"]["l1_sig"], "the owner signed lighter-go's text, at nonce 47");
        assert_eq!(
            (info["USDCFee"].as_u64(), info["ToAccountIndex"].as_u64()),
            (Some(3_000_000), Some(281_474_976_710_646))
        );
    }

    /// Claims one address and signs with another key.
    struct Impostor(LocalWallet);

    impl Wallet for Impostor {
        fn address(&self) -> Address {
            "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".parse().unwrap()
        }

        fn sign_message(&self, message: &str) -> impl Future<Output = Result<L1Signature, WalletError>> {
            ready(self.0.sign_text(message))
        }
    }

    #[test]
    fn a_signature_from_another_wallet_posts_nothing_and_the_nonce_is_asked_again() {
        let client = client([
            ok(INFO),
            ok(r#"{"code":200,"transfer_fee_usdc":3000000}"#),
            ok(r#"{"code":200,"nonce":47}"#),
            ok(r#"{"code":200,"nonce":47}"#),
        ]);
        let impostor = Impostor(LocalWallet::from_hex(&vector_wallet_key()).unwrap());
        let refused = block_on(fast_withdraw(&client, &impostor, impostor.address(), Dec::from_int(50)));
        assert!(matches!(refused, Err(SendError::Wallet(WalletError::WrongSigner { .. }))), "{refused:?}");
        assert_eq!(block_on(client.next_nonce()), Ok(47), "the nonce is asked again, not counted on");
        let posted = client.http().sent().iter().filter(|request| request.url.ends_with("/fastwithdraw")).count();
        assert_eq!(posted, 0);
    }
}
