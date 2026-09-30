use crate::prg::PrgSeed;
use crate::prg::FixedKeyPrgStream;
use crate::{ring, Group};

use super::RingElm;
// use serde::ser::{Serialize, Serializer, SerializeStruct};
// use std::fmt;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BeaverTuple<const BITS: u32 = 32>{
    pub a: RingElm<BITS>,
    pub b: RingElm<BITS>,
    pub ab: RingElm<BITS>,
    pub delta_a: RingElm<BITS>,
    pub delta_b: RingElm<BITS>,
}

impl<const BITS: u32> BeaverTuple<BITS>{
    fn new(ra: RingElm<BITS>, rb: RingElm<BITS>, rc: RingElm<BITS>) -> Self{
        Self { a: ra, b: rb, ab: rc, delta_a:RingElm::zero(), delta_b:RingElm::zero(), }
    }

    pub fn genBeaver(beavertuples0: &mut Vec<Self>, beavertuples1: &mut Vec<Self>, seed: &PrgSeed, size:usize) {
        let mut stream = FixedKeyPrgStream::new();
        stream.set_key(&seed.key);

        for i in 0..size{
            let n = BITS as usize;
            let rd_bits = stream.next_bits(n*5);
            let a0 = RingElm::from_bits_BE(&rd_bits[..n]);
            let b0 = RingElm::from_bits_BE(&rd_bits[n..2*n]);

            let a1 = RingElm::from_bits_BE(&rd_bits[2*n..3*n]);
            let b1 = RingElm::from_bits_BE(&rd_bits[3*n..4*n]);

            let ab0 = RingElm::from_bits_BE(&rd_bits[4*n..5*n]);

            let mut a = RingElm::zero();
            a.add(&a0);
            a.add(&a1);

            let mut b = RingElm::zero();
            b.add(&b0);
            b.add(&b1);

            let mut ab = RingElm::one();
            ab.mul(&a);
            ab.mul(&b);

            ab.sub(&ab0);

            let beaver0 = Self{
                a: a0,
                b: b0,
                ab: ab0,
                delta_a:RingElm::zero(),
                delta_b:RingElm::zero(),
            };

            let beaver1 = Self{
                a: a1,
                b: b1,
                ab: ab,
                delta_a:RingElm::zero(),
                delta_b:RingElm::zero(),
            };
            beavertuples0.push(beaver0);
            beavertuples1.push(beaver1);
            
        }
    }
    
    pub fn beaver_mul0(&mut self, alpha: RingElm<BITS>, beta: RingElm<BITS>)-> Vec<u8>{
        self.delta_a = alpha - self.a;
        self.delta_b = beta - self.b;

        let mut container  = Vec::<u8>::new();
        container.append(&mut self.delta_a.to_u8_vec());
        container.append(&mut self.delta_b.to_u8_vec());
        container
    }

    /*The multiplication of [alpha] x [beta], the values of beaver_share are [a], [b], and [ab], d and e are the reconstructed values of alpha-a, beta-b*/
    pub fn beaver_mul1(&mut self, is_server: bool, otherHalf:&Vec<u8> ) -> RingElm<BITS>{
        let len = RingElm::<BITS>::BYTES;
        assert_eq!(otherHalf.len(), 2*len);
        self.delta_a.add(&RingElm::from_u8_slice(&otherHalf[..len]));
        self.delta_b.add(&RingElm::from_u8_slice(&otherHalf[len..]));
        let mut result= RingElm::zero();
        if is_server{
            result.add(&(self.delta_a*self.delta_b) );
        }
        result.add(&(self.delta_a*self.b) );
        result.add(&(self.delta_b*self.a) );
        result.add(& self.ab);
        result
    }

    pub fn mul_open(&mut self, alpha: RingElm<BITS>, beta: RingElm<BITS>) -> (RingElm<BITS>, RingElm<BITS>){
        self.delta_a = alpha - self.a;
        self.delta_b = beta - self.b;
        (self.delta_a, self.delta_b)
    }

    pub fn mul_compute(&mut self, is_server: bool, alpha: &RingElm<BITS>, beta: &RingElm<BITS>) -> RingElm<BITS>{
        self.delta_a = alpha.clone();
        self.delta_b = beta.clone();
        let mut result= RingElm::zero();
        if is_server{
            result.add(&(self.delta_a*self.delta_b) );
        }
        result.add(&(self.delta_a*self.b) );
        result.add(&(self.delta_b*self.a) );
        result.add(& self.ab);
        result
    }

}


#[cfg(test)]
mod test{
    use crate::{beavertuple::*, RingElm, Share};

    #[test]
    fn test_beaver_mul(){
        let a0 = RingElm::<32>::from(3u32);
        let a1 = RingElm::<32>::from(2u32);
        let b0 = RingElm::<32>::from(2u32);
        let b1 = RingElm::<32>::from(6u32);
        let c0 = RingElm::<32>::from(17u32);
        let c1 = RingElm::<32>::from(23u32);

        let alpha0 = RingElm::<32>::from(23u32);
        let alpha1 = RingElm::<32>::from(17u32);

        let beta0 = RingElm::<32>::from(14u32);
        let beta1 = RingElm::<32>::from(16u32);
        let mut beaver0 = BeaverTuple::new(a0, b0, c0);
        let mut beaver1 = BeaverTuple::new(a1, b1, c1);

        let msg0 = beaver0.mul_open(alpha0, beta0);
        //let msg0 = beaver0.beaver_mul0(alpha0, beta0);

        let msg1 = beaver1.mul_open(alpha1, beta1);
        //let msg1 = beaver1.beaver_mul0(alpha1, beta1);
        //println!("msg0 = {:?}", msg0);
        //println!("msg1 = {:?}", msg1);
        let msg = (msg0.0 + msg1.0, msg0.1 + msg1.1);
        //println!("msg1 = {:?}", msg);
        
        //let r0 = beaver0.mul_compute(false, msg.0, msg.1);
        let r0 = beaver0.mul_compute(false, &msg.0, &msg.1);
        let r1 = beaver1.mul_compute(true, &msg.0, &msg.1);

        println!("{:?}", r0);
        println!("{:?}", r1);
        let r_real = (alpha0 + alpha1) * (beta0 + beta1);
        assert_eq!(r0 + r1, r_real);
        //assert_eq!(r1, r_real);
    }

    fn check_wide<const BITS: u32>() {
        let (mut t0, mut t1) = (Vec::new(), Vec::new());
        BeaverTuple::<BITS>::genBeaver(&mut t0, &mut t1, &PrgSeed::random(), 4);
        for (b0, b1) in t0.iter_mut().zip(t1.iter_mut()) {
            let (x, y) = (RingElm::<BITS>::max(), RingElm::<BITS>::new(0x1234_5678_9abc_def0_1234_5678_9abc_def0));
            let (x0, x1) = x.share();
            let (y0, y1) = y.share();
            let m0 = b0.beaver_mul0(x0, y0);
            let m1 = b1.beaver_mul0(x1, y1);
            assert_eq!(m0.len(), 2 * RingElm::<BITS>::BYTES);
            let r0 = b0.beaver_mul1(false, &m1);
            let r1 = b1.beaver_mul1(true, &m0);
            assert_eq!(r0 + r1, x * y, "BITS={}", BITS);
        }
    }

    #[test]
    fn test_beaver_mul_widths(){
        check_wide::<1>();
        check_wide::<32>();
        check_wide::<61>();
        check_wide::<64>();
        check_wide::<128>();
    }
}

// /*The multiplication of [alpha] x [beta], the values of beaver_share are [a], [b], and [ab], d and e are the reconstructed values of alpha-a, beta-b*/
// fn beaver_mul(is_server: bool, beaver_share: &BeaverTuple, d: &RingElm, e: &RingElm) -> RingElm{
    
//     let mut r;
//     if is_server{
//         r = beaver_share.ab.clone();
//     }
//     else{
//         r = RingElm::zero();
//     }

//     let mut r0 = d.clone();
//     r0.mul(&e); //d*e

//     let mut r1 = d.clone();
//     r1.mul(&beaver_share.b); //d*[b]

//     let mut r2 = e.clone();
//     r2.mul(&beaver_share.a); //e*[a]
    
//     r.add(&r0);
//     r.add(&r1);
//     r.add(&r2);

//     r
// }

// impl Serialize for BeaverTuple {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         // 3 is the number of fields in the struct.
//         let mut state = serializer.serialize_struct("beavertuple", 3)?;
//         state.serialize_field("r", &self.a)?;
//         state.serialize_field("g", &self.b)?;
//         state.serialize_field("b", &self.ab)?;
//         state.end()
//     }
// }

// impl<'de> Deserialize<'de> for BeaverTuple {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where D: Deserializer<'de>,
//     {
//         enum Field { A, B, C }
//         //type Field = RingElm;
//         // This part could also be generated independently by:
//         //
//         //    #[derive(Deserialize)]
//         //    #[serde(field_identifier, rename_all = "lowercase")]
//         //    enum Field { Secs, Nanos }
//         impl<'de> Deserialize<'de> for Field {
//             fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
//             where D: Deserializer<'de>,
//             {
//                 struct FieldVisitor;

//                 impl<'de> Visitor<'de> for FieldVisitor {
//                     type Value = Field;

//                     fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                         formatter.write_str("`a` or `b` or `c`")
//                     }

//                     fn visit_str<E>(self, value: &str) -> Result<Field, E>
//                     where E: de::Error,
//                     {
//                         match value {
//                             "a" => Ok(Field::A),
//                             "b" => Ok(Field::B),
//                             "c" => Ok(Field::C),
//                             _ => Err(de::Error::unknown_field(value, FIELDS)),
//                         }
//                     }
//                 }

//                 deserializer.deserialize_identifier(FieldVisitor)
//             }
//         }

//         struct BeaverVisitor;

//         impl<'de> Visitor<'de> for BeaverVisitor {
//             type Value = BeaverTuple;

//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("struct BeaverTuple")
//             }

//             fn visit_seq<V>(self, mut seq: V) -> Result<BeaverTuple, V::Error>
//             where V: SeqAccess<'de>,
//             {
//                 let a = seq.next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(0, &self))?;
//                 let b = seq.next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(1, &self))?;
//                 let c = seq.next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(2, &self))?;
//                 Ok(BeaverTuple::new(a, b, c))
//             }

//             fn visit_map<V>(self, mut map: V) -> Result<BeaverTuple, V::Error>
//             where
//                 V: MapAccess<'de>,
//             {
//                 let mut a = None;
//                 let mut b = None;
//                 let mut c = None;
//                 while let Some(key) = map.next_key()? {
//                     match key {
//                         Field::A => {
//                             if a.is_some() {
//                                 return Err(de::Error::duplicate_field("secs"));
//                             }
//                             a = Some(map.next_value()?);
//                         }
//                         Field::B => {
//                             if b.is_some() {
//                                 return Err(de::Error::duplicate_field("nanos"));
//                             }
//                             b = Some(map.next_value()?);
//                         }
//                         Field::C => {
//                             if c.is_some() {
//                                 return Err(de::Error::duplicate_field("nanos"));
//                             }
//                             c = Some(map.next_value()?);
//                         }
//                     }
//                 }
//                 let a = a.ok_or_else(|| de::Error::missing_field("a"))?;
//                 let b = b.ok_or_else(|| de::Error::missing_field("b"))?;
//                 let c = c.ok_or_else(|| de::Error::missing_field("c"))?;
//                 Ok(BeaverTuple::new(a, b, c))
//             }
//         }

//         const FIELDS: &'static [&'static str] = &["a", "b", "c"];
//         deserializer.deserialize_struct("BeaverTuple", FIELDS, BeaverVisitor)
//     }
// }


