//! The online phase between the two computing parties, over any [`Channel`].
//!
//! For each gate instance, party `b` holding the share `x_b`:
//! 1. sends `x_b + r_in_b` and opens `x_hat = x + r_in` (one round, batched over all instances);
//! 2. evaluates its FSS key on `x_hat`, getting a share of `g(x) + r_out`;
//! 3. subtracts `r_out_b`, which leaves an additive share of `g(x)`.

use crate::gates::PartyKey;
use crate::transport::Channel;
use crate::Error;

/// Run the gates on additively shared inputs. Returns this party's shares of the outputs.
pub fn eval_shared<C: Channel>(keys: &[PartyKey], x_shares: &[u128], ch: &mut C) -> Result<Vec<u128>, Error> {
    if keys.len() != x_shares.len() {
        return Err(Error::new(format!("{} keys but {} input shares", keys.len(), x_shares.len())));
    }
    let masked: Vec<u128> = keys
        .iter()
        .zip(x_shares)
        .map(|(k, &x)| k.gate.ring().add(x, k.r_in_share))
        .collect();
    let x_hats = open(keys, &masked, ch)?;
    Ok(keys
        .iter()
        .zip(x_hats)
        .map(|(k, x_hat)| k.gate.ring().sub(k.gate.eval(x_hat), k.r_out_share))
        .collect())
}

/// Reveal additively shared values to both parties.
pub fn reveal<C: Channel>(keys: &[PartyKey], shares: &[u128], ch: &mut C) -> Result<Vec<u128>, Error> {
    open(keys, shares, ch)
}

fn open<C: Channel>(keys: &[PartyKey], mine: &[u128], ch: &mut C) -> Result<Vec<u128>, Error> {
    let theirs = ch.exchange(mine)?;
    if theirs.len() != mine.len() {
        return Err(Error::new(format!("peer sent {} values, expected {}", theirs.len(), mine.len())));
    }
    Ok(keys
        .iter()
        .zip(mine.iter().zip(theirs))
        .map(|(k, (&a, b))| k.gate.ring().add(a, b))
        .collect())
}
