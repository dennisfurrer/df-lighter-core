//! [`U256`], the EVM's word as an exact unsigned integer: built from bytes and
//! narrower integers, read back into them, and checked arithmetic.

/// An unsigned 256-bit integer. The limbs are most significant first, so the
/// derived order is the numeric one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct U256([u64; 4]);

impl U256 {
    pub const ZERO: U256 = U256([0; 4]);
    pub const ONE: U256 = U256([0, 0, 0, 1]);
    pub const MAX: U256 = U256([u64::MAX; 4]);

    #[must_use]
    pub const fn from_u64(value: u64) -> U256 {
        U256([0, 0, 0, value])
    }

    #[must_use]
    pub const fn from_u128(value: u128) -> U256 {
        U256([0, 0, (value >> 64) as u64, value as u64])
    }

    #[must_use]
    pub fn from_be_bytes(bytes: [u8; 32]) -> U256 {
        let mut limbs = [0u64; 4];
        for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks_exact(8)) {
            let mut eight = [0u8; 8];
            eight.copy_from_slice(chunk);
            *limb = u64::from_be_bytes(eight);
        }
        U256(limbs)
    }

    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for (chunk, limb) in bytes.chunks_exact_mut(8).zip(self.0) {
            chunk.copy_from_slice(&limb.to_be_bytes());
        }
        bytes
    }

    /// Up to 32 big-endian bytes, a shorter run read as if zero-padded on the left.
    #[must_use]
    pub fn from_be_slice(bytes: &[u8]) -> Option<U256> {
        let start = 32usize.checked_sub(bytes.len())?;
        let mut word = [0u8; 32];
        word[start..].copy_from_slice(bytes);
        Some(U256::from_be_bytes(word))
    }

    /// The value as big-endian bytes with no leading zeros; zero is empty.
    #[must_use]
    pub fn to_be_trimmed(self) -> Vec<u8> {
        let bytes = self.to_be_bytes();
        let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(32);
        bytes[first..].to_vec()
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    #[must_use]
    pub const fn to_u64(self) -> Option<u64> {
        match self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 {
            true => Some(self.0[3]),
            false => None,
        }
    }

    #[must_use]
    pub const fn to_u128(self) -> Option<u128> {
        match self.0[0] == 0 && self.0[1] == 0 {
            true => Some(((self.0[2] as u128) << 64) | self.0[3] as u128),
            false => None,
        }
    }

    /// How many bits the value needs: 0 for zero, 256 for [`U256::MAX`].
    #[must_use]
    pub fn bits(self) -> u32 {
        let mut above = 256;
        for limb in self.0 {
            if limb != 0 {
                return above - limb.leading_zeros();
            }
            above -= 64;
        }
        0
    }

    #[must_use]
    pub fn checked_add(self, other: U256) -> Option<U256> {
        let mut out = [0u64; 4];
        let mut carry = false;
        for at in (0..4).rev() {
            let (sum, first) = self.0[at].overflowing_add(other.0[at]);
            let (sum, second) = sum.overflowing_add(u64::from(carry));
            out[at] = sum;
            carry = first || second;
        }
        (!carry).then_some(U256(out))
    }

    #[must_use]
    pub fn checked_sub(self, other: U256) -> Option<U256> {
        let mut out = [0u64; 4];
        let mut borrow = false;
        for at in (0..4).rev() {
            let (difference, first) = self.0[at].overflowing_sub(other.0[at]);
            let (difference, second) = difference.overflowing_sub(u64::from(borrow));
            out[at] = difference;
            borrow = first || second;
        }
        (!borrow).then_some(U256(out))
    }

    #[must_use]
    pub fn checked_mul(self, other: U256) -> Option<U256> {
        // Little-endian limbs for the schoolbook product; anything past four is overflow.
        let a = self.little();
        let b = other.little();
        let mut product = [0u64; 8];
        for (i, &left) in a.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &right) in b.iter().enumerate() {
                let wide = u128::from(left) * u128::from(right) + u128::from(product[i + j]) + carry;
                product[i + j] = wide as u64;
                carry = wide >> 64;
            }
            product[i + 4] = carry as u64;
        }
        if product[4..].iter().any(|limb| *limb != 0) {
            return None;
        }
        Some(U256([product[3], product[2], product[1], product[0]]))
    }

    #[must_use]
    pub fn saturating_add(self, other: U256) -> U256 {
        self.checked_add(other).unwrap_or(U256::MAX)
    }

    #[must_use]
    pub fn saturating_mul(self, other: U256) -> U256 {
        self.checked_mul(other).unwrap_or(U256::MAX)
    }

    /// `10^power`, while it fits: up to `10^77`.
    #[must_use]
    pub fn ten_to(power: u32) -> Option<U256> {
        let ten = U256::from_u64(10);
        (0..power).try_fold(U256::ONE, |value, _| value.checked_mul(ten))
    }

    /// The quotient and remainder of a division by a nonzero `u64`.
    #[must_use]
    pub fn div_rem_u64(self, divisor: u64) -> Option<(U256, u64)> {
        if divisor == 0 {
            return None;
        }
        let mut quotient = [0u64; 4];
        let mut remainder = 0u128;
        for (at, limb) in self.0.iter().enumerate() {
            let wide = (remainder << 64) | u128::from(*limb);
            quotient[at] = (wide / u128::from(divisor)) as u64;
            remainder = wide % u128::from(divisor);
        }
        Some((U256(quotient), remainder as u64))
    }

    /// The quotient and remainder of a division by a nonzero divisor, by long
    /// division one bit at a time.
    #[must_use]
    pub fn div_rem(self, divisor: U256) -> Option<(U256, U256)> {
        if divisor.is_zero() {
            return None;
        }
        let mut quotient = U256::ZERO;
        let mut remainder = U256::ZERO;
        for bit in (0..256).rev() {
            remainder = remainder.shifted_left_one();
            if self.bit(bit) {
                remainder.0[3] |= 1;
            }
            if remainder >= divisor {
                remainder = remainder.checked_sub(divisor)?;
                quotient.set_bit(bit);
            }
        }
        Some((quotient, remainder))
    }

    fn little(self) -> [u64; 4] {
        [self.0[3], self.0[2], self.0[1], self.0[0]]
    }

    fn bit(self, at: usize) -> bool {
        (self.0[3 - at / 64] >> (at % 64)) & 1 == 1
    }

    fn set_bit(&mut self, at: usize) {
        self.0[3 - at / 64] |= 1 << (at % 64);
    }

    fn shifted_left_one(self) -> U256 {
        let mut out = [0u64; 4];
        for (at, limb) in out.iter_mut().enumerate() {
            let carried = self.0.get(at + 1).map_or(0, |next| next >> 63);
            *limb = (self.0[at] << 1) | carried;
        }
        U256(out)
    }
}

impl From<u8> for U256 {
    fn from(value: u8) -> U256 {
        U256::from_u64(u64::from(value))
    }
}

impl From<u16> for U256 {
    fn from(value: u16) -> U256 {
        U256::from_u64(u64::from(value))
    }
}

impl From<u32> for U256 {
    fn from(value: u32) -> U256 {
        U256::from_u64(u64::from(value))
    }
}

impl From<u64> for U256 {
    fn from(value: u64) -> U256 {
        U256::from_u64(value)
    }
}

impl From<u128> for U256 {
    fn from(value: u128) -> U256 {
        U256::from_u128(value)
    }
}

#[cfg(test)]
#[path = "tests/uint.rs"]
mod tests;
