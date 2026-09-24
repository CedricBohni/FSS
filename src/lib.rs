//! Two-party FSS gates in the dealer model (1 dealer + 2 semi-honest parties), following
//! Boyle, Chandran, Gilboa, Gupta, Ishai, Kumar, Rathee: "Function Secret Sharing for
//! Mixed-Mode and Fixed-Point Secure Computation", Eurocrypt 2021.
//!
//! Gates, over Z_{2^n} for any 1 <= n <= 128 on signed (two's complement) values:
//! - [`gates::lt0`]: `1{x < 0}` (Fig. 8 specialised to comparison with 0),
//! - [`gates::ars`]: arithmetic right shift by a public `s` (Fig. 7).
//!
//! Roles:
//! - the dealer calls [`gates::deal`] and sends each party its `Vec<PartyKey>` (serde),
//! - each party runs [`online::eval_shared`] over a [`transport::Channel`].
//!
//! [`simulate`] runs dealer and both parties in-process, for tests.

pub mod ddcf;
pub mod fixed;
pub mod gates;
pub mod online;
pub mod ring;
pub mod transport;

pub use fixed::FixedPoint;
pub use gates::{deal, GateKind, PartyKey};
pub use ring::Ring;

#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Error {
        Error(msg.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(format!("I/O error: {e}"))
    }
}

impl From<bincode::Error> for Error {
    fn from(e: bincode::Error) -> Error {
        Error(format!("serialization error: {e}"))
    }
}

/// Run one gate on the ring element `x` with fresh keys: the dealer deals, `x` is split into
/// random additive shares, both parties run the online phase, and the output is reconstructed.
pub fn simulate(kind: GateKind, ring: Ring, x: u128) -> Result<u128, Error> {
    let (k0, k1) = deal(kind, ring, 1)?;
    let (k0, k1) = (&k0[0], &k1[0]);
    let (x0, x1) = ring.share(x);
    let x_hat = ring.add(ring.add(x0, k0.r_in_share), ring.add(x1, k1.r_in_share));
    let y0 = ring.sub(k0.gate.eval(x_hat), k0.r_out_share);
    let y1 = ring.sub(k1.gate.eval(x_hat), k1.r_out_share);
    Ok(ring.add(y0, y1))
}
