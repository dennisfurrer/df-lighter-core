//! The base field: wrap-around at p, inverses, and bytes read only when canonical.

use super::*;

#[test]
fn arithmetic_wraps_at_p() {
    let top = Fp::new(P - 1);
    assert_eq!(top.add(Fp::ONE), Fp::ZERO);
    assert_eq!(Fp::ZERO.sub(Fp::ONE), top);
    assert_eq!(top.mul(top), Fp::ONE, "(-1)^2");
    assert_eq!(Fp::new(P), Fp::ZERO);
    assert_eq!(Fp::new(u64::MAX).value(), u64::MAX - P);
    let x = Fp::new(0x1234_5678_9abc_def0);
    assert_eq!(x.mul(x.inverse_or_zero()), Fp::ONE);
    assert_eq!(Fp::ZERO.inverse_or_zero(), Fp::ZERO);
    assert_eq!(Fp::new(3).mul_add(Fp::new(4), Fp::new(5)), Fp::new(23));
    assert_eq!(x.neg().add(x), Fp::ZERO);
}

#[test]
fn bytes_read_eight_at_a_time_little_endian_and_padded() {
    let chunks = Fp::from_le_chunks(b"1790025200:100001:4").unwrap();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0], Fp::new(u64::from_le_bytes(*b"17900252")));
    assert_eq!(chunks[2], Fp::new(u64::from_le_bytes([b'1', b':', b'4', 0, 0, 0, 0, 0])));
    assert_eq!(Fp::from_le_chunks(&[0xff; 8]), None, "not below p");
    assert_eq!(Fp::canonical(P), None);
}
