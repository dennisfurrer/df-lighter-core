//! Words as thirty-two bytes of hex: a live log topic, and an address in one.

use super::*;

#[test]
fn a_word_is_thirty_two_bytes_of_hex() {
    let text = "0x8c7f585fb295f7eb1e6aeb8fba61b23a4fe60beda405f0045073b185c74412e3";
    let word: B256 = text.parse().unwrap();
    assert_eq!(word.to_string(), text);
    assert_eq!(word.0[0], 0x8c);
    assert_eq!(serde_json::from_str::<B256>(&format!("\"{text}\"")).unwrap(), word);
    assert_eq!(serde_json::to_string(&word).unwrap(), format!("\"{text}\""));
    assert!("0x8c7f".parse::<B256>().is_err());
    let user: B256 = "0x0000000000000000000000006b9e773128f453f5c2c60935ee2de2cbc5390a24".parse().unwrap();
    assert_eq!(user.to_address().unwrap().to_hex(), "0x6b9e773128f453f5c2c60935ee2de2cbc5390a24");
    assert_eq!(B256::from_address(user.to_address().unwrap()), user);
    assert_eq!(word.to_address(), None, "high bytes set: not an address");
    assert_eq!(U256::from(word).to_be_bytes(), word.0);
    assert_eq!(B256::from(U256::from(6u8)).0[31], 6);
}
