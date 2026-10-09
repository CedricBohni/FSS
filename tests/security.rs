//! Regression test for the vendored libfss PRG fix (vendor/libfss/PATCHES.md).
//!
//! Unpatched libfss writes `alpha_i` into the right control-correction bit of level i of every
//! DCF key. With the fix, those bits must be uncorrelated with alpha.

use fss_gates::ddcf::{DdcfKey, Z};
use fss_gates::U256;

const BITS: u32 = 16;

/// The right control-correction bit of every level of the key's DCF.
fn right_cw_bits(key: &DdcfKey<Z>) -> Vec<bool> {
    key.dcf().unwrap().cor_words.iter().map(|cw| cw.bits.1).collect()
}

#[test]
fn dcf_keys_do_not_reveal_alpha() {
    let (mut agree, mut total) = (0, 0);
    for _ in 0..300 {
        let alpha = rand::random::<u16>();
        let (k0, _) = DdcfKey::gen(BITS, BITS, U256::from(alpha), Z(U256::ONE), Z(U256::ZERO));
        for (i, cw) in right_cw_bits(&k0).into_iter().enumerate() {
            let alpha_bit = (alpha >> (BITS as usize - 1 - i)) & 1 == 1;
            agree += (cw == alpha_bit) as u32;
            total += 1;
        }
    }
    let rate = agree as f64 / total as f64;
    // Unpatched libfss gives exactly 1.0. Independent bits give 0.5 +- ~0.01 here.
    assert!((0.45..0.55).contains(&rate), "correction bits match alpha {:.1}% of the time", rate * 100.0);
}
