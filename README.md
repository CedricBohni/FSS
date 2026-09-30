# fss-gates

Two-party FSS gates in the dealer model (1 dealer + 2 semi-honest parties), after Boyle et al.,
*Function Secret Sharing for Mixed-Mode and Fixed-Point Secure Computation* (Eurocrypt 2021).
Works over Z_{2^n} for any 1 ≤ n ≤ 128, on signed (two's complement) fixed-point values with any
number of fractional bits.

| Gate | Computes | Paper | Key material | Online |
|---|---|---|---|---|
| `lt0` | `1{x < 0}` | Fig. 8 (signed comparison with 0) | 1 DDCF on n−1 bits | 1 DCF eval |
| `ars` | `x >>_A s` = `floor(x / 2^s)`, exact | Fig. 7 | DCF on s bits + DDCF on n−1 bits | 2 DCF evals |

All outputs are additive shares in Z_{2^n}. Every gate needs one round to open the masked input.

## Your own numbers

```sh
cargo run --release -- check --bits 16 --frac 8 -- -3.25 7.5 raw:-32768
cargo run --release -- check --bits 128 --frac 64 --shift 64 --trials 100 -- -123456.789
```

A value is either a decimal, encoded as `round(v · 2^frac)`, or `raw:<int>`, the signed ring
integer itself. `--shift` defaults to `--frac`. Every trial uses fresh keys and random shares,
and the result is compared with the plaintext computation. The command exits non-zero if any
trial fails.

To keep cases as regression tests, add lines to [tests/my_cases.txt](tests/my_cases.txt)
(`bits frac shift value`) and run `cargo test --test my_cases`.

## Distributed run (separate terminals)

```sh
B=target/release/fss-gates
$B deal  --gate ars --bits 32 --shift 12 --count 2 --out keys      # dealer
$B share --bits 32 --frac 12 -- -1234.5678 3.75                    # input owner: prints both share lists
# terminal 1
$B party --id 0 --key keys/party0.key --input <shares0> --listen 127.0.0.1:7000 --reveal --frac 12
# terminal 2
$B party --id 1 --key keys/party1.key --input <shares1> --connect 127.0.0.1:7000 --reveal --frac 12
```

Without `--reveal`, each party prints only its output shares. Use this when a later gate
consumes the output.

## Library use

```rust
use fss_gates::{deal, GateKind, Ring, online::eval_shared, transport::TcpChannel};

let ring = Ring::new(64)?;
let (keys0, keys1) = deal(GateKind::Ars { shift: 16 }, ring, 1000)?;   // dealer; keys are serde types
// party b, holding additive shares x_b of its inputs:
let y_b = eval_shared(&keys_b, &x_b, &mut channel)?;                  // shares of the outputs
```

`gates::{lt0, ars}` also expose the raw offset gates, `Gen(r_in, r_out)` and
`Eval(b, k_b, x̂)`, for use in your own protocol. Every key is single-use.

## Design notes

- **libfss** (from [FSS-KRE](https://github.com/nann-cheng/FSS-KRE), commit `eb79d1b`) provides the
  DCF. It is vendored in `vendor/libfss` because it needed a security fix: its PRG returned
  constant control bits, so **every DCF key contained α in plaintext**. For these gates, α is
  derived from the input mask, so each party could have recovered the other inputs. See
  [vendor/libfss/PATCHES.md](vendor/libfss/PATCHES.md). `tests/security.rs` guards against a
  regression.
- The vendored libfss's `RingElm<BITS>` (and its IC gate) works over Z_{2^BITS} for any
  compile-time `BITS` in 1..=128 (see PATCHES.md). The gates here take n at runtime, so their
  DCF payloads are Z_{2^128} (`ddcf::Z`, `ddcf::Z2`) and are reduced mod 2^n afterwards.
  Reduction mod 2^n is a ring homomorphism, so one payload type serves every n. The cost: each
  correction word stores 16 bytes even when n is small.
- Alternatives considered: [`fss-rs`/`dcf`](https://github.com/myl7/fss) (myl7) has a DCF with
  byte-granular input domains. It would work, but it does not fit arbitrary bit widths as well
  as libfss's bit-vector DCF.

## Tests

`cargo test` covers:
- exhaustive checks for n = 1..8 (every input, every shift, fresh masks),
- edge values and random values for every n up to 128,
- the online protocol over in-process channels and TCP,
- key serialization,
- the libfss security regression,
- `tests/my_cases.txt`.

The tests were checked against deliberately broken gates to confirm that they fail.
