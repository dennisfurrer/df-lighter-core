//! Types read from their text, values encoded and read back, held to Foundry's `cast calldata`
//! (1.x) for the dynamic shapes, and refused where a word does not fit its type.

use super::*;
use crate::client::evm::u256::U256;
use crate::utils::{Address, hex};

fn address(text: &str) -> Address {
    text.parse().unwrap()
}

#[test]
fn types_read_from_their_text_and_write_back_canonically() {
    assert_eq!(kinds("uint48, uint16 assetIndex,address[] ,bytes").unwrap().len(), 4);
    for (text, canonical) in [
        ("uint", "uint256"),
        ("uint8", "uint8"),
        ("address[]", "address[]"),
        ("uint48[][]", "uint48[][]"),
        ("string", "string"),
        ("bool", "bool"),
    ] {
        assert_eq!(Kind::parse(text).unwrap().canonical(), canonical);
    }
    for bad in ["uint7", "uint264", "int8", "bytes32", "(address)", "tuple", "address[3]", ""] {
        assert!(Kind::parse(bad).is_err(), "{bad}");
    }
    assert_eq!(kinds(" ").unwrap(), Vec::new());
}

#[test]
fn a_function_reads_with_or_without_returns_and_names() {
    let function = Function::parse("addressToAccountIndex(address owner) returns (uint48)").unwrap();
    assert_eq!((function.signature().as_str(), function.outputs.len()), ("addressToAccountIndex(address)", 1));
    assert_eq!(function.selector(), [0xab, 0xf6, 0xa0, 0x38]);
    assert_eq!(selector("desertMode()").unwrap(), [0x02, 0xcf, 0xb5, 0x63]);
    for bad in ["", "(uint8)", "f(uint8", "f(uint8) gives (bool)", "f g(uint8)"] {
        assert!(Function::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn arrays_strings_and_bytes_encode_as_cast_does_and_read_back() {
    let batch = Function::parse("depositBatch(uint64[],address[],uint48[])").unwrap();
    let args = [
        Token::Array(vec![Token::uint(25_500_000u64), Token::uint(1_000_000u64)]),
        Token::Array(vec![
            Token::address(address("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")),
            Token::address(address("0x70997970C51812dc3A010C7d01b50e0d17dc79C8")),
        ]),
        Token::Array(vec![Token::uint(100_001u64), Token::uint(281_474_976_710_600u64)]),
    ];
    let calldata = batch.encode(&args).unwrap();
    assert_eq!(
        hex::encode_prefixed(&calldata),
        "0xea2102e4000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000185196000000000000000000000000000000000000000000000000000000000000f42400000000000000000000000000000000000000000000000000000000000000002000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb9226600000000000000000000000070997970c51812dc3a010c7d01b50e0d17dc79c8000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000186a10000000000000000000000000000000000000000000000000000ffffffffffc8"
    );
    assert_eq!(batch.decode_input(&calldata).unwrap(), args);

    let mixed = Function::parse("f(string,bytes,bool)").unwrap();
    let args = [Token::String("lighter".to_owned()), Token::Bytes(vec![1, 2]), Token::Bool(true)];
    let calldata = mixed.encode(&args).unwrap();
    assert_eq!(
        hex::encode_prefixed(&calldata),
        "0xd79eeaa3000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000076c6967687465720000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020102000000000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(mixed.decode_input(&calldata).unwrap(), args);
    let read = &mixed.decode_input(&calldata).unwrap();
    assert_eq!(
        (read[0].as_str(), read[1].as_bytes(), read[2].as_bool()),
        (Some("lighter"), Some(&[1u8, 2][..]), Some(true))
    );
}

#[test]
fn values_and_words_that_do_not_fit_their_type_are_refused() {
    let withdraw = Function::parse("withdraw(uint48,uint16,uint8,uint64)").unwrap();
    let too_wide = [Token::uint(1u64 << 48), Token::uint(3u8), Token::uint(0u8), Token::uint(1u8)];
    assert!(matches!(withdraw.encode(&too_wide), Err(AbiError::Mismatch { .. })), "2^48 is past a uint48");
    assert!(matches!(withdraw.encode(&too_wide[..3]), Err(AbiError::Count { wants: 4, got: 3 })));
    let wrong = [Token::Bool(true), Token::uint(3u8), Token::uint(0u8), Token::uint(1u8)];
    assert!(matches!(withdraw.encode(&wrong), Err(AbiError::Mismatch { .. })));

    let uint8 = [Kind::Uint(8)];
    assert!(matches!(decode(&uint8, &U256::from(256u16).to_be_bytes()), Err(AbiError::Misfit { at: 0, .. })));
    assert_eq!(decode(&uint8, &U256::from(255u8).to_be_bytes()).unwrap(), [Token::uint(255u8)]);
    assert!(matches!(decode(&[Kind::Bool], &U256::from(2u8).to_be_bytes()), Err(AbiError::Misfit { .. })));
    let mut dirty = [0u8; 32];
    dirty[0] = 1;
    assert!(matches!(decode(&[Kind::Address], &dirty), Err(AbiError::Misfit { .. })), "high bytes set");
    assert!(matches!(decode(&uint8, &[0u8; 31]), Err(AbiError::Short(31))));
    // An offset or a length past what is believed, and an array longer than the data.
    assert!(matches!(decode(&[Kind::Bytes], &U256::MAX.to_be_bytes()), Err(AbiError::Misfit { .. })));
    let mut long = U256::from(32u8).to_be_bytes().to_vec();
    long.extend(U256::from(1000u16).to_be_bytes());
    assert!(matches!(decode(&[Kind::Array(Box::new(Kind::Uint(8)))], &long), Err(AbiError::Short(_))));
    assert!(matches!(withdraw.decode_input(&[0xde, 0xad, 0xbe, 0xef]), Err(AbiError::Selector { .. })));
}
