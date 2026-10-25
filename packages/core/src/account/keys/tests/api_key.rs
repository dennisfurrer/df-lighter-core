//! An API key's auth tokens, held to lighter-go's (`vectors/lighter_go.json`), and what it shows
//! of itself.

use super::*;
use crate::test_support::{replay, vector_envelope, vector_key, vectors};

#[test]
fn auth_tokens_are_what_lighter_go_mints() {
    let tokens = vectors()["tokens"].as_array().unwrap().clone();
    assert_eq!(tokens.len(), 3);
    for case in tokens {
        let name = case["name"].as_str().unwrap();
        let (_, envelope) = vector_envelope(&case);
        let key = ApiKey::new(envelope.account, envelope.api_key, vector_key(&case));
        let deadline_s = case["deadline_s"].as_u64().unwrap();
        let token = key.auth_token_drawing(deadline_s, replay(&case)).unwrap();
        assert_eq!(token.as_str(), case["token"].as_str().unwrap(), "{name}");
        assert_eq!(token.deadline_s(), deadline_s);
        let (message, signature) = token.as_str().rsplit_once(':').unwrap();
        assert_eq!(message, format!("{deadline_s}:{}:{}", envelope.account, envelope.api_key), "{name}");
        assert_eq!(signature.len(), 160, "{name}: 80 bytes of hex, no prefix");
    }
}

#[test]
fn a_key_shows_its_slot_and_public_key_and_never_its_secret() {
    let case = &vectors()["tokens"][0];
    let (_, envelope) = vector_envelope(case);
    let secret = vector_key(case);
    let hex = secret.to_hex();
    let key = ApiKey::new(envelope.account, envelope.api_key, secret);
    let shown = format!("{key:?}");
    assert!(shown.contains("100001") && shown.contains(&key.public_key().to_hex()), "{shown}");
    assert!(!shown.contains(hex.trim_start_matches("0x")), "{shown}");
    let token = key.auth_token_drawing(1_790_025_200, replay(case)).unwrap();
    let shown = format!("{token:?}");
    assert!(shown.contains("1790025200") && !shown.contains(token.as_str()), "{shown}");
    assert_eq!(ApiKey::from_hex(key.account(), key.slot(), &hex).unwrap().public_key(), key.public_key());
    let moved = key.at(AccountIndex::new(7).unwrap(), ApiKeyIndex::new(9).unwrap());
    assert_eq!((moved.account().get(), moved.slot().get(), moved.public_key()), (7, 9, key.public_key()));
}

#[cfg(feature = "sign")]
#[test]
fn a_generated_key_signs_tokens_with_fresh_randomness() {
    let key = ApiKey::generate(AccountIndex::new(1).unwrap(), ApiKeyIndex::new(4).unwrap()).unwrap();
    let first = key.auth_token(1_790_000_000).unwrap();
    let second = key.auth_token(1_790_000_000).unwrap();
    assert_ne!(first.as_str(), second.as_str(), "a fresh nonce each time");
    assert!(first.as_str().starts_with("1790000000:1:4:"));
    const { assert!(TOKEN_S < MAX_TOKEN_S) };
}
