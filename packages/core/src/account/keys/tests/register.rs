//! Key registrations, held to lighter-go and go-ethereum (`vectors/lighter_go.json`): the hash,
//! `tx_info`, the text the owner signs, the owner's signature and the `tx_info` sent.

use super::*;
use crate::account::keys::LocalWallet;
use crate::test_support::{replay, vector, vector_envelope, vector_key, vector_wallet_key};
use crate::utils::sign::tx::sign_drawing;

#[test]
fn registrations_sign_and_carry_the_owner_s_signature_as_lighter_go_does() {
    let owner = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    for name in ["register-key-a", "register-key-b-on-sub", "robinhood-register"] {
        let case = vector(name);
        let (chain_id, envelope) = vector_envelope(&case);
        let tx = ChangePubKey::new(PublicKey::from_hex(case["fields"]["PubKey"].as_str().unwrap()).unwrap());
        let signed = sign_drawing(&tx, chain_id, &envelope, &vector_key(&case), replay(&case)).unwrap();
        let out = &case["out"];
        assert_eq!(signed.hash().to_hex(), out["tx_hash"].as_str().unwrap(), "{name}: hash");
        assert_eq!(signed.tx_info(), out["tx_info"].as_str().unwrap(), "{name}: tx_info");
        assert_eq!(signed.l1_message(), out["message_to_sign"].as_str(), "{name}: message");
        let l1 = owner.sign_text(signed.l1_message().unwrap()).unwrap();
        assert_eq!(l1.to_hex(), out["l1_sig"].as_str().unwrap(), "{name}: the owner's signature");
        assert_eq!(signed.tx_info_with_l1(&l1.to_hex()).as_deref(), out["signed_tx_info"].as_str(), "{name}");
    }
}
