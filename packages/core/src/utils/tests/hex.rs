//! Hex both ways, and what a decode refuses.

use super::*;

#[test]
fn hex_is_written_lowercase_and_read_in_any_case_with_or_without_its_prefix() {
    for (bytes, plain, prefixed) in [
        (&b"hello world"[..], "68656c6c6f20776f726c64", "0x68656c6c6f20776f726c64"),
        (&[0xde, 0xad, 0xbe, 0xef][..], "deadbeef", "0xdeadbeef"),
        (&[][..], "", "0x"),
    ] {
        assert_eq!((encode(bytes), encode_prefixed(bytes)), (plain.to_owned(), prefixed.to_owned()));
    }
    for text in ["0xDEADBEEF", "deadbeef", "  0xdeadbeef\n", "0XdeadBEEF"] {
        assert_eq!(decode(text), Ok(vec![0xde, 0xad, 0xbe, 0xef]), "{text:?}");
    }
    assert_eq!(decode_array::<4>("0xdeadbeef"), Ok([0xde, 0xad, 0xbe, 0xef]));
}

/// A refused secret is never quoted back in the error.
#[test]
fn a_decode_refuses_odd_lengths_wrong_lengths_and_non_digits() {
    assert_eq!(decode_array::<32>("0xdeadbeef"), Err(BadHex::WrongLength { expected: 32, found: 4 }));
    assert_eq!(decode("0xabc"), Err(BadHex::OddLength(3)));
    assert_eq!(decode_array::<2>("0xabc"), Err(BadHex::OddLength(3)));
    assert_eq!(decode("0xzz"), Err(BadHex::NotAHexDigit('z')));
    let error = decode_array::<32>("0xnot-a-key").unwrap_err();
    assert!(!error.to_string().contains("not-a-key"), "{error} leaked the input");
}
