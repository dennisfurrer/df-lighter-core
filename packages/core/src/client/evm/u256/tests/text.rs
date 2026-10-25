//! Expected values are node's `BigInt` over the same digits, and quantities as nodes write them.

use super::*;

const MAX_DECIMAL: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

#[test]
fn quantities_and_decimals_read_and_write_across_the_full_width() {
    for (quantity, value) in [
        ("0x0", U256::ZERO),
        ("0x", U256::ZERO),
        ("0x3e7", U256::from(999u64)),
        ("0X3E7", U256::from(999u64)),
        // A 32-byte word, as eth_call answers decimals().
        ("0x0000000000000000000000000000000000000000000000000000000000000006", U256::from(6u8)),
        (&format!("0x{}", "f".repeat(64)), U256::MAX),
    ] {
        assert_eq!(U256::from_quantity(quantity), Ok(value), "{quantity}");
    }
    // Quantities as eth_getBalance writes them, up to the full width.
    for (quantity, decimal) in [
        ("0x0", "0"),
        ("0x3e7", "999"),
        ("0x30fc0c21318a936a96a51a5", "947499191167351653726310821"),
        (
            "0x8000000000000000000000000000000000000000000000000000000000003039",
            "57896044618658097711785492504343953926634992332820282019728792003956564832313",
        ),
        (&format!("0x{}", "f".repeat(64)), MAX_DECIMAL),
    ] {
        let value = U256::from_decimal(decimal).unwrap();
        assert_eq!((value.to_quantity(), value.to_string()), (quantity.to_owned(), decimal.to_owned()));
        assert_eq!(quantity.parse::<U256>(), Ok(value), "FromStr takes hex with its prefix too");
    }
    let read: U256 = serde_json::from_str("\"0x3e7\"").unwrap();
    assert_eq!((read, serde_json::to_string(&read).unwrap()), (U256::from(999u64), "\"0x3e7\"".to_owned()));
}

#[test]
fn what_is_not_a_quantity_or_a_decimal_or_does_not_fit_is_refused() {
    assert!(matches!(U256::from_quantity(&format!("0x1{}", "0".repeat(64))), Err(BadQuantity::TooLarge(_))));
    for bad in ["12", "0xzz", ""] {
        assert!(matches!(U256::from_quantity(bad), Err(BadQuantity::NotHex(_))), "{bad:?}: a quantity is prefixed hex");
    }
    assert!(matches!(
        U256::from_decimal("115792089237316195423570985008687907853269984665640564039457584007913129639936"),
        Err(BadQuantity::TooLarge(_))
    ));
    for bad in ["", "-1", "1.5", "1e3", " "] {
        assert!(U256::from_decimal(bad).is_err(), "{bad:?}");
    }
    assert!(serde_json::from_str::<U256>("999").is_err(), "a bare number is not a quantity");
}
