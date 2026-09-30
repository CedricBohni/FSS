# Vendored libfss

Source: https://github.com/nann-cheng/FSS-KRE, directory `libfss/`, commit
`eb79d1beec569c790826898558fd7f6feaa5949a`.

The upstream repository has no license file. Check with its authors before redistributing
this copy.

## Changes

1. **Security fix in `src/prg.rs`** (`PrgSeed::expand_dir` and `PrgSeed::long_expand`).
   Upstream cleared the two low bits of the seed (`key_short[0] &= 0xFC`) and then read the
   control bits `t_L`, `t_R` from those cleared bits, so they were always `(true, true)`.
   The DCF correction bits are `t_CW^L = t0^L ^ t1^L ^ alpha_i ^ 1` and
   `t_CW^R = t0^R ^ t1^R ^ alpha_i`, so every key then stored `alpha` in the clear. The fix
   reads the control bits from the unmasked seed (`self.key[0]`), as the Poplar code this
   PRG derives from does. `tests/security.rs` in the parent crate checks that keys no longer
   reveal `alpha`.
2. `#![allow(warnings)]` at the top of `src/lib.rs` to silence upstream lint noise.
3. **Generic ring width.** Upstream `RingElm` was hard-wired to Z_{2^32} (a `u32`). It is now
   `RingElm<const BITS: u32 = 32>`, the ring Z_{2^BITS} for any `1 <= BITS <= 128`, stored in a
   `u128` and kept reduced. Widths outside that range fail to compile. `RingElm` with no
   parameter is still the 32-bit ring.
   - Serde encodes an element as the smallest of `u8`/`u16`/`u32`/`u64`/`u128` that holds
     `BITS` bits (identical to upstream for `BITS = 32`); deserialization rejects values that
     do not fit. `to_u8_vec`/`From<Vec<u8>>` use `ceil(BITS/8)` big-endian bytes.
   - New helpers: `RingElm::{new, max, value, to_u64, to_u128, to_bits_BE, from_bits_BE,
     from_u8_slice}`, `From<u8|u16|u64|u128|usize>`, and `u128_to_bits_BE`/`bits_to_u128_BE`
     in `lib.rs`.
   - `ICKey`, `ICCKey`, `CondEvalKey`, `BeaverTuple` and `QElmMatrix` take the same `BITS`
     parameter (default 32). The IC gates build their DCF over `BITS` input bits instead of 32,
     and the Beaver messages are `2 * ceil(BITS/8)` bytes instead of 8.
   - Upstream tests that relied on the old concrete type now name `RingElm::<32>`; new tests
     cover the ring, IC/ICC (inside and outside the interval), CondEval and Beaver
     multiplication at several widths up to 128.
