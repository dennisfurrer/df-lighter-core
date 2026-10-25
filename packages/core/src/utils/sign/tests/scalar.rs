//! Scalars mod n: reduced on the way in, drawn as Go draws them, arithmetic that agrees with itself.

use super::*;

fn le(limbs: [u64; 5]) -> [u8; 40] {
    let mut bytes = [0u8; 40];
    for (chunk, limb) in bytes.chunks_exact_mut(8).zip(limbs) {
        chunk.copy_from_slice(&limb.to_le_bytes());
    }
    bytes
}

#[test]
fn n_itself_reduces_to_zero_and_is_not_canonical() {
    assert_eq!(Scalar::from_le_bytes(&le(N)), Scalar::ZERO);
    assert_eq!(Scalar::from_canonical_le_bytes(&le(N)), None);
    let mut below = N;
    below[0] -= 1;
    assert!(Scalar::from_canonical_le_bytes(&le(below)).is_some());
    assert_eq!(Scalar::from_le_bytes(&le(below)).to_le_bytes(), le(below));
}

#[test]
fn a_draw_is_read_big_endian_with_its_top_bit_cleared() {
    let mut draw = [0xffu8; 40];
    assert_eq!(Scalar::from_rand_draw(&draw), None, "2^319 - 1 is not below n: draw again");
    draw[0] = 0x80;
    draw[1..].fill(0);
    draw[39] = 5;
    let mut expected = [0u8; 40];
    expected[0] = 5;
    assert_eq!(Scalar::from_rand_draw(&draw).map(Scalar::to_le_bytes), Some(expected), "the top bit is dropped");
}

#[test]
fn arithmetic_agrees_with_itself() {
    let a = Scalar::from_le_bytes(&[0x5a; 40]);
    let b = Scalar::from_le_bytes(&[0xa7; 40]);
    let mut one = [0u8; 40];
    one[0] = 1;
    let one = Scalar::from_le_bytes(&one);
    assert_eq!(a.mul(one), a);
    assert_eq!(a.mul(b), b.mul(a));
    assert_eq!(a.sub(a), Scalar::ZERO);
    assert_eq!(Scalar::ZERO.sub(a).sub(b), Scalar::ZERO.sub(b).sub(a));
    assert_eq!(format!("{a:?}"), "Scalar(..)", "a secret is never printed");
}
