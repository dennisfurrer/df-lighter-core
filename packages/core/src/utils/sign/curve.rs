//! ECgFp5 (Thomas Pornin, 2022): y^2 = x(x^2 + 2x + 263z) over GF(p^5), a prime-order
//! group whose law is P + Q + N with N = (0, 0). Points are kept in the fractional
//! (x, u) coordinates of the paper, u = x/y, with the complete formulas of Pornin's
//! ecgfp5 and elliottech/poseidon_crypto `curve/ecgfp5/point.go`. A group element
//! encodes as one GF(p^5) value, w = y/x = t/u, which is Lighter's public key.

use crate::utils::sign::fp5::Fp5;
use crate::utils::sign::scalar::Scalar;

const B1: u64 = 263;
const B: Fp5 = Fp5::from_u64s([0, B1, 0, 0, 0]);
const B_MUL2: Fp5 = Fp5::from_u64s([0, 2 * B1, 0, 0, 0]);
const B_MUL4: Fp5 = Fp5::from_u64s([0, 4 * B1, 0, 0, 0]);
const A: Fp5 = Fp5::from_u64s([2, 0, 0, 0, 0]);
const FOUR: Fp5 = Fp5::from_u64s([4, 0, 0, 0, 0]);

/// A group element in fractional coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Point {
    x: Fp5,
    z: Fp5,
    u: Fp5,
    t: Fp5,
}

impl Point {
    pub(crate) const NEUTRAL: Point = Point { x: Fp5::ZERO, z: Fp5::ONE, u: Fp5::ZERO, t: Fp5::ONE };

    pub(crate) const GENERATOR: Point = Point {
        x: Fp5::from_u64s([
            12883135586176881569,
            4356519642755055268,
            5248930565894896907,
            2165973894480315022,
            2448410071095648785,
        ]),
        z: Fp5::ONE,
        u: Fp5::ONE,
        t: FOUR,
    };

    /// The complete addition (10M).
    #[must_use]
    pub(crate) fn add(self, rhs: Point) -> Point {
        let (x1, z1, u1, tt1) = (self.x, self.z, self.u, self.t);
        let (x2, z2, u2, tt2) = (rhs.x, rhs.z, rhs.u, rhs.t);
        let t1 = x1.mul(x2);
        let t2 = z1.mul(z2);
        let t3 = u1.mul(u2);
        let t4 = tt1.mul(tt2);
        let t5 = x1.add(z1).mul(x2.add(z2)).sub(t1.add(t2));
        let t6 = u1.add(tt1).mul(u2.add(tt2)).sub(t3.add(t4));
        let t7 = t1.add(t2.mul(B));
        let t8 = t4.mul(t7);
        let t9 = t3.mul(t5.mul(B_MUL2).add(t7.double()));
        let t10 = t4.add(t3.double()).mul(t5.add(t7));
        Point { x: t10.sub(t8).mul(B), z: t8.sub(t9), u: t6.mul(t2.mul(B).sub(t1)), t: t8.add(t9) }
    }

    /// Doubling (4M + 5S).
    #[must_use]
    pub(crate) fn double(self) -> Point {
        let (x, z, u, t) = (self.x, self.z, self.u, self.t);
        let t1 = z.mul(t);
        let t2 = t1.mul(t);
        let x1 = t2.square();
        let z1 = t1.mul(u);
        let t3 = u.square();
        let w1 = t2.sub(t3.mul(x.add(z).double()));
        let t4 = z1.square();
        let x_new = t4.mul(B_MUL4);
        let z_new = w1.square();
        let u_new = w1.add(z1).square().sub(t4.add(z_new));
        let t_new = x1.double().sub(t4.mul(FOUR).add(z_new));
        Point { x: x_new, z: z_new, u: u_new, t: t_new }
    }

    fn select(take_a: u64, a: Point, b: Point) -> Point {
        Point {
            x: Fp5::select(take_a, a.x, b.x),
            z: Fp5::select(take_a, a.z, b.z),
            u: Fp5::select(take_a, a.u, b.u),
            t: Fp5::select(take_a, a.t, b.t),
        }
    }

    /// `k` times this point: double and always add, choosing without a branch.
    #[must_use]
    pub(crate) fn mul(self, k: Scalar) -> Point {
        let mut acc = Point::NEUTRAL;
        for i in (0..320).rev() {
            acc = acc.double();
            let sum = acc.add(self);
            acc = Point::select(0u64.wrapping_sub(k.bit(i)), sum, acc);
        }
        acc
    }

    /// The point `w` encodes, as poseidon_crypto's `Decode`: from w = y/x, x solves
    /// x^2 - (w^2 - a)x + b = 0, and of its two roots the one that is not a square. `None` for a
    /// value no point encodes to; zero is the neutral point.
    #[must_use]
    pub(crate) fn decode(w: Fp5) -> Option<Point> {
        let e = w.square().sub(A);
        let delta = e.square().sub(B_MUL4);
        let Some(r) = delta.canonical_sqrt() else {
            return w.is_zero().then_some(Point::NEUTRAL);
        };
        let x1 = e.add(r).half();
        let x = match x1.legendre().value() == 1 {
            true => e.sub(r).half(),
            false => x1,
        };
        Some(Point { x, z: Fp5::ONE, u: Fp5::ONE, t: w })
    }

    /// w = t/u; the neutral point encodes as zero.
    #[must_use]
    pub(crate) fn encode(self) -> Fp5 {
        self.t.mul(self.u.inverse_or_zero())
    }
}

#[cfg(test)]
#[path = "tests/curve.rs"]
mod tests;
