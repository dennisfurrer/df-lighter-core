//! The Goldilocks field, p = 2^64 - 2^32 + 1, every Lighter hash and key is built on. Values stay
//! canonical (below p) and the arithmetic has no data-dependent branches.

/// The modulus.
pub const P: u64 = 0xFFFF_FFFF_0000_0001;

/// 2^64 mod p.
const EPSILON: u64 = 0xFFFF_FFFF;

/// One element, always below p.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Fp(u64);

/// All ones when `bit` is true.
const fn mask(bit: bool) -> u64 {
    0u64.wrapping_sub(bit as u64)
}

/// `x` mod p for any u64: x < 2p, so one subtraction at most.
const fn canon(x: u64) -> u64 {
    let (y, borrow) = x.overflowing_sub(P);
    let keep = mask(borrow);
    (x & keep) | (y & !keep)
}

/// A 128-bit value mod p, using 2^64 = 2^32 - 1 and 2^96 = -1 (mod p).
const fn reduce128(x: u128) -> u64 {
    let low = x as u64;
    let high = (x >> 64) as u64;
    let high_high = high >> 32;
    let high_low = high & EPSILON;
    let (t0, borrow) = low.overflowing_sub(high_high);
    let t0 = t0.wrapping_sub(EPSILON & mask(borrow));
    let t1 = high_low * EPSILON;
    let (t2, carry) = t0.overflowing_add(t1);
    canon(t2.wrapping_add(EPSILON & mask(carry)))
}

impl Fp {
    pub const ZERO: Fp = Fp(0);
    pub const ONE: Fp = Fp(1);

    /// `value` mod p.
    #[must_use]
    pub const fn new(value: u64) -> Fp {
        Fp(canon(value))
    }

    /// `value` when it is below p; `None` for a non-canonical encoding.
    #[must_use]
    pub const fn canonical(value: u64) -> Option<Fp> {
        match value < P {
            true => Some(Fp(value)),
            false => None,
        }
    }

    /// The canonical value, below p.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn add(self, rhs: Fp) -> Fp {
        let (sum, over) = self.0.overflowing_add(rhs.0);
        Fp(canon(sum.wrapping_add(EPSILON & mask(over))))
    }

    #[must_use]
    pub const fn sub(self, rhs: Fp) -> Fp {
        let (difference, borrow) = self.0.overflowing_sub(rhs.0);
        Fp(difference.wrapping_sub(EPSILON & mask(borrow)))
    }

    #[must_use]
    pub const fn mul(self, rhs: Fp) -> Fp {
        Fp(reduce128(self.0 as u128 * rhs.0 as u128))
    }

    #[must_use]
    pub const fn square(self) -> Fp {
        self.mul(self)
    }

    /// `self + x * y`, reduced once.
    #[must_use]
    pub const fn mul_add(self, x: Fp, y: Fp) -> Fp {
        Fp(reduce128(self.0 as u128 + x.0 as u128 * y.0 as u128))
    }

    #[must_use]
    pub const fn neg(self) -> Fp {
        Fp::ZERO.sub(self)
    }

    #[must_use]
    pub fn pow(self, mut exponent: u64) -> Fp {
        let (mut base, mut power) = (self, Fp::ONE);
        while exponent > 0 {
            if exponent & 1 == 1 {
                power = power.mul(base);
            }
            base = base.square();
            exponent >>= 1;
        }
        power
    }

    /// `self` squared `n` times.
    #[must_use]
    pub(crate) fn exp_power_of_2(self, n: u32) -> Fp {
        (0..n).fold(self, |x, _| x.square())
    }

    /// A square root by Tonelli-Shanks (p - 1 = 2^32 t), as poseidon_crypto's `SqrtF` finds it;
    /// `None` when there is none.
    #[must_use]
    pub(crate) fn sqrt(self) -> Option<Fp> {
        const TWO_ADICITY: u32 = 32;
        const POWER_OF_TWO_GENERATOR: Fp = Fp::new(7_277_203_076_849_721_926);
        if self.value() == 0 {
            return Some(Fp::ZERO);
        }
        if self.pow((P - 1) >> 1).value() != 1 {
            return None;
        }
        let t = (P - 1) >> TWO_ADICITY;
        let mut z = POWER_OF_TWO_GENERATOR;
        let mut w = self.pow((t - 1) / 2);
        let mut x = self.mul(w);
        let mut b = x.mul(w);
        let mut v = TWO_ADICITY;
        while b.value() != 1 {
            let mut k = 0;
            let mut b2k = b;
            while b2k.value() != 1 {
                b2k = b2k.square();
                k += 1;
            }
            w = z.exp_power_of_2(v.checked_sub(k + 1)?);
            z = w.square();
            b = b.mul(z);
            x = x.mul(w);
            v = k;
        }
        Some(x)
    }

    /// 1/self, and zero for zero.
    #[must_use]
    pub fn inverse_or_zero(self) -> Fp {
        self.pow(P - 2)
    }

    /// A sum of up to 2^64 canonical values, reduced.
    #[must_use]
    pub const fn from_u128(value: u128) -> Fp {
        Fp(reduce128(value))
    }

    /// Bytes as elements, eight little-endian bytes each and the last zero-padded, as lighter-go's
    /// `ArrayFromCanonicalLittleEndianBytes` reads a message. `None` when a chunk is not below p.
    #[must_use]
    pub fn from_le_chunks(bytes: &[u8]) -> Option<Vec<Fp>> {
        bytes
            .chunks(8)
            .map(|chunk| {
                let mut word = [0u8; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                Fp::canonical(u64::from_le_bytes(word))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/goldilocks.rs"]
mod tests;
