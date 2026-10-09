//! Less-than-zero gate: `1{x < 0}` for signed `x` in Z_{2^n}, i.e. `MSB(x)`.
//!
//! This is the signed comparison gate of Boyle et al. 2021 (Fig. 8) with the second operand
//! fixed to 0. Viewing `x = x_hat + y` with `y = -r_in`, the MSB is
//! `x_hat[n-1] XOR y[n-1] XOR carry`, where `carry = 1{x_hat[0,n-1) + y[0,n-1) >= 2^{n-1}}`.
//! A DDCF on n-1 bits returns shares of `y[n-1] XOR carry`. Unlike the general comparison
//! gate, no precondition on `x` is needed: the result is exact for every `x`.
//!
//! Key: one DDCF_{n-1} key plus one ring element. Online: one DCF evaluation on n-1 bits.

use crate::ddcf::{DdcfKey, Z};
use crate::ring::{bit, low_bits, low_mask, Ring, U256};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug)]
pub struct Lt0Key {
    party: u8,
    ring: Ring,
    ddcf: DdcfKey<Z>,
    r_share: U256,
}

pub fn gen(ring: Ring, r_in: U256, r_out: U256) -> (Lt0Key, Lt0Key) {
    let n = ring.bits();
    let y = ring.neg(r_in);
    let y_msb = U256::from(bit(y, n - 1));
    let (d0, d1) = DdcfKey::gen(n - 1, n, low_bits(y, n - 1), Z(y_msb ^ 1), Z(y_msb));
    let (r0, r1) = ring.share(r_out);
    (
        Lt0Key { party: 0, ring, ddcf: d0, r_share: r0 },
        Lt0Key { party: 1, ring, ddcf: d1, r_share: r1 },
    )
}

impl Lt0Key {
    pub fn ring(&self) -> Ring {
        self.ring
    }

    pub fn party(&self) -> u8 {
        self.party
    }

    /// This party's share of `1{x_hat - r_in < 0} + r_out`.
    pub fn eval(&self, x_hat: U256) -> U256 {
        let ring = self.ring;
        let n = ring.bits();
        let b = U256::from(self.party);
        let x_hat = ring.reduce(x_hat);
        let x_msb = U256::from(bit(x_hat, n - 1));
        // z = 2^{n-1} - x_hat[0,n-1) - 1, so that z < alpha  <=>  there is a carry into bit n-1.
        let z = low_mask(n - 1) - low_bits(x_hat, n - 1);
        let m = self.ddcf.eval(z).0;
        // Shares of x_msb XOR m = x_msb + m - 2 * x_msb * m (x_msb is public).
        let msb = ring.sub(ring.add(b * x_msb, m), ring.mul(x_msb << 1, m));
        ring.add(msb, self.r_share)
    }
}

/// Serialized form: the ring element in `ceil(n/8)` bytes.
#[derive(Serialize, Deserialize)]
struct Wire<D> {
    party: u8,
    ring: Ring,
    ddcf: D,
    r_share: Vec<u8>,
}

impl Serialize for Lt0Key {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Wire { party: self.party, ring: self.ring, ddcf: &self.ddcf, r_share: self.ring.to_bytes(self.r_share) }
            .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Lt0Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = Wire::<DdcfKey<Z>>::deserialize(d)?;
        let r_share = w.ring.from_bytes(&w.r_share).map_err(D::Error::custom)?;
        Ok(Lt0Key { party: w.party, ring: w.ring, ddcf: w.ddcf, r_share })
    }
}
