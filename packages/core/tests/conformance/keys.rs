//! Who signs: a key registered at a slot (`ChangePubKey`), signed by the new key and by the
//! owner's wallet over "Register Lighter Account", on Lighter, on a sub-account and on Robinhood
//! Chain; both published keys made again from the draws lighter-go's `GenerateAPIKey` took; and
//! the auth tokens `ConstructAuthToken` made for each signer.

use df_lighter_core::account::ChangePubKey;
use df_lighter_core::utils::hex;
use df_lighter_core::utils::sign::{Fp, PublicKey, SecretKey, Signature, hash_to_fp5};

use crate::support::{secret, vectors};
use crate::vector::{Draws, Signer, every, number, text};

const CHANGE_PUB_KEY: [&str; 3] = ["register-key-a", "register-key-b-on-sub", "robinhood-register"];

/// The transaction cases replayed here.
pub const CASES: [&[&str]; 1] = [&CHANGE_PUB_KEY];

/// The auth tokens on file.
pub const TOKENS: [&str; 3] = ["auth-token-7h", "auth-token-robinhood", "auth-token-sub-account"];

/// The keys lighter-go generated, by the name the cases sign with.
pub const GENERATED: [&str; 2] = ["a", "b"];

#[test]
fn every_registration_is_signed_by_the_new_key_and_its_owner_as_lighter_go_signs_it() {
    every("ChangePubKey", &CHANGE_PUB_KEY, &[], |case| {
        let key = PublicKey::from_hex(case.fields().text("PubKey")).unwrap();
        assert_eq!(key, case.signer().key.public_key(), "{}: the key registers itself", case.name());
        Ok(ChangePubKey::new(key))
    });
}

#[test]
fn both_published_keys_are_made_again_from_the_draws_lighter_go_took() {
    let keys = &vectors()["keys"];
    for name in GENERATED {
        let key = &keys[name];
        let draws = Draws::of(&key["draws"]);
        let generated = SecretKey::generate_drawing(draws.draw()).unwrap();
        assert_eq!(draws.left(), 0, "{name}: every draw, no more");
        assert_eq!(generated.to_hex().as_str(), text(key, "private_key"), "{name}: the private key");
        assert_eq!(generated.public_key().to_hex(), text(key, "public_key"), "{name}: the public key");
    }
}

#[test]
fn every_auth_token_is_lighter_go_s_and_its_signature_verifies_by_the_public_key_alone() {
    let tokens = vectors()["tokens"].as_array().unwrap().clone();
    assert_eq!(tokens.iter().map(|token| text(token, "name")).collect::<Vec<_>>(), TOKENS);
    for token in &tokens {
        let name = text(token, "name");
        let signer = Signer::of(&token["signer"]);
        let deadline_s = number(token, "deadline_s");
        let draws = Draws::of(&token["draws"]);
        let made = signer.key.auth_token_drawing(deadline_s, draws.draw()).unwrap();
        assert_eq!(draws.left(), 0, "{name}: every draw, no more");
        assert_eq!(made.as_str(), text(token, "token"), "{name}");
        assert_eq!(made.deadline_s(), deadline_s, "{name}");

        // `{deadline}:{account}:{slot}`, hashed as text and signed: the key's and no other's.
        let (message, signature) = text(token, "token").rsplit_once(':').unwrap();
        let hashed = hash_to_fp5(&Fp::from_le_chunks(message.as_bytes()).unwrap());
        let signature = Signature::from_bytes(hex::decode_array::<80>(signature).unwrap());
        assert!(signer.public.verifies(hashed, &signature), "{name}");
        let other = if signer.public == secret("a").public_key() { "b" } else { "a" };
        assert!(!secret(other).public_key().verifies(hashed, &signature), "{name}: not key {other}'s");
    }
}
