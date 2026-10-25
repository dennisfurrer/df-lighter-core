//! Expected values are node's `BigInt` over the same digits.

use super::*;

fn big(text: &str) -> U256 {
    text.parse().unwrap()
}

#[test]
fn arithmetic_is_checked_at_the_edges() {
    assert_eq!(U256::MAX.checked_add(U256::ONE), None);
    assert_eq!(U256::ZERO.checked_sub(U256::ONE), None);
    assert_eq!(U256::MAX.saturating_add(U256::ONE), U256::MAX);
    let two_128 = big("340282366920938463463374607431768211456");
    assert_eq!(two_128.checked_mul(two_128), None, "2^256 does not fit");
    let two_200 = two_128.checked_mul(U256::from(1u128 << 72)).unwrap();
    assert_eq!(two_200.to_string(), "1606938044258990275541962092341162602522202993782792835301376");
    assert_eq!(
        U256::ten_to(77).unwrap().to_quantity(),
        "0xdd15fe86affad91249ef0eb713f39ebeaa987b6e6fd2a0000000000000000000"
    );
    assert_eq!(U256::ten_to(78), None);
    assert_eq!(U256::from(u64::MAX).checked_add(U256::ONE).unwrap().to_quantity(), "0x10000000000000000");
    assert_eq!(big("0x10000000000000000").checked_sub(U256::ONE), Some(U256::from(u64::MAX)));
    assert_eq!(U256::MAX.saturating_mul(U256::from(2u8)), U256::MAX);
}

#[test]
fn bytes_and_narrow_integers_convert_both_ways() {
    let mut word = [0u8; 32];
    word[0] = 0x80;
    word[31] = 0x01;
    let value = U256::from_be_bytes(word);
    assert_eq!(value.to_be_bytes(), word);
    assert_eq!(U256::from_be_slice(&[0x01, 0x00]), Some(U256::from(256u16)));
    assert_eq!(U256::from_be_slice(&[]), Some(U256::ZERO));
    assert_eq!(U256::from_be_slice(&[0u8; 33]), None, "33 bytes is more than a word");
    assert_eq!(U256::from(u64::MAX).to_u64(), Some(u64::MAX));
    assert_eq!(U256::from(u128::MAX).to_u64(), None);
    assert_eq!(U256::from(u128::MAX).to_u128(), Some(u128::MAX));
    assert_eq!(big("340282366920938463463374607431768211456").to_u128(), None);
    assert!(U256::ONE < U256::MAX && U256::from(1u128 << 64) > U256::from(u64::MAX));
    assert_eq!(U256::ZERO.bits(), 0);
    assert_eq!(U256::from(255u8).bits(), 8);
    assert_eq!(U256::MAX.bits(), 256);
}
