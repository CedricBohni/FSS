//! Dual distributed comparison function (DDCF) on top of the libfss DCF.
//!
//! `f(x) = beta1` if `x < alpha`, else `beta2`, for `x, alpha` in `{0,1}^m` (Boyle et al. 2021,
//! Sec. 3). It is a DCF with payload `beta1 - beta2` plus additive shares of `beta2`.
//!
//! Payloads live in Z_{2^128} (`Z`) or Z_{2^128}^2 (`Z2`). Gates reduce the outputs mod 2^n,
//! which is a ring homomorphism, so one payload type serves every ring width.

use crate::ring::{low_bits, to_bits_msb_first};
use fss::dcf::DCFKey;
use fss::prg::FromRng;
use fss::Group;
use serde::{Deserialize, Serialize};

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

pub trait Payload: FromRng + Group + Clone + Copy + std::fmt::Debug {}
impl Payload for Z {}
impl Payload for Z2 {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DdcfKey<T> {
    domain_bits: u32,
    /// `None` when `domain_bits == 0`: the only input is the empty string, and `x < alpha` never holds.
    dcf: Option<DCFKey<T>>,
    beta2_share: T,
}

impl<T: Payload> DdcfKey<T> {
    /// Keys for `f(x) = beta1 if x < alpha else beta2` on `domain_bits`-bit inputs.
    /// `alpha` is reduced mod `2^domain_bits`.
    pub fn gen(domain_bits: u32, alpha: u128, beta1: T, beta2: T) -> (Self, Self) {
        assert!(domain_bits <= 128);
        let (dcf0, dcf1) = if domain_bits == 0 {
            (None, None)
        } else {
            let mut diff = beta1;
            diff.sub(&beta2);
            let alpha_bits = to_bits_msb_first(low_bits(alpha, domain_bits), domain_bits);
            let (k0, k1) = DCFKey::gen(&alpha_bits, &diff);
            (Some(k0), Some(k1))
        };
        let mut s0 = T::zero();
        s0.randomize();
        let mut s1 = beta2;
        s1.sub(&s0);
        (
            DdcfKey { domain_bits, dcf: dcf0, beta2_share: s0 },
            DdcfKey { domain_bits, dcf: dcf1, beta2_share: s1 },
        )
    }

    /// This party's additive share of `f(x)`; `x` is reduced mod `2^domain_bits`.
    pub fn eval(&self, x: u128) -> T {
        let mut out = self.beta2_share;
        if let Some(dcf) = &self.dcf {
            let x_bits = to_bits_msb_first(low_bits(x, self.domain_bits), self.domain_bits);
            out.add(&dcf.eval(&x_bits));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn reconstruct(k0: &DdcfKey<Z2>, k1: &DdcfKey<Z2>, x: u128) -> Z2 {
        let mut y = k0.eval(x);
        y.add(&k1.eval(x));
        y
    }

    #[test]
    fn exhaustive_small_domains() {
        let (b1, b2) = (Z2(7, u128::MAX), Z2(3, 1 << 127));
        for m in 0..=5u32 {
            for alpha in 0..(1u128 << m) {
                let (k0, k1) = DdcfKey::gen(m, alpha, b1, b2);
                for x in 0..(1u128 << m) {
                    let want = if x < alpha { b1 } else { b2 };
                    assert_eq!(reconstruct(&k0, &k1, x), want, "m={m} alpha={alpha} x={x}");
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
            let (k0, k1) = DdcfKey::gen(128, alpha, b1, b2);
            for x in [0, alpha.wrapping_sub(1), alpha, alpha.wrapping_add(1), u128::MAX, rng.gen()] {
                let want = if x < alpha { b1 } else { b2 };
                assert_eq!(reconstruct(&k0, &k1, x), want);
            }
        }
    }
}
