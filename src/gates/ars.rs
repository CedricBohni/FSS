//! Arithmetic right shift `x >>_A s` for signed `x` in Z_{2^n} and public `0 <= s < n`
//! (Boyle et al. 2021, Fig. 7). The result is exact: `floor(x / 2^s)`.
//!
//! With `x = x_hat + y`, `y = -r_in` and `L(v) = v[0,n-1)`, Lemma 4 gives
//! `x >>_A s = (L(x_hat) >> s) + (L(y) >> s) + t_s - 2^{n-s-1} (t_{n-1} + MSB(x))`,
//! where `t_i = 1{x_hat mod 2^i + y mod 2^i >= 2^i}` is the carry out of the low i bits.
//! A DCF_s key yields `t_s`; a DDCF_{n-1} key with payload pairs yields both `t_{n-1}` and
//! `y[n-1] XOR t_{n-1}`, from which the parties get `MSB(x)` locally.
//!
//! Key: one DCF_s key, one DDCF_{n-1} key, one ring element. Online: two DCF evaluations.

use crate::ddcf::{DdcfKey, Z, Z2};
use crate::ring::{bit, low_bits, low_mask, Ring, U256};
use crate::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug)]
pub struct ArsKey {
    party: u8,
    ring: Ring,
    shift: u32,
    dcf_s: DdcfKey<Z>,
    ddcf_n1: DdcfKey<Z2>,
    r_share: U256,
}

pub fn gen(ring: Ring, shift: u32, r_in: U256, r_out: U256) -> Result<(ArsKey, ArsKey), Error> {
    let n = ring.bits();
    if shift >= n {
        return Err(Error::new(format!("shift must be in 0..{n} for a {n}-bit ring, got {shift}")));
    }
    let y = ring.neg(r_in);
    let y_msb = U256::from(bit(y, n - 1));
    let alpha_n1 = low_bits(y, n - 1);
    let (s0, s1) = DdcfKey::gen(shift, n, low_bits(y, shift), Z(U256::ONE), Z(U256::ZERO));
    let (d0, d1) = DdcfKey::gen(n - 1, n, alpha_n1, Z2(U256::ONE, y_msb ^ 1), Z2(U256::ZERO, y_msb));
    let (r0, r1) = ring.share(ring.add(r_out, alpha_n1 >> shift));
    Ok((
        ArsKey { party: 0, ring, shift, dcf_s: s0, ddcf_n1: d0, r_share: r0 },
        ArsKey { party: 1, ring, shift, dcf_s: s1, ddcf_n1: d1, r_share: r1 },
    ))
}

impl ArsKey {
    pub fn ring(&self) -> Ring {
        self.ring
    }

    pub fn party(&self) -> u8 {
        self.party
    }

    pub fn shift(&self) -> u32 {
        self.shift
    }

    /// This party's share of `((x_hat - r_in) >>_A s) + r_out`.
    pub fn eval(&self, x_hat: U256) -> U256 {
        let ring = self.ring;
        let n = ring.bits();
        let s = self.shift;
        let b = U256::from(self.party);
        let x_hat = ring.reduce(x_hat);
        let x_msb = U256::from(bit(x_hat, n - 1));
        let x_low = low_bits(x_hat, n - 1);

        let t_s = self.dcf_s.eval(low_mask(s) - low_bits(x_hat, s)).0;
        let Z2(t_n1, m) = self.ddcf_n1.eval(low_mask(n - 1) - x_low);
        let msb = ring.sub(ring.add(b * x_msb, m), ring.mul(x_msb << 1, m));

        let mut out = ring.add(b * (x_low >> s), self.r_share);
        out = ring.add(out, t_s);
        let scale = U256::ONE << (n - s - 1);
        ring.sub(out, ring.mul(scale, ring.add(t_n1, msb)))
    }
}

/// Serialized form: the ring element in `ceil(n/8)` bytes.
#[derive(Serialize, Deserialize)]
struct Wire<S, D> {
    party: u8,
    ring: Ring,
    shift: u32,
    dcf_s: S,
    ddcf_n1: D,
    r_share: Vec<u8>,
}

impl Serialize for ArsKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Wire {
            party: self.party,
            ring: self.ring,
            shift: self.shift,
            dcf_s: &self.dcf_s,
            ddcf_n1: &self.ddcf_n1,
            r_share: self.ring.to_bytes(self.r_share),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for ArsKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = Wire::<DdcfKey<Z>, DdcfKey<Z2>>::deserialize(d)?;
        if w.shift >= w.ring.bits() {
            return Err(D::Error::custom(format!("ars key: shift {} out of range for a {}-bit ring", w.shift, w.ring.bits())));
        }
        let r_share = w.ring.from_bytes(&w.r_share).map_err(D::Error::custom)?;
        Ok(ArsKey { party: w.party, ring: w.ring, shift: w.shift, dcf_s: w.dcf_s, ddcf_n1: w.ddcf_n1, r_share })
    }
}
