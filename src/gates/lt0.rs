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
use crate::ring::{bit, low_bits, low_mask, Ring};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lt0Key {
    party: u8,
    ring: Ring,
    ddcf: DdcfKey<Z>,
    r_share: u128,
}

pub fn gen(ring: Ring, r_in: u128, r_out: u128) -> (Lt0Key, Lt0Key) {
    let n = ring.bits();
    let y = ring.neg(r_in);
    let y_msb = bit(y, n - 1) as u128;
    let (d0, d1) = DdcfKey::gen(n - 1, n, low_bits(y, n - 1), Z(1 ^ y_msb), Z(y_msb));
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
    pub fn eval(&self, x_hat: u128) -> u128 {
        let ring = self.ring;
        let n = ring.bits();
        let b = self.party as u128;
        let x_hat = ring.reduce(x_hat);
        let x_msb = bit(x_hat, n - 1) as u128;
        // z = 2^{n-1} - x_hat[0,n-1) - 1, so that z < alpha  <=>  there is a carry into bit n-1.
        let z = low_mask(n - 1) - low_bits(x_hat, n - 1);
        let m = self.ddcf.eval(z).0;
        // Shares of x_msb XOR m = x_msb + m - 2 * x_msb * m (x_msb is public).
        let msb = ring.sub(ring.add(b * x_msb, m), ring.mul(2 * x_msb, m));
        ring.add(msb, self.r_share)
    }
}
