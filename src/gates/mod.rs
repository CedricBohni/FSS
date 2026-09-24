//! FSS gates and the dealer.
//!
//! Every gate is an *offset* gate: for masks `r_in`, `r_out` chosen by the dealer, party `b`
//! evaluates its key on the public masked input `x_hat = x + r_in` and gets an additive share
//! of `g(x) + r_out`.
//!
//! [`deal`] additionally hands each party additive shares of `r_in` and `r_out`, so that the
//! online protocol ([`crate::online`]) can take plain additive shares of `x` in and give plain
//! additive shares of `g(x)` out.

pub mod ars;
pub mod lt0;

use crate::ring::Ring;
use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateKind {
    /// `1{x < 0}` via the MSB construction (Fig. 8).
    Lt0,
    /// `x >>_A shift` (Fig. 7).
    Ars { shift: u32 },
}

impl GateKind {
    pub fn parse(name: &str, shift: Option<u32>) -> Result<GateKind, Error> {
        match (name, shift) {
            ("lt0", _) => Ok(GateKind::Lt0),
            ("ars", Some(shift)) => Ok(GateKind::Ars { shift }),
            ("ars", None) => Err(Error::new("gate `ars` needs --shift")),
            _ => Err(Error::new(format!("unknown gate `{name}` (expected lt0 or ars)"))),
        }
    }

    /// Plaintext reference: what the gate computes on the ring element `x`.
    pub fn reference(&self, ring: Ring, x: u128) -> u128 {
        match self {
            GateKind::Lt0 => ring.msb(x) as u128,
            GateKind::Ars { shift } => ring.from_signed(ring.to_signed(x) >> shift),
        }
    }
}

impl std::fmt::Display for GateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GateKind::Lt0 => write!(f, "lt0"),
            GateKind::Ars { shift } => write!(f, "ars(s={shift})"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GateKey {
    Lt0(lt0::Lt0Key),
    Ars(ars::ArsKey),
}

impl GateKey {
    pub fn gen(kind: GateKind, ring: Ring, r_in: u128, r_out: u128) -> Result<(GateKey, GateKey), Error> {
        Ok(match kind {
            GateKind::Lt0 => {
                let (k0, k1) = lt0::gen(ring, r_in, r_out);
                (GateKey::Lt0(k0), GateKey::Lt0(k1))
            }
            GateKind::Ars { shift } => {
                let (k0, k1) = ars::gen(ring, shift, r_in, r_out)?;
                (GateKey::Ars(k0), GateKey::Ars(k1))
            }
        })
    }

    /// This party's share of `g(x_hat - r_in) + r_out`.
    pub fn eval(&self, x_hat: u128) -> u128 {
        match self {
            GateKey::Lt0(k) => k.eval(x_hat),
            GateKey::Ars(k) => k.eval(x_hat),
        }
    }

    pub fn kind(&self) -> GateKind {
        match self {
            GateKey::Lt0(_) => GateKind::Lt0,
            GateKey::Ars(k) => GateKind::Ars { shift: k.shift() },
        }
    }

    pub fn ring(&self) -> Ring {
        match self {
            GateKey::Lt0(k) => k.ring(),
            GateKey::Ars(k) => k.ring(),
        }
    }

    pub fn party(&self) -> u8 {
        match self {
            GateKey::Lt0(k) => k.party(),
            GateKey::Ars(k) => k.party(),
        }
    }
}

/// Everything one party needs from the dealer for one gate instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartyKey {
    pub gate: GateKey,
    pub r_in_share: u128,
    pub r_out_share: u128,
}

/// The dealer: fresh masks and keys for `count` independent gate instances.
/// Each key may be used for exactly one input; reusing it leaks `x - x'`.
pub fn deal(kind: GateKind, ring: Ring, count: usize) -> Result<(Vec<PartyKey>, Vec<PartyKey>), Error> {
    let mut keys0 = Vec::with_capacity(count);
    let mut keys1 = Vec::with_capacity(count);
    for _ in 0..count {
        let (r_in, r_out) = (ring.random(), ring.random());
        let (g0, g1) = GateKey::gen(kind, ring, r_in, r_out)?;
        let (i0, i1) = ring.share(r_in);
        let (o0, o1) = ring.share(r_out);
        keys0.push(PartyKey { gate: g0, r_in_share: i0, r_out_share: o0 });
        keys1.push(PartyKey { gate: g1, r_in_share: i1, r_out_share: o1 });
    }
    Ok((keys0, keys1))
}
