//! Keys and signatures: what is printed, what is refused, and that a signature verifies.

use super::*;

#[test]
fn a_secret_is_never_printed_and_a_bad_key_never_quoted() {
    let key = SecretKey::from_le_bytes(&[9; 40]);
    assert_eq!(format!("{key:?}"), "SecretKey(..)");
    let error = SecretKey::from_hex("0xnot-a-key").unwrap_err();
    assert!(!error.to_string().contains("not-a-key"));
    assert_eq!(SecretKey::from_hex("0x00"), Err(BadKey::Length));
}

#[test]
fn a_public_key_reads_with_or_without_its_prefix_and_only_canonical() {
    let key = SecretKey::from_le_bytes(&[9; 40]).public_key();
    let plain = key.to_hex().trim_start_matches("0x").to_uppercase();
    assert!(key.is(&plain), "the venue lists keys bare");
    assert_eq!(PublicKey::from_hex(&key.to_hex()), Ok(key));
    assert_eq!(PublicKey::from_bytes([0xff; 40]), Err(BadKey::NotCanonical));
    assert_eq!(key.to_string(), key.to_hex());
}

#[test]
fn a_signature_verifies_for_its_key_and_message_only() {
    let key = SecretKey::from_le_bytes(&[9; 40]);
    let message = Fp5::from_u64s([1, 2, 3, 4, 5]);
    let mut counter = 0u8;
    let signature = key
        .sign_drawing(message, |draw: &mut [u8; 40]| {
            counter += 1;
            draw.fill(counter);
            Ok::<(), ()>(())
        })
        .unwrap();
    assert!(key.verifies(message, &signature));
    assert!(!key.verifies(Fp5::from_u64s([1, 2, 3, 4, 6]), &signature));
    assert!(signature.halves().is_some());
    assert_eq!(Signature::from_bytes([0xff; 80]).halves(), None, "halves past n are not a signature");
    assert_eq!(signature.to_hex().len(), 160);
}

#[test]
fn a_nonce_is_drawn_until_one_is_below_n() {
    let mut draws = [[0xffu8; 40], [0x01; 40]].into_iter();
    let scalar = sample_scalar(|draw: &mut [u8; 40]| {
        *draw = draws.next().ok_or(())?;
        Ok::<(), ()>(())
    });
    assert!(scalar.is_ok());
    assert_eq!(draws.next(), None, "the first draw was not below n");
}

/// The venue holds only the public key: lighter-go's own signatures verify with it, and
/// anything changed does not.
#[test]
fn a_public_key_alone_verifies_what_its_secret_signed() {
    let vectors = crate::test_support::vectors();
    for name in ["a", "b"] {
        let secret = SecretKey::from_hex(vectors["keys"][name]["private_key"].as_str().unwrap()).unwrap();
        let public = secret.public_key();
        for seed in 0u64..6 {
            let hashed = Fp5::from_u64s([seed, seed * 7, 3, 0, seed << 40]);
            let mut draws = (0u8..).map(|n| [n.wrapping_mul(31).wrapping_add(u8::try_from(seed).unwrap()); 40]);
            let signature = secret.sign_drawing(hashed, |bytes: &mut [u8; 40]| {
                *bytes = draws.next().unwrap();
                bytes[39] &= 0x0f;
                Ok::<(), ()>(())
            });
            let signature = signature.unwrap();
            assert!(public.verifies(hashed, &signature), "{name} {seed}");
            let other = Fp5::from_u64s([seed + 1, seed * 7, 3, 0, seed << 40]);
            assert!(!public.verifies(other, &signature), "{name} {seed}: another hash");
            let mut forged = *signature.bytes();
            forged[3] ^= 1;
            assert!(!public.verifies(hashed, &Signature::from_bytes(forged)), "{name} {seed}: a changed s");
        }
        let elsewhere =
            SecretKey::from_hex(vectors["keys"][if name == "a" { "b" } else { "a" }]["private_key"].as_str().unwrap())
                .unwrap()
                .public_key();
        let hashed = Fp5::from_u64s([1, 2, 3, 4, 5]);
        let signature = secret.sign_drawing(hashed, |bytes: &mut [u8; 40]| {
            *bytes = [9; 40];
            bytes[39] = 0;
            Ok::<(), ()>(())
        });
        assert!(!elsewhere.verifies(hashed, &signature.unwrap()), "{name}: another key");
    }
}

/// Every point decodes from its encoding: the generator's multiples and the neutral point.
#[test]
fn an_encoded_point_decodes_to_the_same_point() {
    use crate::utils::sign::curve::Point;
    use crate::utils::sign::scalar::Scalar;
    assert!(Point::decode(Fp5::ZERO).is_some_and(|point| point.encode() == Fp5::ZERO));
    for k in 1u8..20 {
        let mut bytes = [0u8; 40];
        bytes[0] = k;
        bytes[9] = k.wrapping_mul(13);
        let point = Point::GENERATOR.mul(Scalar::from_le_bytes(&bytes));
        let w = point.encode();
        let decoded = Point::decode(w).unwrap();
        assert_eq!(decoded.encode(), w, "{k}");
        assert_eq!(decoded.add(point).encode(), point.double().encode(), "{k}: the same group element");
    }
    assert!(
        Point::decode(Fp5::from_u64s([1, 0, 0, 0, 0])).is_none()
            || Point::decode(Fp5::from_u64s([2, 0, 0, 0, 0])).is_none()
    );
}
