//! Dual distributed comparison function (DDCF) on top of the libfss DCF.
//!
//! `f(x) = beta1` if `x < alpha`, else `beta2`, for `x, alpha` in `{0,1}^m` (Boyle et al. 2021,
//! Sec. 3). It is a DCF with payload `beta1 - beta2` plus additive shares of `beta2`.
//!
//! Payloads are computed in Z_{2^128} (`Z`) or Z_{2^128}^2 (`Z2`), but only their value mod
//! 2^`payload_bits` is kept. Reduction mod 2^k is a ring homomorphism, so the DCF evaluates
//! correctly mod 2^k with every correction word reduced. A key therefore stores each payload in
//! `ceil(payload_bits / 8)` bytes instead of 16, and one payload type serves every ring width.

use crate::ring::{low_bits, low_mask, to_bits_msb_first};
use fss::dcf::{CorWord, DCFKey};
use fss::prg::{FromRng, PrgSeed};
use fss::Group;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Element of Z_{2^128}.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Z(pub u128);

/// Element of Z_{2^128} x Z_{2^128}.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Z2(pub u128, pub u128);

impl Group for Z {
    fn zero() -> Self {
        Z(0)
    }
    fn one() -> Self {
        Z(1)
    }
    fn negate(&mut self) {
        self.0 = self.0.wrapping_neg();
    }
    fn add(&mut self, o: &Self) {
        self.0 = self.0.wrapping_add(o.0);
    }
    fn mul(&mut self, o: &Self) {
        self.0 = self.0.wrapping_mul(o.0);
    }
    fn sub(&mut self, o: &Self) {
        self.0 = self.0.wrapping_sub(o.0);
    }
}

impl FromRng for Z {
    fn from_rng(&mut self, rng: &mut (impl rand::Rng + rand::RngCore)) {
        self.0 = rng.gen::<u128>();
    }
}

impl Group for Z2 {
    fn zero() -> Self {
        Z2(0, 0)
    }
    fn one() -> Self {
        Z2(1, 1)
    }
    fn negate(&mut self) {
        self.0 = self.0.wrapping_neg();
        self.1 = self.1.wrapping_neg();
    }
    fn add(&mut self, o: &Self) {
        self.0 = self.0.wrapping_add(o.0);
        self.1 = self.1.wrapping_add(o.1);
    }
    fn mul(&mut self, o: &Self) {
        self.0 = self.0.wrapping_mul(o.0);
        self.1 = self.1.wrapping_mul(o.1);
    }
    fn sub(&mut self, o: &Self) {
        self.0 = self.0.wrapping_sub(o.0);
        self.1 = self.1.wrapping_sub(o.1);
    }
}

impl FromRng for Z2 {
    fn from_rng(&mut self, rng: &mut (impl rand::Rng + rand::RngCore)) {
        self.0 = rng.gen::<u128>();
        self.1 = rng.gen::<u128>();
    }
}

pub trait Payload: FromRng + Group + Clone + Copy + std::fmt::Debug {
    /// Number of u128 components.
    const LIMBS: usize;
    fn map(self, f: impl Fn(u128) -> u128) -> Self;
    fn write_limbs(&self, out: &mut Vec<u128>);
    fn read_limbs(limbs: &[u128]) -> Self;
}

impl Payload for Z {
    const LIMBS: usize = 1;
    fn map(self, f: impl Fn(u128) -> u128) -> Self {
        Z(f(self.0))
    }
    fn write_limbs(&self, out: &mut Vec<u128>) {
        out.push(self.0);
    }
    fn read_limbs(l: &[u128]) -> Self {
        Z(l[0])
    }
}

impl Payload for Z2 {
    const LIMBS: usize = 2;
    fn map(self, f: impl Fn(u128) -> u128) -> Self {
        Z2(f(self.0), f(self.1))
    }
    fn write_limbs(&self, out: &mut Vec<u128>) {
        out.extend([self.0, self.1]);
    }
    fn read_limbs(l: &[u128]) -> Self {
        Z2(l[0], l[1])
    }
}

#[derive(Clone, Debug)]
pub struct DdcfKey<T> {
    domain_bits: u32,
    /// Payloads (and outputs) are reduced mod 2^payload_bits, 1..=128.
    payload_bits: u32,
    /// `None` when `domain_bits == 0`: the only input is the empty string, and `x < alpha` never holds.
    dcf: Option<DCFKey<T>>,
    beta2_share: T,
}

impl<T: Payload> DdcfKey<T> {
    /// Keys for `f(x) = beta1 if x < alpha else beta2` on `domain_bits`-bit inputs, with outputs
    /// in Z_{2^payload_bits}. `alpha` is reduced mod `2^domain_bits`.
    pub fn gen(domain_bits: u32, payload_bits: u32, alpha: u128, beta1: T, beta2: T) -> (Self, Self) {
        assert!(domain_bits <= 128);
        assert!((1..=128).contains(&payload_bits));
        let reduce = |t: T| t.map(|v| low_bits(v, payload_bits));
        let (dcf0, dcf1) = if domain_bits == 0 {
            (None, None)
        } else {
            let mut diff = beta1;
            diff.sub(&beta2);
            let alpha_bits = to_bits_msb_first(low_bits(alpha, domain_bits), domain_bits);
            let (mut k0, mut k1) = DCFKey::gen(&alpha_bits, &diff);
            for k in [&mut k0, &mut k1] {
                for cw in &mut k.cor_words {
                    cw.word = reduce(cw.word);
                }
                k.word = reduce(k.word);
            }
            (Some(k0), Some(k1))
        };
        let mut s0 = T::zero();
        s0.randomize();
        let mut s1 = beta2;
        s1.sub(&s0);
        let key = |dcf, share| DdcfKey { domain_bits, payload_bits, dcf, beta2_share: reduce(share) };
        (key(dcf0, s0), key(dcf1, s1))
    }

    /// This party's additive share of `f(x)` in Z_{2^payload_bits}; `x` is reduced mod `2^domain_bits`.
    pub fn eval(&self, x: u128) -> T {
        let mut out = self.beta2_share;
        if let Some(dcf) = &self.dcf {
            let x_bits = to_bits_msb_first(low_bits(x, self.domain_bits), self.domain_bits);
            out.add(&dcf.eval(&x_bits));
        }
        out.map(|v| low_bits(v, self.payload_bits))
    }

    /// The underlying libfss DCF key (`None` for a 0-bit domain).
    pub fn dcf(&self) -> Option<&DCFKey<T>> {
        self.dcf.as_ref()
    }
}

/// Serialized form of a [`DdcfKey`]: every payload (correction words, final word, beta2 share)
/// goes into `payloads`, `ceil(payload_bits / 8)` little-endian bytes per u128 component.
#[derive(Serialize, Deserialize)]
struct WireKey {
    domain_bits: u32,
    payload_bits: u32,
    dcf: Option<WireDcf>,
    payloads: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct WireDcf {
    key_idx: bool,
    root_seed: PrgSeed,
    /// Seed and control-bit corrections per level.
    cor_words: Vec<(PrgSeed, (bool, bool))>,
}

fn payload_bytes(payload_bits: u32) -> usize {
    (payload_bits as usize + 7) / 8
}

impl<T: Payload> Serialize for DdcfKey<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut limbs = Vec::new();
        if let Some(dcf) = &self.dcf {
            dcf.cor_words.iter().for_each(|cw| cw.word.write_limbs(&mut limbs));
            dcf.word.write_limbs(&mut limbs);
        }
        self.beta2_share.write_limbs(&mut limbs);
        let nb = payload_bytes(self.payload_bits);
        let payloads = limbs.iter().flat_map(|v| v.to_le_bytes()[..nb].to_vec()).collect();
        WireKey {
            domain_bits: self.domain_bits,
            payload_bits: self.payload_bits,
            dcf: self.dcf.as_ref().map(|d| WireDcf {
                key_idx: d.key_idx,
                root_seed: d.root_seed.clone(),
                cor_words: d.cor_words.iter().map(|cw| (cw.seed.clone(), cw.bits)).collect(),
            }),
            payloads,
        }
        .serialize(s)
    }
}

impl<'de, T: Payload> Deserialize<'de> for DdcfKey<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = WireKey::deserialize(d)?;
        if w.domain_bits > 128 || !(1..=128).contains(&w.payload_bits) {
            return Err(D::Error::custom(format!(
                "DDCF key: invalid widths (domain {} bits, payload {} bits)",
                w.domain_bits, w.payload_bits
            )));
        }
        let levels = match &w.dcf {
            Some(dcf) if dcf.cor_words.len() == w.domain_bits as usize && w.domain_bits > 0 => dcf.cor_words.len(),
            None if w.domain_bits == 0 => 0,
            _ => return Err(D::Error::custom("DDCF key: correction words do not match the domain width")),
        };
        let words = if w.dcf.is_some() { levels + 1 } else { 0 } + 1;
        let nb = payload_bytes(w.payload_bits);
        if w.payloads.len() != words * T::LIMBS * nb {
            return Err(D::Error::custom("DDCF key: wrong payload length"));
        }
        let mask = low_mask(w.payload_bits);
        let mut limbs = Vec::with_capacity(words * T::LIMBS);
        for chunk in w.payloads.chunks(nb) {
            let mut buf = [0u8; 16];
            buf[..nb].copy_from_slice(chunk);
            let v = u128::from_le_bytes(buf);
            if v & !mask != 0 {
                return Err(D::Error::custom("DDCF key: payload exceeds its bit width"));
            }
            limbs.push(v);
        }
        let mut payloads = limbs.chunks(T::LIMBS).map(T::read_limbs);
        let mut next = || payloads.next().expect("length checked above");
        let dcf = w.dcf.map(|dcf| DCFKey {
            key_idx: dcf.key_idx,
            root_seed: dcf.root_seed,
            cor_words: dcf.cor_words.into_iter().map(|(seed, bits)| CorWord { seed, bits, word: next() }).collect(),
            word: next(),
        });
        Ok(DdcfKey { domain_bits: w.domain_bits, payload_bits: w.payload_bits, dcf, beta2_share: next() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn reconstruct(k0: &DdcfKey<Z2>, k1: &DdcfKey<Z2>, x: u128) -> Z2 {
        let mut y = k0.eval(x);
        y.add(&k1.eval(x));
        y.map(|v| low_bits(v, k0.payload_bits))
    }

    fn roundtrip<T: Payload>(k: &DdcfKey<T>) -> DdcfKey<T> {
        bincode::deserialize(&bincode::serialize(k).unwrap()).unwrap()
    }

    #[test]
    fn exhaustive_small_domains() {
        let (b1, b2) = (Z2(7, u128::MAX), Z2(3, 1 << 127));
        for p in [1, 3, 8, 64, 128] {
            let want_of = |b: Z2| b.map(|v| low_bits(v, p));
            for m in 0..=5u32 {
                for alpha in 0..(1u128 << m) {
                    let (k0, k1) = DdcfKey::gen(m, p, alpha, b1, b2);
                    let (r0, r1) = (roundtrip(&k0), roundtrip(&k1));
                    for x in 0..(1u128 << m) {
                        let want = want_of(if x < alpha { b1 } else { b2 });
                        assert_eq!(reconstruct(&k0, &k1, x), want, "p={p} m={m} alpha={alpha} x={x}");
                        assert_eq!(reconstruct(&r0, &r1, x), want, "serialized, p={p} m={m} alpha={alpha} x={x}");
                    }
                }
            }
        }
    }

    #[test]
    fn full_128_bit_domain() {
        let mut rng = rand::thread_rng();
        let (b1, b2) = (Z2(rng.gen(), rng.gen()), Z2(rng.gen(), rng.gen()));
        for _ in 0..20 {
            let alpha: u128 = rng.gen();
            let (k0, k1) = DdcfKey::gen(128, 128, alpha, b1, b2);
            for x in [0, alpha.wrapping_sub(1), alpha, alpha.wrapping_add(1), u128::MAX, rng.gen()] {
                let want = if x < alpha { b1 } else { b2 };
                assert_eq!(reconstruct(&k0, &k1, x), want);
            }
        }
    }

    #[test]
    fn key_size_scales_with_payload_width() {
        // Fixed part: widths 4 + 4, Option tag 1, key_idx 1, root seed 16, two Vec lengths 8 + 8.
        // Per level: seed 16 + control bits 2 + payload. Plus final word and beta2 share.
        let size = |m: u32, p: u32| {
            let (k, _) = DdcfKey::gen(m, p, 5, Z2(1, 1), Z2(0, 0));
            bincode::serialize(&k).unwrap().len()
        };
        for (m, p) in [(31, 32), (15, 16), (7, 8), (63, 64), (127, 128), (10, 1), (10, 24)] {
            let pb = 2 * payload_bytes(p);
            assert_eq!(size(m, p), 42 + m as usize * (18 + pb) + 2 * pb, "m={m} p={p}");
        }
        let (k, _) = DdcfKey::gen(0, 16, 0, Z(1), Z(0));
        assert_eq!(bincode::serialize(&k).unwrap().len(), 4 + 4 + 1 + 8 + 2);
    }

    #[test]
    fn rejects_malformed_keys() {
        let (k, _) = DdcfKey::gen(8, 12, 100, Z(3), Z(1));
        let good = bincode::serialize(&k).unwrap();
        let bad = |bytes: &[u8]| bincode::deserialize::<DdcfKey<Z>>(bytes).is_err();
        assert!(!bad(&good));
        // payload_bits = 0 and 129.
        for pb in [0u32, 129] {
            let mut b = good.clone();
            b[4..8].copy_from_slice(&pb.to_le_bytes());
            assert!(bad(&b), "payload_bits {pb}");
        }
        // domain_bits no longer matches the number of correction words.
        let mut b = good.clone();
        b[0..4].copy_from_slice(&9u32.to_le_bytes());
        assert!(bad(&b));
        // A payload with bits above its 12-bit width (high byte of the last payload).
        let mut b = good.clone();
        *b.last_mut().unwrap() |= 0x10;
        assert!(bad(&b));
        // Truncated payloads.
        assert!(bad(&good[..good.len() - 1]));
    }
}
