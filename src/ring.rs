//! The ring Z_{2^n} for 1 <= n <= 256, with elements stored in a [`U256`].
//!
//! Elements are always kept reduced, i.e. in `[0, 2^n)`. Signed values use two's complement:
//! `x` represents `x - 2^n` when its MSB is set.

use crate::Error;
pub use ethnum::{I256, U256};
use rand::Rng;
use serde::{Deserialize, Serialize};

pub const MAX_BITS: u32 = 256;

/// Serialized as its bit width, which is checked on deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Ring {
    bits: u32,
}

impl TryFrom<u32> for Ring {
    type Error = Error;
    fn try_from(bits: u32) -> Result<Ring, Error> {
        Ring::new(bits)
    }
}

impl From<Ring> for u32 {
    fn from(r: Ring) -> u32 {
        r.bits
    }
}

impl Ring {
    pub fn new(bits: u32) -> Result<Ring, Error> {
        if bits == 0 || bits > MAX_BITS {
            return Err(Error::new(format!("ring bit width must be in 1..={MAX_BITS}, got {bits}")));
        }
        Ok(Ring { bits })
    }

    pub fn bits(&self) -> u32 {
        self.bits
    }

    /// Bytes per element on the wire and in key files: `ceil(n / 8)`.
    pub fn bytes(&self) -> usize {
        (self.bits as usize + 7) / 8
    }

    pub fn mask(&self) -> U256 {
        low_mask(self.bits)
    }

    pub fn reduce(&self, x: U256) -> U256 {
        x & self.mask()
    }

    pub fn add(&self, a: U256, b: U256) -> U256 {
        self.reduce(a.wrapping_add(b))
    }

    pub fn sub(&self, a: U256, b: U256) -> U256 {
        self.reduce(a.wrapping_sub(b))
    }

    pub fn neg(&self, a: U256) -> U256 {
        self.reduce(a.wrapping_neg())
    }

    pub fn mul(&self, a: U256, b: U256) -> U256 {
        self.reduce(a.wrapping_mul(b))
    }

    pub fn msb(&self, x: U256) -> bool {
        bit(x, self.bits - 1)
    }

    /// Smallest and largest signed value representable in n bits.
    pub fn signed_range(&self) -> (I256, I256) {
        if self.bits == MAX_BITS {
            (I256::MIN, I256::MAX)
        } else {
            (-(I256::ONE << (self.bits - 1)), (I256::ONE << (self.bits - 1)) - 1)
        }
    }

    /// Two's complement encoding of `x` (reduced mod 2^n; out-of-range values wrap).
    pub fn from_signed(&self, x: I256) -> U256 {
        self.reduce(x.as_u256())
    }

    pub fn to_signed(&self, x: U256) -> I256 {
        let x = self.reduce(x);
        if self.msb(x) {
            (x | !self.mask()).as_i256()
        } else {
            x.as_i256()
        }
    }

    pub fn random(&self) -> U256 {
        let mut rng = rand::thread_rng();
        self.reduce(U256::from_words(rng.gen(), rng.gen()))
    }

    /// Split `x` into two uniformly random additive shares.
    pub fn share(&self, x: U256) -> (U256, U256) {
        let s0 = self.random();
        (s0, self.sub(x, s0))
    }

    /// `x` as `ceil(n / 8)` little-endian bytes.
    pub fn to_bytes(&self, x: U256) -> Vec<u8> {
        self.reduce(x).to_le_bytes()[..self.bytes()].to_vec()
    }

    /// Inverse of [`Ring::to_bytes`]. Fails on a wrong length or a value of `2^n` or more.
    pub fn from_bytes(&self, bytes: &[u8]) -> Result<U256, Error> {
        if bytes.len() != self.bytes() {
            return Err(Error::new(format!("expected {} bytes per {}-bit ring element, got {}", self.bytes(), self.bits, bytes.len())));
        }
        let mut buf = [0u8; 32];
        buf[..bytes.len()].copy_from_slice(bytes);
        let x = U256::from_le_bytes(buf);
        if x & !self.mask() != 0 {
            return Err(Error::new(format!("value exceeds the {}-bit ring", self.bits)));
        }
        Ok(x)
    }

    /// The concatenated [`Ring::to_bytes`] encodings of `xs`.
    pub fn encode(&self, xs: &[U256]) -> Vec<u8> {
        xs.iter().flat_map(|&x| self.to_bytes(x)).collect()
    }

    /// Inverse of [`Ring::encode`].
    pub fn decode(&self, bytes: &[u8]) -> Result<Vec<U256>, Error> {
        if bytes.len() % self.bytes() != 0 {
            return Err(Error::new(format!("{} bytes is not a whole number of {}-byte ring elements", bytes.len(), self.bytes())));
        }
        bytes.chunks(self.bytes()).map(|c| self.from_bytes(c)).collect()
    }
}

/// `2^k - 1` for 0 <= k <= 256.
pub fn low_mask(k: u32) -> U256 {
    if k >= 256 {
        U256::MAX
    } else {
        (U256::ONE << k) - 1
    }
}

/// `x mod 2^k`, written `x[0,k)` in the paper.
pub fn low_bits(x: U256, k: u32) -> U256 {
    x & low_mask(k)
}

/// Bit `i` of `x`, written `x[i]` in the paper.
pub fn bit(x: U256, i: u32) -> bool {
    (x >> i) & 1 == 1
}

/// The low `k` bits of `x`, most significant first (the input order libfss DCF keys expect).
pub fn to_bits_msb_first(x: U256, k: u32) -> Vec<bool> {
    (0..k).rev().map(|i| bit(x, i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_roundtrip_extremes() {
        for bits in [1, 2, 7, 8, 63, 64, 65, 127, 128, 129, 200, 255, 256] {
            let r = Ring::new(bits).unwrap();
            let (lo, hi) = r.signed_range();
            for v in [lo, lo + 1, I256::MINUS_ONE, I256::ZERO, hi - 1, hi] {
                if v < lo || v > hi {
                    continue;
                }
                assert_eq!(r.to_signed(r.from_signed(v)), v, "bits={bits} v={v}");
            }
        }
    }

    #[test]
    fn rejects_bad_widths() {
        assert!(Ring::new(0).is_err());
        assert!(Ring::new(257).is_err());
    }

    #[test]
    fn bits_msb_first() {
        assert_eq!(to_bits_msb_first(U256::new(0b110), 3), vec![true, true, false]);
        assert_eq!(to_bits_msb_first(U256::new(0b110), 0), Vec::<bool>::new());
        assert!(to_bits_msb_first(U256::ONE << 255, 256)[0]);
    }

    #[test]
    fn bytes_roundtrip() {
        for bits in [1, 8, 9, 128, 129, 256] {
            let r = Ring::new(bits).unwrap();
            for x in [U256::ZERO, r.mask(), r.random()] {
                assert_eq!(r.to_bytes(x).len(), r.bytes());
                assert_eq!(r.from_bytes(&r.to_bytes(x)).unwrap(), x);
            }
        }
        let r = Ring::new(12).unwrap();
        assert!(r.from_bytes(&[0xff, 0x10]).is_err()); // bit 12 set
        assert!(r.from_bytes(&[0xff]).is_err()); // too short
    }
}
