//! GF(p^5) = GF(p)\[X\]/(X^5 - 3) over Goldilocks: the field ECgFp5 is defined on, and the shape of
//! every Lighter transaction hash and public key, five limbs written as 40 bytes little-endian.

use crate::utils::sign::goldilocks::{Fp, P};

/// X^5 = W.
const W: Fp = Fp::new(3);

/// A primitive fifth root of unity: the Frobenius map multiplies limb i by its i-th power.
const DTH_ROOT: Fp = Fp::new(1_041_288_259_238_279_555);

/// One element, limb 0 the constant term.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Fp5(pub [Fp; 5]);

impl Fp5 {
    pub const ZERO: Fp5 = Fp5([Fp::ZERO; 5]);
    pub const ONE: Fp5 = Fp5([Fp::ONE, Fp::ZERO, Fp::ZERO, Fp::ZERO, Fp::ZERO]);

    #[must_use]
    pub const fn from_u64s(limbs: [u64; 5]) -> Fp5 {
        Fp5([Fp::new(limbs[0]), Fp::new(limbs[1]), Fp::new(limbs[2]), Fp::new(limbs[3]), Fp::new(limbs[4])])
    }

    #[must_use]
    pub(crate) fn add(self, rhs: Fp5) -> Fp5 {
        let (a, b) = (self.0, rhs.0);
        Fp5([a[0].add(b[0]), a[1].add(b[1]), a[2].add(b[2]), a[3].add(b[3]), a[4].add(b[4])])
    }

    #[must_use]
    pub(crate) fn sub(self, rhs: Fp5) -> Fp5 {
        let (a, b) = (self.0, rhs.0);
        Fp5([a[0].sub(b[0]), a[1].sub(b[1]), a[2].sub(b[2]), a[3].sub(b[3]), a[4].sub(b[4])])
    }

    #[must_use]
    pub(crate) fn double(self) -> Fp5 {
        self.add(self)
    }

    #[must_use]
    pub(crate) fn mul(self, rhs: Fp5) -> Fp5 {
        let (a, b) = (self.0, rhs.0);
        let c0 = a[0].mul(b[0]).add(W.mul(a[1].mul(b[4]).add(a[2].mul(b[3])).add(a[3].mul(b[2])).add(a[4].mul(b[1]))));
        let c1 = a[0].mul(b[1]).add(a[1].mul(b[0])).add(W.mul(a[2].mul(b[4]).add(a[3].mul(b[3])).add(a[4].mul(b[2]))));
        let c2 = a[0].mul(b[2]).add(a[1].mul(b[1])).add(a[2].mul(b[0])).add(W.mul(a[3].mul(b[4]).add(a[4].mul(b[3]))));
        let c3 = a[0].mul(b[3]).add(a[1].mul(b[2])).add(a[2].mul(b[1])).add(a[3].mul(b[0])).add(W.mul(a[4].mul(b[4])));
        let c4 = a[0].mul(b[4]).add(a[1].mul(b[3])).add(a[2].mul(b[2])).add(a[3].mul(b[1])).add(a[4].mul(b[0]));
        Fp5([c0, c1, c2, c3, c4])
    }

    #[must_use]
    pub(crate) fn square(self) -> Fp5 {
        self.mul(self)
    }

    #[must_use]
    pub(crate) fn scale(self, k: Fp) -> Fp5 {
        let a = self.0;
        Fp5([a[0].mul(k), a[1].mul(k), a[2].mul(k), a[3].mul(k), a[4].mul(k)])
    }

    /// The Frobenius map applied `count` times: limb i times DTH_ROOT^(count * i).
    fn frobenius(self, count: u64) -> Fp5 {
        let step = DTH_ROOT.pow(count % 5);
        let mut factor = Fp::ONE;
        let mut limbs = self.0;
        for limb in &mut limbs {
            *limb = limb.mul(factor);
            factor = factor.mul(step);
        }
        Fp5(limbs)
    }

    /// 1/self through the norm down to GF(p); zero for zero.
    #[must_use]
    pub(crate) fn inverse_or_zero(self) -> Fp5 {
        let d = self.frobenius(1);
        let e = d.mul(d.frobenius(1));
        let f = e.mul(e.frobenius(2));
        let (a, f5) = (self.0, f.0);
        let norm =
            a[0].mul(f5[0]).add(W.mul(a[1].mul(f5[4]).add(a[2].mul(f5[3])).add(a[3].mul(f5[2])).add(a[4].mul(f5[1]))));
        f.scale(norm.inverse_or_zero())
    }

    #[must_use]
    pub(crate) fn is_zero(self) -> bool {
        self.0.iter().all(|limb| limb.value() == 0)
    }

    #[must_use]
    pub(crate) fn neg(self) -> Fp5 {
        Fp5::ZERO.sub(self)
    }

    #[must_use]
    pub(crate) fn half(self) -> Fp5 {
        self.scale(Fp::new(2).inverse_or_zero())
    }

    fn exp_power_of_2(self, n: u32) -> Fp5 {
        (0..n).fold(self, |x, _| x.square())
    }

    /// 1 for a non-zero square, -1 for a non-square, 0 for zero: x^((p^5 - 1)/2) through the
    /// norm, as poseidon_crypto's `Legendre`.
    #[must_use]
    pub(crate) fn legendre(self) -> Fp {
        let frob1 = self.frobenius(1);
        let frob2 = frob1.frobenius(1);
        let frob12 = frob1.mul(frob2);
        let norm = self.mul(frob12).mul(frob12.frobenius(2)).0[0];
        let r31 = norm.exp_power_of_2(31);
        r31.exp_power_of_2(32).mul(r31.inverse_or_zero())
    }

    /// A square root, as poseidon_crypto's `Sqrt` finds it; `None` when there is none.
    #[must_use]
    pub(crate) fn sqrt(self) -> Option<Fp5> {
        let v = self.exp_power_of_2(31);
        let d = self.mul(v.exp_power_of_2(32)).mul(v.inverse_or_zero());
        let e = d.mul(d.frobenius(2)).frobenius(1);
        let f = e.square().0;
        let x = self.0;
        let g = x[0].mul(f[0]).add(W.mul(x[1].mul(f[4]).add(x[2].mul(f[3])).add(x[3].mul(f[2])).add(x[4].mul(f[1]))));
        let s = g.sqrt()?;
        Some(e.inverse_or_zero().scale(s))
    }

    /// The square root whose sign (`Sgn0`) is clear: the one every decoder picks.
    #[must_use]
    pub(crate) fn canonical_sqrt(self) -> Option<Fp5> {
        let root = self.sqrt()?;
        Some(match root.sgn0() {
            true => root.neg(),
            false => root,
        })
    }

    /// poseidon_crypto's `Sgn0`, as written there: true when the first non-zero limb is even.
    fn sgn0(self) -> bool {
        let mut sign = false;
        let mut zero = true;
        for limb in self.0 {
            let even = limb.value() & 1 == 0;
            sign = sign || (zero && even);
            zero = zero && limb.value() == 0;
        }
        sign
    }

    /// Forty bytes, five limbs little-endian.
    #[must_use]
    pub fn to_le_bytes(self) -> [u8; 40] {
        let mut bytes = [0u8; 40];
        for (chunk, limb) in bytes.chunks_exact_mut(8).zip(self.0) {
            chunk.copy_from_slice(&limb.value().to_le_bytes());
        }
        bytes
    }

    /// Five little-endian limbs, each below p; `None` for any that is not.
    #[must_use]
    pub fn from_le_bytes(bytes: &[u8; 40]) -> Option<Fp5> {
        let mut limbs = [Fp::ZERO; 5];
        for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks_exact(8)) {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            let value = u64::from_le_bytes(word);
            if value >= P {
                return None;
            }
            *limb = Fp::new(value);
        }
        Some(Fp5(limbs))
    }

    /// `a` where `take_a` is all ones, `b` where it is zero, without a branch.
    #[must_use]
    pub(crate) fn select(take_a: u64, a: Fp5, b: Fp5) -> Fp5 {
        let mut limbs = [Fp::ZERO; 5];
        for (limb, (x, y)) in limbs.iter_mut().zip(a.0.iter().zip(b.0.iter())) {
            *limb = Fp::new((x.value() & take_a) | (y.value() & !take_a));
        }
        Fp5(limbs)
    }
}

#[cfg(test)]
#[path = "tests/fp5.rs"]
mod tests;
