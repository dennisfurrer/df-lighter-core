//! Opening a sub-account, held to lighter-go (`vectors/lighter_go.json`): the hash and `tx_info`;
//! and a sub-account's key refused before anything is signed or sent.

use super::*;
use crate::client::{Client, Network};
use crate::test_support::{Wire, block_on, replay, vector, vector_envelope, vector_key};
use crate::utils::sign::tx::sign_drawing;
use crate::utils::{AccountIndex, ApiKeyIndex};

#[test]
fn a_sub_account_is_opened_as_lighter_go_signs_it() {
    let case = vector("create-sub-account");
    let (chain_id, envelope) = vector_envelope(&case);
    let signed = sign_drawing(&CreateSubAccount, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
    let out = &case["out"];
    assert_eq!(signed.tx_type().code(), 9);
    assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap());
    assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap());
    assert_eq!(signed.l1_message(), None, "the API key alone opens one");
}

#[test]
fn only_a_master_opens_a_sub_account_and_nothing_goes_out_otherwise() {
    let secret = vector_key(&vector("create-sub-account"));
    let sub = AccountIndex::new(281_474_976_710_600).unwrap();
    let key = ApiKey::new(sub, ApiKeyIndex::new(5).unwrap(), secret);
    let client = Client::new(Wire::answering([]), Network::Lighter, || 1_790_000_000_000, key);
    match block_on(create_sub_account(&client)) {
        Err(SendError::Sign(SignTxError::Bad(bad))) => assert_eq!(bad.field, "AccountIndex"),
        other => panic!("refused before signing, not {other:?}"),
    }
    assert!(client.http().sent().is_empty(), "nothing asked, nothing sent");
}
