//! The quintic extension: inverses through the norm, and forty bytes read only when canonical.

use super::*;

#[test]
fn every_nonzero_element_has_an_inverse() {
    let x = Fp5::from_u64s([1, 2, 3, 4, 5]);
    assert_eq!(x.mul(x.inverse_or_zero()), Fp5::ONE);
    let y = Fp5::from_u64s([P - 1, 0, 0, 0, 7]);
    assert_eq!(y.mul(y.inverse_or_zero()), Fp5::ONE);
    assert_eq!(Fp5::ZERO.inverse_or_zero(), Fp5::ZERO);
    assert_eq!(x.mul(y), y.mul(x));
    assert_eq!(x.add(y).sub(y), x);
}

#[test]
fn bytes_round_trip_and_a_limb_at_p_is_refused() {
    let x = Fp5::from_u64s([1, 2, 3, 4, 5]);
    assert_eq!(Fp5::from_le_bytes(&x.to_le_bytes()), Some(x));
    let mut bytes = x.to_le_bytes();
    bytes[32..].copy_from_slice(&P.to_le_bytes());
    assert_eq!(Fp5::from_le_bytes(&bytes), None);
}
