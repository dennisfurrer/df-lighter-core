//! Addresses read in any case and written lowercase, and nothing else read as one.

use super::*;

#[test]
fn any_case_reads_to_the_same_address_and_writes_back_lowercase() {
    let lower = "0xbaff87940cb148c5e2529c0a5944b0a3386fb015";
    let address: Address = lower.parse().unwrap();
    let json = serde_json::to_string(&address).unwrap();
    assert_eq!(json, format!("\"{lower}\""));
    assert_eq!(serde_json::from_str::<Address>(&json).unwrap(), address);
    let upper: Address = serde_json::from_str("\"0xBAFF87940CB148C5E2529C0A5944B0A3386FB015\"").unwrap();
    assert_eq!((upper, upper.to_string()), (address, lower.to_owned()));
    let mixed: Address = "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".parse().unwrap();
    assert_eq!(mixed.to_hex(), "0xaf88d065e77c8cc2239327c5edb3a432268e5831");
    assert_eq!(" 0X14791697260E4c9A71f18484C9f997B308e59325 ".parse::<Address>().unwrap().bytes()[0], 0x14);
}

#[test]
fn anything_else_is_refused_rather_than_zeroed() {
    for bad in [
        "0xdeadbeef",
        "baff87940cb148c5e2529c0a5944b0a3386fb015",
        "0xzzff87940cb148c5e2529c0a5944b0a3386fb015",
        "0xbaff87940cb148c5e2529c0a5944b0a3386fb0155",
        "",
        "0x",
    ] {
        assert!(bad.parse::<Address>().is_err(), "{bad} should be refused");
    }
    for said in ["\"0x00\"", "\"nonsense\"", "\"\"", "42"] {
        assert!(serde_json::from_str::<Address>(said).is_err(), "{said}");
    }
}
