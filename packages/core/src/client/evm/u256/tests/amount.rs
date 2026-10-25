//! Expected values are node's `BigInt` over the same digits.

use super::*;

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn big(text: &str) -> U256 {
    text.parse().unwrap()
}

#[test]
fn wei_converts_to_and_from_an_exact_decimal_at_a_named_number_of_places() {
    // ETH has eighteen; USDC on Ethereum and USDG on Robinhood Chain six.
    assert_eq!(U256::from_dec(dec("1.5"), 18), Ok(U256::from(1_500_000_000_000_000_000u64)));
    assert_eq!(U256::from(1_500_000_000_000_000_000u64).to_dec(18), Ok(dec("1.5")));
    assert_eq!(U256::ONE.to_dec(18), Ok(dec("0.000000000000000001")));
    assert_eq!(U256::from_dec(dec("12.5"), 6), Ok(U256::from(12_500_000u64)));
    assert_eq!(U256::from_dec(Dec::ZERO, 18), Ok(U256::ZERO));
    // A balance as wide as a large holder's.
    assert_eq!(big("947499191167351653726310821").to_dec(18), Ok(dec("947499191.167351653726310821")));
    // Past eighteen places, as a token with extra EVM decimals counts: exact or refused.
    assert_eq!(U256::ten_to(26).unwrap().to_dec(26), Ok(Dec::ONE));
    assert_eq!(U256::from_dec(Dec::ONE, 26), Ok(U256::ten_to(26).unwrap()));
    assert_eq!(U256::ONE.to_dec(26), Err(BadAmount::Unkept { amount: U256::ONE, decimals: 26 }));
    assert_eq!(U256::from_dec(Dec::ONE, 77), U256::ten_to(77).ok_or(BadAmount::TooLarge));
    assert_eq!(U256::from_dec(dec("10"), 77), Err(BadAmount::TooLarge));
    assert_eq!(U256::MAX.to_dec(0), Err(BadAmount::TooLarge), "past what a Dec counts");
    assert_eq!(U256::from_dec(dec("0.0000001"), 6), Err(BadAmount::TooFine { value: dec("0.0000001"), decimals: 6 }));
    assert_eq!(U256::from_dec(dec("-1"), 18), Err(BadAmount::NotPositive(dec("-1"))));
}
