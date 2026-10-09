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

use crate::ring::{Ring, U256};
use crate::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    pub fn reference(&self, ring: Ring, x: U256) -> U256 {
        match self {
            GateKind::Lt0 => U256::from(ring.msb(x)),
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
    pub fn gen(kind: GateKind, ring: Ring, r_in: U256, r_out: U256) -> Result<(GateKey, GateKey), Error> {
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
    pub fn eval(&self, x_hat: U256) -> U256 {
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
#[derive(Clone, Debug)]
pub struct PartyKey {
    pub gate: GateKey,
    pub r_in_share: U256,
    pub r_out_share: U256,
    /// Public, random identifier of this gate instance, the same in both parties' keys. The
    /// online phase compares the IDs to catch keys from different deals or in a different order.
    pub id: u128,
}

/// Serialized form of a [`PartyKey`]: the mask shares in `ceil(n/8)` bytes each.
#[derive(Serialize, Deserialize)]
struct PartyWire<G> {
    gate: G,
    r_in_share: Vec<u8>,
    r_out_share: Vec<u8>,
    id: u128,
}

impl Serialize for PartyKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ring = self.gate.ring();
        PartyWire {
            gate: &self.gate,
            r_in_share: ring.to_bytes(self.r_in_share),
            r_out_share: ring.to_bytes(self.r_out_share),
            id: self.id,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for PartyKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = PartyWire::<GateKey>::deserialize(d)?;
        let ring = w.gate.ring();
        let r_in_share = ring.from_bytes(&w.r_in_share).map_err(D::Error::custom)?;
        let r_out_share = ring.from_bytes(&w.r_out_share).map_err(D::Error::custom)?;
        Ok(PartyKey { gate: w.gate, r_in_share, r_out_share, id: w.id })
    }
}

/// The dealer: fresh masks and keys for `count` independent gate instances.
/// Each key may be used for exactly one input; reusing it leaks `x - x'`.
pub fn deal(kind: GateKind, ring: Ring, count: usize) -> Result<(Vec<PartyKey>, Vec<PartyKey>), Error> {
    let mut keys0 = Vec::with_capacity(count);
    let mut keys1 = Vec::with_capacity(count);
    let first_id: u128 = rand::random();
    for i in 0..count {
        let id = first_id.wrapping_add(i as u128);
        let (r_in, r_out) = (ring.random(), ring.random());
        let (g0, g1) = GateKey::gen(kind, ring, r_in, r_out)?;
        let (i0, i1) = ring.share(r_in);
        let (o0, o1) = ring.share(r_out);
        keys0.push(PartyKey { gate: g0, r_in_share: i0, r_out_share: o0, id });
        keys1.push(PartyKey { gate: g1, r_in_share: i1, r_out_share: o1, id });
    }
    Ok((keys0, keys1))
}
