//! Scalars of ECgFp5: integers modulo the group order n, a prime just below 2^319, in five
//! little-endian 64-bit limbs. Multiplication is Montgomery's, as in Pornin's ecgfp5 and
//! elliottech/poseidon_crypto `curve/ecgfp5/scalar_field.go`.

use crate::utils::sign::fp5::Fp5;

/// The group order n.
const N: [u64; 5] =
    [0xE80F_D996_948B_FFE1, 0xE888_5C39_D724_A09C, 0x7FFF_FFE6_CFB8_0639, 0x7FFF_FFF1_0000_0016, 0x7FFF_FFFD_8000_0007];
/// -1/n mod 2^64.
const N0I: u64 = 0xD78B_EF72_057B_7BDF;
/// 2^640 mod n.
const R2: [u64; 5] =
    [0xA010_01DC_E33D_C739, 0x6C32_28D3_3F62_ACCF, 0xD1D7_96CC_91CF_8525, 0xAADF_FF5D_1574_C1D8, 0x4ACA_13B2_8CA2_51F5];

/// One scalar, always below n. Its `Debug` shows nothing: a secret key is one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Scalar([u64; 5]);

impl std::fmt::Debug for Scalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Scalar(..)")
    }
}

impl zeroize::Zeroize for Scalar {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

/// `a - b` over 320 bits and whether it borrowed.
fn sub_inner(a: &[u64; 5], b: &[u64; 5]) -> ([u64; 5], bool) {
    let mut out = [0u64; 5];
    let mut borrow = false;
    for i in 0..5 {
        let (d, b1) = a[i].overflowing_sub(b[i]);
        let (d, b2) = d.overflowing_sub(u64::from(borrow));
        out[i] = d;
        borrow = b1 || b2;
    }
    (out, borrow)
}

/// `a + b` over 320 bits, wrapping.
fn add_inner(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let mut out = [0u64; 5];
    let mut carry = false;
    for i in 0..5 {
        let (s, c1) = a[i].overflowing_add(b[i]);
        let (s, c2) = s.overflowing_add(u64::from(carry));
        out[i] = s;
        carry = c1 || c2;
    }
    out
}

/// `a` when `take_a`, else `b`, without a branch.
fn select(take_a: bool, a: [u64; 5], b: [u64; 5]) -> [u64; 5] {
    let m = 0u64.wrapping_sub(u64::from(take_a));
    std::array::from_fn(|i| (a[i] & m) | (b[i] & !m))
}

/// One conditional subtraction of n.
fn reduce_once(v: [u64; 5]) -> [u64; 5] {
    let (r, borrow) = sub_inner(&v, &N);
    select(borrow, v, r)
}

/// (a * b) / 2^320 mod n, for `a` below n.
fn monty_mul(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let mut r = [0u64; 5];
    for &m in b {
        let f = a[0].wrapping_mul(m).wrapping_add(r[0]).wrapping_mul(N0I);
        let (mut cc1, mut cc2) = (0u64, 0u64);
        for j in 0..5 {
            let z = u128::from(a[j]) * u128::from(m) + u128::from(r[j]) + u128::from(cc1);
            cc1 = (z >> 64) as u64;
            let z = u128::from(f) * u128::from(N[j]) + u128::from(z as u64) + u128::from(cc2);
            cc2 = (z >> 64) as u64;
            if j > 0 {
                r[j - 1] = z as u64;
            }
        }
        r[4] = cc1.wrapping_add(cc2);
    }
    reduce_once(r)
}

impl Scalar {
    pub const ZERO: Scalar = Scalar([0; 5]);

    /// Any 320-bit value, reduced mod n (it is below 3n).
    fn reduce(v: [u64; 5]) -> Scalar {
        Scalar(reduce_once(reduce_once(v)))
    }

    /// 40 little-endian bytes, reduced mod n (`ScalarElementFromLittleEndianBytes`).
    #[must_use]
    pub fn from_le_bytes(bytes: &[u8; 40]) -> Scalar {
        let mut limbs = [0u64; 5];
        for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks_exact(8)) {
            let mut b = [0u8; 8];
            b.copy_from_slice(chunk);
            *limb = u64::from_le_bytes(b);
        }
        Scalar::reduce(limbs)
    }

    /// 40 little-endian bytes only when they are below n, as a signature's halves must be.
    #[must_use]
    pub fn from_canonical_le_bytes(bytes: &[u8; 40]) -> Option<Scalar> {
        let mut limbs = [0u64; 5];
        for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks_exact(8)) {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            *limb = u64::from_le_bytes(word);
        }
        let (_, below) = sub_inner(&limbs, &N);
        below.then_some(Scalar(limbs))
    }

    #[must_use]
    pub fn to_le_bytes(self) -> [u8; 40] {
        let mut out = [0u8; 40];
        for (chunk, limb) in out.chunks_exact_mut(8).zip(self.0) {
            chunk.copy_from_slice(&limb.to_le_bytes());
        }
        out
    }

    /// A GF(p^5) element read as the 320-bit integer of its limbs, mod n (`FromGfp5`).
    #[must_use]
    pub fn from_fp5(x: Fp5) -> Scalar {
        Scalar::reduce(x.0.map(|l| l.value()))
    }

    /// Go's `crypto/rand.Int(reader, n)` for one 40-byte draw: big-endian, the top bit
    /// cleared, kept only when below n. `None` asks for another draw.
    #[must_use]
    pub fn from_rand_draw(draw: &[u8; 40]) -> Option<Scalar> {
        let mut be = *draw;
        be[0] &= 0x7F;
        let mut limbs = [0u64; 5];
        for (i, limb) in limbs.iter_mut().enumerate() {
            let mut b = [0u8; 8];
            b.copy_from_slice(&be[32 - 8 * i..40 - 8 * i]);
            *limb = u64::from_be_bytes(b);
        }
        let (_, below) = sub_inner(&limbs, &N);
        below.then_some(Scalar(limbs))
    }

    #[must_use]
    pub(crate) fn sub(self, rhs: Scalar) -> Scalar {
        let (r, borrow) = sub_inner(&self.0, &rhs.0);
        Scalar(select(borrow, add_inner(&r, &N), r))
    }

    #[must_use]
    pub(crate) fn mul(self, rhs: Scalar) -> Scalar {
        Scalar(monty_mul(&monty_mul(&self.0, &R2), &rhs.0))
    }

    /// Bit `i`, 0 or 1.
    #[must_use]
    pub(crate) fn bit(self, i: usize) -> u64 {
        (self.0[i / 64] >> (i % 64)) & 1
    }
}

#[cfg(test)]
#[path = "tests/scalar.rs"]
mod tests;
