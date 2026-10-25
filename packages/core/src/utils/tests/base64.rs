//! Standard base64 both ways, as Go writes a byte slice.

use super::*;

#[test]
fn the_rfc_4648_vectors_round_trip() {
    for (bytes, text) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(encode(bytes.as_bytes()), text);
        assert_eq!(decode(text).unwrap(), bytes.as_bytes(), "{text}");
    }
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(decode(&encode(&all)).unwrap(), all);
    assert!(encode(&[0xfb, 0xff]).contains('+') && encode(&[0xff, 0xff]).contains('/'));
}

#[test]
fn anything_else_is_refused() {
    for bad in ["Zg=", "Zg", "Z===", "Zg==Zg==", "Z!==", "Zm9v\n"] {
        assert_eq!(decode(bad), Err(BadBase64), "{bad:?}");
    }
}
