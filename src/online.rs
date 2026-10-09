//! The online phase between the two computing parties, over any [`Channel`].
//!
//! For each gate instance, party `b` holding the share `x_b`:
//! 1. sends `x_b + r_in_b` and opens `x_hat = x + r_in` (one round, batched over all instances);
//! 2. evaluates its FSS key on `x_hat`, getting a share of `g(x) + r_out`;
//! 3. subtracts `r_out_b`, which leaves an additive share of `g(x)`.
//!
//! Step 1 also carries a check that the two parties hold matching keys: a digest of the gate
//! instance IDs (public, from the dealer) and the sender's party number, as 17 one-byte values
//! so they do not widen the message. Keys from different deals, in a different order, or two
//! copies of the same party's keys are rejected before any output is computed.

use crate::gates::PartyKey;
use crate::ring::U256;
use crate::transport::Channel;
use crate::Error;

/// Run the gates on additively shared inputs. Returns this party's shares of the outputs.
pub fn eval_shared<C: Channel>(keys: &[PartyKey], x_shares: &[U256], ch: &mut C) -> Result<Vec<U256>, Error> {
    if keys.len() != x_shares.len() {
        return Err(Error::new(format!("{} keys but {} input shares", keys.len(), x_shares.len())));
    }
    let Some(first) = keys.first() else {
        return Ok(Vec::new());
    };
    let party = first.gate.party();
    if keys.iter().any(|k| k.gate.party() != party) {
        return Err(Error::new("the keys mix party 0 and party 1 keys"));
    }
    let mut msg: Vec<U256> = keys
        .iter()
        .zip(x_shares)
        .map(|(k, &x)| k.gate.ring().add(x, k.r_in_share))
        .collect();
    let digest = key_digest(keys);
    msg.extend(digest.to_le_bytes().map(U256::from));
    msg.push(U256::from(party));

    let mut theirs = ch.exchange(&msg)?;
    if theirs.len() != msg.len() {
        return Err(Error::new(format!(
            "peer sent {} values, expected {} (do both parties hold keys from the same deal?)",
            theirs.len(),
            msg.len()
        )));
    }
    let check = theirs.split_off(keys.len());
    let their_party = check[16];
    let their_digest = check[..16].iter().map(|&b| u8::try_from(b).ok()).collect::<Option<Vec<u8>>>();
    if their_party != U256::from(1 - party) {
        return Err(Error::new(format!("both parties hold party {party}'s keys")));
    }
    if their_digest.as_deref() != Some(&digest.to_le_bytes()[..]) {
        return Err(Error::new("the peer's keys do not match ours (different deal, or a different order)"));
    }
    let x_hats: Vec<U256> = keys.iter().zip(msg.iter().zip(theirs)).map(|(k, (&a, b))| k.gate.ring().add(a, b)).collect();
    Ok(keys
        .iter()
        .zip(x_hats)
        .map(|(k, x_hat)| k.gate.ring().sub(k.gate.eval(x_hat), k.r_out_share))
        .collect())
}

/// Reveal additively shared values to both parties.
pub fn reveal<C: Channel>(keys: &[PartyKey], shares: &[U256], ch: &mut C) -> Result<Vec<U256>, Error> {
    open(keys, shares, ch)
}

/// Order-sensitive digest of the instance IDs and ring widths. It only has to catch honest
/// mistakes (the parties are semi-honest), so a simple multiplicative hash is enough.
fn key_digest(keys: &[PartyKey]) -> u128 {
    keys.iter().fold(0x6c62272e07bb014262b821756295c58d, |h, k| {
        let h = (h ^ k.id).wrapping_mul(0x0000000001000000000000000000013b);
        (h ^ k.gate.ring().bits() as u128).wrapping_mul(0x0000000001000000000000000000013b)
    })
}

fn open<C: Channel>(keys: &[PartyKey], mine: &[U256], ch: &mut C) -> Result<Vec<U256>, Error> {
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
