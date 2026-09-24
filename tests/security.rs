//! Regression test for the vendored libfss PRG fix (vendor/libfss/PATCHES.md).
//!
//! Unpatched libfss writes `alpha_i` into the right control-correction bit of level i of every
//! DCF key. With the fix, those bits must be uncorrelated with alpha.

use fss_gates::ddcf::{DdcfKey, Z};

const BITS: u32 = 16;

/// Bincode layout of `DdcfKey<Z>`: domain_bits u32 | Option tag u8 | DCFKey { key_idx u8,
/// root_seed [u8; 16], cor_words: len u64 + BITS x (seed [u8; 16], bits (u8, u8), word u128),
/// word u128 } | beta2_share u128.
fn right_cw_bits(key: &DdcfKey<Z>) -> Vec<bool> {
    let bytes = bincode::serialize(key).unwrap();
    let cw_start = 4 + 1 + 1 + 16 + 8;
    let cw_len = 16 + 2 + 16;
    assert_eq!(bytes.len(), cw_start + BITS as usize * cw_len + 16 + 16, "unexpected key layout");
    (0..BITS as usize)
        .map(|i| {
            let b = bytes[cw_start + i * cw_len + 17];
            assert!(b <= 1, "unexpected key layout");
            b == 1
        })
        .collect()
}

#[test]
fn dcf_keys_do_not_reveal_alpha() {
    let (mut agree, mut total) = (0, 0);
    for _ in 0..300 {
        let alpha: u128 = rand::random::<u16>() as u128;
        let (k0, _) = DdcfKey::gen(BITS, alpha, Z(1), Z(0));
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
