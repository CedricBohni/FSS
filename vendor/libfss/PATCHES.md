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
