//! Transfers held to lighter-go and go-ethereum (`vectors/lighter_go.json`): the hash, `tx_info`,
//! the text the owner signs, the owner's signature and the `tx_info` sent; amounts made exactly;
//! and a transfer sent with the fee the venue asks.

use super::*;
use crate::utils::units::BadUnits;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn amounts_are_made_exactly_in_the_asset_s_venue_decimals() {
    let sub = AccountIndex::new(281_474_976_710_600).unwrap();
    let transfer = Transfer::collateral(Network::Robinhood, sub, dec("25.5")).unwrap();
    assert_eq!((transfer.asset, transfer.amount, transfer.fee), (AssetId::COLLATERAL, 25_500_000, 0));
    assert_eq!((transfer.from_route, transfer.to_route, transfer.memo), (Route::Perps, Route::Perps, Memo::NONE));
    let eth = Transfer::new(sub, AssetId::ETH, 8, dec("0.12345678")).unwrap().routes(Route::Spot, Route::Spot);
    assert_eq!((eth.amount, eth.from_route, eth.to_route), (12_345_678, Route::Spot, Route::Spot));
    assert_eq!(
        Transfer::collateral(Network::Lighter, sub, dec("0.0000001")),
        Err(BadUnits::TooFine { value: dec("0.0000001"), places: 6 }),
        "a seventh place is refused, never rounded"
    );
    assert!(matches!(Transfer::collateral(Network::Lighter, sub, Dec::ZERO), Err(BadUnits::NotPositive(_))));
    assert!(matches!(Transfer::new(sub, AssetId::ETH, 18, dec("2")), Err(BadUnits::TooLarge { .. })), "past 2^60");
    let fee = transfer.with_fee(1_000_000).with_memo(Memo::text("rent").unwrap());
    assert_eq!((fee.fee, &fee.memo.bytes()[..5]), (1_000_000, &b"rent\0"[..]));
    assert_eq!(Memo::text(&"x".repeat(33)), None);
    let owner: Address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".parse().unwrap();
    assert_eq!(Memo::address(owner).to_hex(), "f39fd6e51aad88f6f4ce6ab8827279cfffb92266000000000000000000000000");
    assert_eq!(Memo::from_bytes([7; 32]).bytes(), &[7; 32]);
}

#[test]
fn lighter_go_s_checks_refuse_what_it_refuses() {
    let to = AccountIndex::new(1).unwrap();
    let base = Transfer::collateral(Network::Lighter, to, dec("1")).unwrap();
    assert!(Transfer { amount: 0, ..base }.check().is_err());
    assert!(Transfer { amount: MAX_AMOUNT + 1, ..base }.check().is_err());
    assert!(Transfer { fee: MAX_AMOUNT + 1, ..base }.check().is_err());
    assert_eq!(Transfer { amount: MAX_AMOUNT, fee: MAX_AMOUNT, ..base }.check(), Ok(()));
}

#[cfg(feature = "sign")]
mod vectors {
    use serde_json::Value;

    use super::*;
    use crate::account::keys::LocalWallet;
    use crate::test_support::{replay, vector, vector_envelope, vector_key, vector_wallet_key};
    use crate::utils::sign::SignTxError;
    use crate::utils::sign::tx::sign_drawing;

    fn from_fields(fields: &Value) -> Transfer {
        let number = |key: &str| fields[key].as_u64().unwrap();
        let route = |key: &str| if number(key) == 0 { Route::Perps } else { Route::Spot };
        let memo: [u8; 32] = hex::decode(fields["Memo"].as_str().unwrap()).unwrap().try_into().unwrap();
        Transfer {
            to: AccountIndex::new(number("ToAccountIndex")).unwrap(),
            asset: AssetId::new(u16::try_from(number("AssetIndex")).unwrap()).unwrap(),
            from_route: route("FromRouteType"),
            to_route: route("ToRouteType"),
            amount: number("Amount"),
            fee: number("USDCFee"),
            memo: Memo::from_bytes(memo),
        }
    }

    #[test]
    fn transfers_sign_and_carry_the_owner_s_signature_as_lighter_go_does() {
        let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
        let names = [
            "transfer-to-sub-account",
            "transfer-to-another-owner",
            "fast-withdraw-memo",
            "transfer-spot-eth",
            "transfer-perps-to-spot",
            "robinhood-transfer",
        ];
        for name in names {
            let case = vector(name);
            let (chain_id, envelope) = vector_envelope(&case);
            let tx = from_fields(&case["fields"]);
            let signed = sign_drawing(&tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
            let out = &case["out"];
            assert_eq!(u64::from(signed.tx_type().code()), out["tx_type"].as_u64().unwrap(), "{name}: type");
            assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
            assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
            assert_eq!(signed.l1_message(), out["message_to_sign"].as_str(), "{name}: message");
            let l1 = owner.sign_text(signed.l1_message().unwrap()).unwrap();
            assert_eq!(l1.to_hex(), out["l1_sig"].as_str().unwrap(), "{name}: the owner's signature");
            assert_eq!(signed.tx_info_with_l1(&l1.to_hex()).as_deref(), out["signed_tx_info"].as_str(), "{name}");
        }
    }

    #[test]
    fn a_transfer_of_nothing_is_refused_before_it_is_signed() {
        let case = vector("refused-transfer-zero");
        assert!(case["error"].as_str().unwrap().contains("TransferAmount"));
        let (chain_id, envelope) = vector_envelope(&case);
        let tx = from_fields(&case["fields"]);
        let refused = sign_drawing(&tx, chain_id, &envelope, &vector_key(&case), replay(&case));
        assert!(matches!(refused, Err(SignTxError::Bad(BadTx { field: "Amount", .. }))), "{refused:?}");
    }
}

#[cfg(feature = "sign")]
mod sending {
    use super::*;
    use crate::account::keys::LocalWallet;
    use crate::client::Client;
    use crate::test_support::{Wire, block_on, ok, vector, vector_key, vector_wallet_key};
    use crate::utils::ApiKeyIndex;

    fn clock() -> u64 {
        1_790_000_000_000
    }

    fn client(wire: Wire) -> Client<Wire, ApiKey> {
        let key = ApiKey::new(
            AccountIndex::new(100_001).unwrap(),
            ApiKeyIndex::new(4).unwrap(),
            vector_key(&vector("transfer-to-sub-account")),
        );
        Client::new(wire, Network::Lighter, clock, key)
    }

    fn tx_info(body: &str) -> serde_json::Value {
        let info = body.split('&').find_map(|pair| pair.strip_prefix("tx_info=")).unwrap();
        let decoded = info.replace('+', " ");
        let mut bytes = Vec::new();
        let raw = decoded.as_bytes();
        let mut at = 0;
        while at < raw.len() {
            if raw[at] == b'%' {
                bytes.push(u8::from_str_radix(std::str::from_utf8(&raw[at + 1..at + 3]).unwrap(), 16).unwrap());
                at += 3;
            } else {
                bytes.push(raw[at]);
                at += 1;
            }
        }
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn a_transfer_carries_the_fee_the_venue_asks_and_the_owner_signs_one_to_another_owner() {
        let wire = Wire::answering([
            ok(r#"{"code":200,"transfer_fee_usdc":1000000}"#),
            ok(r#"{"code":200,"nonce":46}"#),
            ok(r#"{"code":200,"tx_hash":"aa"}"#),
            ok(r#"{"code":200,"transfer_fee_usdc":0}"#),
            ok(r#"{"code":200,"tx_hash":"bb"}"#),
        ]);
        let client = client(wire);
        let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
        let other =
            Transfer::collateral(Network::Lighter, AccountIndex::new(713_888).unwrap(), Dec::from_int(100)).unwrap();
        assert_eq!(block_on(transfer_to_owner(&client, other, &owner)).unwrap().tx_hash, "aa");
        let sub = AccountIndex::new(281_474_976_710_600).unwrap();
        let own = Transfer::collateral(Network::Lighter, sub, Dec::from_int(25)).unwrap();
        assert_eq!(block_on(transfer(&client, own)).unwrap().tx_hash, "bb");

        let sent = client.http().bodies();
        assert!(sent[0].0.ends_with("/api/v1/transferFeeInfo?account_index=100001&to_account_index=713888"));
        assert!(client.http().sent()[0].header("authorization").is_some(), "the fee is the account's own read");
        let first = tx_info(&sent[2].1);
        assert_eq!((first["USDCFee"].as_u64(), first["Nonce"].as_u64()), (Some(1_000_000), Some(46)));
        let l1 = first["L1Sig"].as_str().unwrap();
        assert_eq!(l1.len(), 132, "the owner's signature rides along");
        let second = tx_info(&sent[4].1);
        assert_eq!(
            (second["USDCFee"].as_u64(), second["Nonce"].as_u64(), second["L1Sig"].as_str()),
            (Some(0), Some(47), Some(""))
        );
        assert_eq!(second["ToAccountIndex"].as_u64(), Some(281_474_976_710_600));
    }
}
