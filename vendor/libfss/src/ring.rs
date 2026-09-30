// The ring Z_{2^BITS} for 1 <= BITS <= 128. Elements are stored in a u128 and always kept
// reduced, i.e. in [0, 2^BITS). `RingElm` without parameters is the upstream 32-bit ring.
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use std::cmp::Ordering;
use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RingElm<const BITS: u32 = 32> {
    value: u128,
}

impl<const BITS: u32> RingElm<BITS> {
    /// Evaluated at monomorphization: rejects widths outside 1..=128 at compile time.
    const VALID: () = assert!(BITS >= 1 && BITS <= 128, "RingElm width must be in 1..=128");

    pub const BITS: u32 = BITS;
    /// Bytes used by `to_u8_vec` and the serde encoding.
    pub const BYTES: usize = ((BITS + 7) / 8) as usize;
    pub const MASK: u128 = if BITS >= 128 { u128::MAX } else { (1u128 << BITS) - 1 };

    /// `value mod 2^BITS`.
    #[inline]
    pub fn new(value: u128) -> Self {
        let () = Self::VALID;
        RingElm { value: value & Self::MASK }
    }

    /// The largest element, 2^BITS - 1 (i.e. -1).
    #[inline]
    pub fn max() -> Self {
        Self::new(u128::MAX)
    }

    #[inline]
    pub fn value(&self) -> u128 {
        self.value
    }

    pub fn to_vec(&self, len: usize) -> Vec<Self> {
        std::iter::repeat(*self).take(len).collect()
    }

    pub fn print(&self) {
        print!("{} ", self.value);
    }

    pub fn to_u32(&self) -> Option<u32> {
        u32::try_from(self.value).ok()
    }

    pub fn to_u64(&self) -> Option<u64> {
        u64::try_from(self.value).ok()
    }

    pub fn to_u128(&self) -> u128 {
        self.value
    }

    /// The element's bits, most significant first, `BITS` of them.
    pub fn to_bits_BE(&self) -> Vec<bool> {
        crate::u128_to_bits_BE(BITS as usize, self.value)
    }

    /// Element from up to 128 bits, most significant first; reduced mod 2^BITS.
    pub fn from_bits_BE(bits: &[bool]) -> Self {
        Self::new(crate::bits_to_u128_BE(bits))
    }

    /// Big-endian encoding in `BYTES` bytes.
    pub fn to_u8_vec(&self) -> Vec<u8> {
        self.value.to_be_bytes()[16 - Self::BYTES..].to_vec()
    }

    /// Inverse of `to_u8_vec`. Panics unless `bytes.len() == BYTES`.
    pub fn from_u8_slice(bytes: &[u8]) -> Self {
        if bytes.len() != Self::BYTES {
            panic!("Invalid conversion: expected {} bytes, got {}", Self::BYTES, bytes.len());
        }
        let mut buf = [0u8; 16];
        buf[16 - Self::BYTES..].copy_from_slice(bytes);
        Self::new(u128::from_be_bytes(buf))
    }
}

impl<const BITS: u32> Add for RingElm<BITS> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.value.wrapping_add(rhs.value))
    }
}

impl<const BITS: u32> Sub for RingElm<BITS> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.value.wrapping_sub(rhs.value))
    }
}

impl<const BITS: u32> Mul for RingElm<BITS> {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(self.value.wrapping_mul(rhs.value))
    }
}

macro_rules! impl_from_uint {
    ($($t:ty),*) => {$(
        impl<const BITS: u32> From<$t> for RingElm<BITS> {
            #[inline]
            fn from(inp: $t) -> Self {
                Self::new(inp as u128)
            }
        }
    )*};
}
impl_from_uint!(u8, u16, u32, u64, u128, usize);

impl<const BITS: u32> From<Vec<u8>> for RingElm<BITS> {
    #[inline]
    fn from(bytes: Vec<u8>) -> Self {
        Self::from_u8_slice(&bytes)
    }
}

impl<const BITS: u32> Ord for RingElm<BITS> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<const BITS: u32> PartialOrd for RingElm<BITS> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const BITS: u32> crate::Group for RingElm<BITS> {
    #[inline]
    fn zero() -> Self {
        Self::new(0)
    }

    #[inline]
    fn one() -> Self {
        Self::new(1)
    }

    #[inline]
    fn add(&mut self, other: &Self) {
        *self = *self + *other;
    }

    #[inline]
    fn sub(&mut self, other: &Self) {
        *self = *self - *other;
    }

    #[inline]
    fn mul(&mut self, other: &Self) {
        *self = *self * *other;
    }

    #[inline]
    fn negate(&mut self) {
        *self = Self::new(self.value.wrapping_neg());
    }
}

impl<const BITS: u32> crate::prg::FromRng for RingElm<BITS> {
    #[inline]
    fn from_rng(&mut self, rng: &mut (impl rand::Rng + rand_core::RngCore)) {
        *self = Self::new(rng.gen::<u128>());
    }
}

impl<const BITS: u32> crate::Share for RingElm<BITS> {}

// Serialized as the smallest unsigned integer that holds BITS bits, so key sizes scale with the
// ring. For BITS = 32 this is the same encoding as upstream's `u32` field.
impl<const BITS: u32> Serialize for RingElm<BITS> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = self.value;
        match BITS {
            0..=8 => s.serialize_u8(v as u8),
            9..=16 => s.serialize_u16(v as u16),
            17..=32 => s.serialize_u32(v as u32),
            33..=64 => s.serialize_u64(v as u64),
            _ => s.serialize_u128(v),
        }
    }
}

impl<'de, const BITS: u32> Deserialize<'de> for RingElm<BITS> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: u128 = match BITS {
            0..=8 => u8::deserialize(d)?.into(),
            9..=16 => u16::deserialize(d)?.into(),
            17..=32 => u32::deserialize(d)?.into(),
            33..=64 => u64::deserialize(d)?.into(),
            _ => u128::deserialize(d)?,
        };
        if v & !Self::MASK != 0 {
            return Err(serde::de::Error::custom(format!("value does not fit in {} bits", BITS)));
        }
        Ok(Self::new(v))
    }
}

impl<T> crate::Group for (T, T) where T: crate::Group + Clone,
{
    #[inline]
    fn zero() -> Self {
        (T::zero(), T::zero())
    }

    #[inline]
    fn one() -> Self {
        (T::one(), T::one())
    }

    #[inline]
    fn add(&mut self, other: &Self) {
        self.0.add(&other.0);
        self.1.add(&other.1);
    }

    #[inline]
    fn mul(&mut self, other: &Self) {
        self.0.mul(&other.0);
        self.1.mul(&other.1);
    }

    #[inline]
    fn sub(&mut self, other: &Self) {
        let mut inv0 = other.0.clone();
        let mut inv1 = other.1.clone();
        inv0.negate();
        inv1.negate();
        self.0.add(&inv0);
        self.1.add(&inv1);
    }

    #[inline]
    fn negate(&mut self) {
        self.0.negate();
        self.1.negate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Group;

    #[test]
    fn wraps_at_width() {
        assert_eq!(RingElm::<8>::from(255u32) + RingElm::from(1u32), RingElm::zero());
        assert_eq!(RingElm::<1>::one() + RingElm::one(), RingElm::zero());
        assert_eq!(RingElm::<128>::max() + RingElm::one(), RingElm::zero());
        assert_eq!(RingElm::<13>::from(1u32 << 13), RingElm::zero());
        let mut x = RingElm::<64>::one();
        x.negate();
        assert_eq!(x.value(), u64::MAX as u128);
        assert_eq!(RingElm::<33>::from(3u32) * RingElm::from(1u64 << 32), RingElm::from(1u64 << 32));
    }

    #[test]
    fn default_is_32_bits() {
        let x: RingElm = RingElm::max();
        assert_eq!(x.to_u32(), Some(u32::MAX));
        assert_eq!(x.to_u8_vec().len(), 4);
    }

    #[test]
    fn bytes_and_bits_roundtrip() {
        let x = RingElm::<100>::new(0x0123_4567_89ab_cdef_0123_4567_89);
        assert_eq!(x.to_u8_vec().len(), 13);
        assert_eq!(RingElm::<100>::from(x.to_u8_vec()), x);
        assert_eq!(RingElm::<100>::from_bits_BE(&x.to_bits_BE()), x);
    }

    #[test]
    fn serde_roundtrip_and_size() {
        fn check<const B: u32>(v: u128, size: usize) {
            let x = RingElm::<B>::new(v);
            let enc = bincode::serialize(&x).unwrap();
            assert_eq!(enc.len(), size, "BITS={}", B);
            assert_eq!(bincode::deserialize::<RingElm<B>>(&enc).unwrap(), x);
        }
        check::<5>(u128::MAX, 1);
        check::<16>(u128::MAX, 2);
        check::<32>(u128::MAX, 4);
        check::<64>(u128::MAX, 8);
        check::<65>(u128::MAX, 16);
        check::<128>(u128::MAX, 16);
        assert!(bincode::deserialize::<RingElm<5>>(&[0xffu8]).is_err());
    }
}
