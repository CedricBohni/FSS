use crate::prg::{PrgSeed,FixedKeyPrgStream};
use super::{RingElm,BinElm,dcf::*};
use crate::Group;
use std::mem;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ICKey<const BITS: u32 = 32>{
    pub key_idx: bool,
    dcf_key: DCFKey<BinElm>,
    p: RingElm<BITS>,
    q: RingElm<BITS>,
    word: BinElm,
}

//TODO:Convert BinElm to a general type 
impl<const BITS: u32> ICKey<BITS>
{
    pub fn gen(alpha_bits: &[bool], p_bound:&RingElm<BITS>, q_bound:&RingElm<BITS>) -> (Self, Self) {
        let gamma_in = RingElm::<BITS>::from_bits_BE(alpha_bits);

        let mut gamma = gamma_in.clone();
        gamma.sub(&RingElm::<BITS>::one());

        let gamma_bits = gamma.to_bits_BE();

        let beta = BinElm::from(true);
        let (key0, key1) = DCFKey::gen(&gamma_bits, &beta);

        let mut q_prime = q_bound.clone();
        q_prime.add(&RingElm::<BITS>::one());
        

        let mut alpha_p = p_bound.clone();
        alpha_p.add(&gamma_in);

        let mut alpha_q = q_bound.clone();
        alpha_q.add(&gamma_in);

        let mut alpha_q_prime = alpha_q.clone();
        alpha_q_prime.add(&RingElm::<BITS>::one());

        let root_seed = PrgSeed::random();
        let mut stream = FixedKeyPrgStream::new();
        stream.set_key(&root_seed.key);
        let z_0_bits = stream.next_bits(1usize);
        let z_0 = BinElm::from( z_0_bits[0] );
        let mut z_1 = BinElm::zero();
        if alpha_p > alpha_q{
            z_1.add(&BinElm::one());
        }
        if &alpha_p > p_bound{
            z_1.sub(&BinElm::one());
        }
        if alpha_q_prime > q_prime{
            z_1.add(&BinElm::one());
        }
        if alpha_q == RingElm::<BITS>::max(){
            z_1.add(&BinElm::one());
        }
        z_1.sub(&z_0);

        (
            ICKey{
                key_idx: false,
                dcf_key: key0,
                p: p_bound.clone(),
                q: q_bound.clone(),
                word: z_0,
            },
            ICKey{
                key_idx: true,
                dcf_key: key1,
                p: p_bound.clone(),
                q: q_bound.clone(),
                word: z_1,
            }
        )
    }

    pub fn eval(&self, x:&RingElm<BITS>) -> BinElm {
        let mut q_prime = self.q.clone();
        q_prime.add(&RingElm::<BITS>::one());

        let mut x_p = x.clone();
        x_p.add(&RingElm::<BITS>::max());
        x_p.sub(&self.p);

        let mut x_q_prime = x.clone();
        x_q_prime.add(&RingElm::<BITS>::max());
        x_q_prime.sub(&q_prime);

        let mut output_word:BinElm = BinElm::zero();
        output_word.add(&self.word);

        let x_p_bits = x_p.to_bits_BE();

        let x_q_prime_bits = x_q_prime.to_bits_BE();
        let duplicate_dcf = self.dcf_key.clone();

        let s_p = self.dcf_key.eval(&x_p_bits);
        let s_q_prime = duplicate_dcf.eval(&x_q_prime_bits);
        output_word.add(&s_q_prime);
        output_word.sub(&s_p);

        if self.key_idx{
            if x>&self.p{
                output_word.add(&BinElm::one());
            }

            if x>&q_prime{
                output_word.sub(&BinElm::one());
            }
        }

        output_word
    }


    pub fn key_size(&self) -> usize {
        let mut keySize = 0usize;
        keySize += mem::size_of_val(&self.key_idx);
        keySize += mem::size_of_val(&self.dcf_key);
        keySize += mem::size_of_val(&self.word);
        keySize
    }

}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ICCKey<const BITS: u32 = 32>{
    pub key_idx: bool,
    dcf_key: DCFKey<RingElm<BITS>>,
    p: RingElm<BITS>,
    q: RingElm<BITS>,
    word: RingElm<BITS>,
}

impl<const BITS: u32> ICCKey<BITS>
{
    pub fn gen(alpha_bits: &[bool], p_bound:&RingElm<BITS>, q_bound:&RingElm<BITS>) -> (Self, Self) {
        let gamma_in = RingElm::<BITS>::from_bits_BE(alpha_bits);

        let mut gamma = gamma_in.clone();
        gamma.sub(&RingElm::<BITS>::one());

        let gamma_bits = gamma.to_bits_BE();

        let beta = RingElm::<BITS>::one();
        let (key0, key1) = DCFKey::gen(&gamma_bits, &beta);

        let mut q_prime = q_bound.clone();
        q_prime.add(&RingElm::<BITS>::one());
        

        let mut alpha_p = p_bound.clone();
        alpha_p.add(&gamma_in);

        let mut alpha_q = q_bound.clone();
        alpha_q.add(&gamma_in);

        let mut alpha_q_prime = alpha_q.clone();
        alpha_q_prime.add(&RingElm::<BITS>::one());

        let root_seed = PrgSeed::random();
        let mut stream = FixedKeyPrgStream::new();
        stream.set_key(&root_seed.key);
        let z_0_bits = stream.next_bits(1usize);
        let z_0 = RingElm::<BITS>::from_bits_BE(&z_0_bits);
        let mut z_1 = RingElm::<BITS>::zero();
        if alpha_p > alpha_q{
            z_1.add(&RingElm::<BITS>::one());
        }
        if &alpha_p > p_bound{
            z_1.sub(&RingElm::<BITS>::one());
        }
        if alpha_q_prime > q_prime{
            z_1.add(&RingElm::<BITS>::one());
        }
        if alpha_q == RingElm::<BITS>::max(){
            z_1.add(&RingElm::<BITS>::one());
        }
        z_1.sub(&z_0);

        (
            ICCKey{
                key_idx: false,
                dcf_key: key0,
                p: p_bound.clone(),
                q: q_bound.clone(),
                word: z_0,
            },
            ICCKey{
                key_idx: true,
                dcf_key: key1,
                p: p_bound.clone(),
                q: q_bound.clone(),
                word: z_1,
            }
        )
    }

    pub fn eval(&self, x:&RingElm<BITS>) -> RingElm<BITS> {
        let mut q_prime = self.q.clone();
        q_prime.add(&RingElm::<BITS>::one());

        let mut x_p = x.clone();
        x_p.add(&RingElm::<BITS>::max());
        x_p.sub(&self.p);

        let mut x_q_prime = x.clone();
        x_q_prime.add(&RingElm::<BITS>::max());
        x_q_prime.sub(&q_prime);

        let mut output_word: RingElm<BITS> = RingElm::<BITS>::zero();
        output_word.add(&self.word);

        let x_p_bits = x_p.to_bits_BE();

        let x_q_prime_bits = x_q_prime.to_bits_BE();
        let duplicate_dcf = self.dcf_key.clone();

        let s_p = self.dcf_key.eval(&x_p_bits);
        let s_q_prime = duplicate_dcf.eval(&x_q_prime_bits);
        output_word.add(&s_q_prime);
        output_word.sub(&s_p);

        if self.key_idx{
            if x>&self.p{
                output_word.add(&RingElm::<BITS>::one());
            }

            if x>&q_prime{
                output_word.sub(&RingElm::<BITS>::one());
            }
        }

        output_word
    }


    pub fn key_size(&self) -> usize {
        let mut keySize = 0usize;
        keySize += mem::size_of_val(&self.key_idx);
        keySize += mem::size_of_val(&self.dcf_key);
        keySize += mem::size_of_val(&self.word);
        keySize
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    // Keys are generated for the input mask alpha; the offset input x = alpha + d must give
    // 1{p <= d <= q} (as a ring element for ICC, as a bit for IC).
    fn check<const BITS: u32>() {
        let seed = PrgSeed::random();
        let mut stream = FixedKeyPrgStream::new();
        stream.set_key(&seed.key);
        let alpha_bits = stream.next_bits(BITS as usize);
        let alpha = RingElm::<BITS>::from_bits_BE(&alpha_bits);

        let p = RingElm::<BITS>::zero();
        let q = RingElm::<BITS>::new(RingElm::<BITS>::MASK >> 1);
        let (icc0, icc1) = ICCKey::gen(&alpha_bits, &p, &q);
        let (ic0, ic1) = ICKey::gen(&alpha_bits, &p, &q);

        let one = RingElm::<BITS>::one();
        for d in [p, p + one, q - one, q, q + one, RingElm::<BITS>::max(), RingElm::<BITS>::max() - one] {
            let x = alpha + d;
            let inside = p <= d && d <= q;

            let got = icc0.eval(&x) + icc1.eval(&x);
            let want = if inside { RingElm::<BITS>::one() } else { RingElm::<BITS>::zero() };
            assert_eq!(got, want, "ICC BITS={} d={:?}", BITS, d);

            let mut bit = ic0.eval(&x);
            bit.add(&ic1.eval(&x));
            assert_eq!(bit, BinElm::from(inside), "IC BITS={} d={:?}", BITS, d);
        }
    }

    #[test]
    fn evalCheck() {
        for _ in 0..5 {
            check::<8>();
            check::<32>();
            check::<33>();
            check::<64>();
            check::<100>();
            check::<128>();
        }
    }
}
