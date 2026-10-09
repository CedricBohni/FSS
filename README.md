# fss-gates

Two-party secure computation gates built on function secret sharing (FSS), following Boyle et
al., *Function Secret Sharing for Mixed-Mode and Fixed-Point Secure Computation* (Eurocrypt
2021).

A **dealer** hands out keys. Two **parties** each hold a share of a secret input, and together
they compute a share of the result. Neither party learns the input or the result unless they
choose to reveal it. The parties are assumed to be semi-honest: they follow the protocol but
may try to learn from what they see.

| Gate | Computes | Paper | Online cost |
|---|---|---|---|
| `lt0` | `1` if `x < 0`, else `0` | Fig. 8 (signed comparison with 0) | 1 DCF evaluation |
| `ars` | `x >> s`, i.e. `floor(x / 2^s)`, exact | Fig. 7 (arithmetic right shift) | 2 DCF evaluations |

Values are signed (two's complement) integers or fixed-point numbers in Z_{2^n}, for any ring
width **1 ≤ n ≤ 256** and any number of fractional bits. Each run needs one network round
(plus one more if the parties reveal the result).

## Contents

- [Build](#build)
- [Try it in one terminal](#try-it-in-one-terminal)
- [Demo: dealer and two parties in three terminals](#demo-dealer-and-two-parties-in-three-terminals)
- [Command reference](#command-reference)
- [Use it as a library](#use-it-as-a-library)
- [Tests](#tests)
- [How it works](#how-it-works)

## Build

You need a Rust toolchain (tested with Rust 1.75).

```sh
cargo build --release
```

The program is then at `target/release/fss-gates`. The examples below use a shorthand for it:

```sh
B=target/release/fss-gates
```

Set `B` in every terminal you use, or type the full path instead.

## Try it in one terminal

`check` runs the whole protocol in one process: dealer, both parties, and a comparison with the
plaintext result. Every trial uses fresh keys and fresh random shares.

```sh
$B check --bits 16 --frac 8 -- -3.25 7.5
$B check --bits 144 --frac 53 --shift 53 -- -1234.5678 3.75
$B check --bits 256 --frac 128 -- -123456789012345678901234567890.0625
```

For each value it shows the expected result of `lt0` and `ars` and whether every trial matched
(`PASS`). It exits with code 1 if any trial fails.

Put `--` before the values, so that negative numbers aren't read as options.

## Demo: dealer and two parties in three terminals

This runs the real protocol over TCP. The example uses a 144-bit ring with 53 fractional bits,
and computes `x >> 53` on the inputs −1234.5678 and 3.75.

**Terminal 1: dealer and input owner**

```sh
$B deal  --gate ars --bits 144 --shift 53 --count 2 --out keys
$B share --bits 144 --frac 53 --out shares -- -1234.5678 3.75
```

- `deal` writes one key file per party, `keys/party0.key` and `keys/party1.key`, holding keys
  for 2 gate instances (`--count 2`).
- `share` encodes the values, splits each into two random shares, and writes them to
  `shares/party0.shares` and `shares/party1.shares`.
- `--count` must equal the number of values you share.

**Terminal 2: party 0** (start this one first; it waits for party 1)

```sh
$B party --id 0 --key keys/party0.key --input-file shares/party0.shares \
         --listen 127.0.0.1:7000 --reveal --frac 0
```

**Terminal 3: party 1**

```sh
$B party --id 1 --key keys/party1.key --input-file shares/party1.shares \
         --connect 127.0.0.1:7000 --reveal --frac 0
```

Both parties print their own output shares, and then the revealed results:

```
party 1: 2 output share(s): 3487868220732719537671075874023572798832871,479380227188110352597354488823937211464734
  ars(s=53) -> -1235 (raw -1235)
  ars(s=53) -> 3 (raw 3)
```

The share values are random and differ on every run. The results are always −1235 and 3:
−1234.5678 and 3.75 rounded down.

**To run the demo again,** run `deal` again first. Keys are single-use: each party deletes its
key file as soon as the other party connects.

### Things to know

- **`--frac` on `party` only affects printing.** The result is shown as `raw / 2^frac`. Set it
  to the number of fractional bits the *result* has: the input's fractional bits minus the
  shift. In the demo that is 53 − 53 = 0. For example, to truncate a fixed-point product that
  has 2 × 53 = 106 fractional bits, use `share --frac 106`, `deal --shift 53` and
  `party --frac 53`.
- **For `lt0` instead of `ars`,** use `deal --gate lt0` and leave out `--shift`. The result is
  `1` for negative inputs and `0` otherwise.
- **Without `--reveal`,** each party prints only its output shares. Use this when another
  computation will take the shares as input.
- **Without `--out`,** `share` prints the two share lists instead of writing files. Pass a list
  to `party` with `--input <list>` in place of `--input-file`.
- **On two machines,** party 0 listens on an address the other machine can reach (for example
  `--listen 0.0.0.0:7000`), and party 1 connects to it (`--connect <host>:7000`). Party 1
  keeps retrying for 60 seconds, so the start order is flexible.
- **Mistakes are caught.** The run stops with an error, instead of printing wrong results, if
  the parties use keys from different `deal` runs or the same party's keys, or if a party is
  given the wrong share file or values that don't fit the ring. These checks happen before the
  key is used, where possible.
- **Key and share files are secrets.** They are written readable only by you (mode 0600).
  `keys/` and `shares/` are in `.gitignore`.

## Command reference

Run `fss-gates` with no arguments to print this summary.

| Command | What it does |
|---|---|
| `check --bits N --frac F [--shift S] [--trials T] -- VALUE...` | Simulate both gates on each value in one process and compare with plaintext. `--shift` defaults to `F` (or `N − 1` if `F = N`); `--trials` defaults to 20. |
| `deal --gate lt0\|ars --bits N [--shift S] [--count K] --out DIR` | Write `DIR/party0.key` and `DIR/party1.key` for `K` gate instances (default 1). `ars` needs `--shift`. |
| `share --bits N --frac F [--out DIR] -- VALUE...` | Encode and split values into shares. Prints them, or with `--out` writes `DIR/party0.shares` and `DIR/party1.shares`. |
| `party --id 0\|1 --key FILE (--input LIST \| --input-file FILE) (--listen ADDR \| --connect ADDR) [--reveal] [--frac F]` | Run one party. Party 0 uses `--listen`, party 1 uses `--connect`. |

**Values** are either decimals or raw ring integers:

- **A decimal** such as `-3.25` is encoded as `round(v × 2^frac)`; halves round away from zero.
- **`raw:<int>`** such as `raw:-1` is the signed ring integer itself.
- A value must fit the signed range of an n-bit ring, or the command stops with an error.

**Limits:** `--bits` is 1 to 256, `--frac` is 0 to `--bits`, and `--shift` is 0 to `--bits` − 1.

## Use it as a library

```rust
use fss_gates::online::{eval_shared, reveal};
use fss_gates::transport::LocalChannel;
use fss_gates::{deal, FixedPoint, GateKind, Ring};

fn main() -> Result<(), fss_gates::Error> {
    let ring = Ring::new(64)?;
    let fp = FixedPoint::new(ring, 16)?;

    // Dealer: keys for 2 instances of x >> 16. Send keys0 to party 0 and keys1 to party 1.
    let (keys0, keys1) = deal(GateKind::Ars { shift: 16 }, ring, 2)?;

    // Input owner: encode the values and split them into additive shares.
    let xs = [fp.parse("-3.25")?, fp.parse("7.5")?];
    let (x0, x1): (Vec<_>, Vec<_>) = xs.iter().map(|&x| ring.share(x)).unzip();

    // The two parties, here on two threads joined by an in-process channel.
    let (mut ch0, mut ch1) = LocalChannel::pair();
    let party1 = std::thread::spawn(move || {
        let y1 = eval_shared(&keys1, &x1, &mut ch1)?;
        reveal(&keys1, &y1, &mut ch1)
    });
    let y0 = eval_shared(&keys0, &x0, &mut ch0)?; // party 0's shares of the outputs
    let ys = reveal(&keys0, &y0, &mut ch0)?;
    party1.join().unwrap()?;

    for y in ys {
        println!("{}", ring.to_signed(y)); // -4, 7
    }
    Ok(())
}
```

- **Ring elements** are `fss_gates::U256`; `ring.to_signed` gives an `I256`.
- **Keys** (`PartyKey`) implement serde, so the dealer can send them in any format, e.g. with
  `bincode`.
- **Over a network,** use `transport::TcpChannel::listen` (party 0) and
  `TcpChannel::connect` (party 1) in place of `LocalChannel`. You can also implement the
  `Channel` trait for your own transport.
- **`simulate(kind, ring, x)`** runs dealer and both parties in one call, for quick checks.
- **The bare gates** are in `gates::lt0` and `gates::ars`: `gen(ring, …, r_in, r_out)` and
  `eval(x_hat)`. Use these to build your own protocol around the masked input `x_hat = x + r_in`.

**Every key may be used on one input only.** Using a key twice reveals the difference of the
two inputs.

## Tests

```sh
cargo test                              # everything (takes a few seconds after building)
cargo test --release                    # the same with an optimized build
cargo test --test cli                   # one test file: cli, gates, my_cases or security
cargo test mismatched                   # only tests whose name contains "mismatched"
cargo test --manifest-path vendor/libfss/Cargo.toml   # the vendored libfss's own tests
```

Each test binary ends with a line like `test result: ok. 10 passed; 0 failed`. A failing test
is listed under `failures:` with the inputs that went wrong.

**Add your own cases** as lines in [tests/my_cases.txt](tests/my_cases.txt), in the form
`bits frac shift value`:

```
144 53 53 -1234.5678
```

Then run `cargo test --test my_cases`. Each line is checked with `lt0` and `ars`, 10 times
each, with fresh keys.

**What the tests cover:**

| File | Covers |
|---|---|
| `tests/gates.rs` | Every input and every shift for ring widths 1–8; edge values and random values for every width up to 256; the online protocol over in-process channels and TCP; key serialization; detection of mismatched keys; the TCP message format. |
| `tests/cli.rs` | The program end to end as separate processes: the demo flow above, key deletion, file permissions, the `check` command, and rejection of wrong inputs and mixed-up keys. |
| `tests/common/mod.rs` | The expected results, computed with BigInt arithmetic that shares no code with the library, so a bug in the library's arithmetic can't hide by also being in the expected values. |
| `tests/security.rs` | Regression test for the libfss key leak described below. |
| `tests/my_cases.rs` | The cases in `tests/my_cases.txt`. |
| `src/*.rs` | Unit tests: ring arithmetic, fixed-point parsing, and the DCF wrapper at full 256-bit width. |

The tests were checked by deliberately breaking the gates, the key check, the transport and
the CLI in 16 different ways; every change made at least one test fail.

## How it works

- **Masked input.** The dealer picks a random mask `r_in` for each gate. The parties add their
  shares of `r_in` to their shares of `x` and open `x + r_in`, which reveals nothing about `x`.
  Each party evaluates its FSS key on that public value and gets a share of the result plus a
  second mask `r_out`, which it removes with its share of `r_out`.
- **DCF.** Both gates are built from distributed comparison functions (DCFs). The DCF comes
  from libfss ([FSS-KRE](https://github.com/nann-cheng/FSS-KRE), commit `eb79d1b`), vendored in
  [vendor/libfss](vendor/libfss).
- **Security fix in libfss.** Upstream libfss returned constant control bits from its PRG, so
  **every DCF key contained the comparison point α in plaintext**. Here α comes from the input
  mask, so each party could have recovered the other's inputs. The vendored copy fixes this;
  see [vendor/libfss/PATCHES.md](vendor/libfss/PATCHES.md).
- **256-bit rings.** Ring elements are 256-bit integers (`U256` from the
  [`ethnum`](https://crates.io/crates/ethnum) crate). The DCF works in Z_{2^256} and its
  outputs are reduced mod 2^n. Reduction mod 2^n is compatible with addition and
  multiplication, so keys can store every value in ceil(n/8) bytes. The 16-byte PRG seeds per
  level stay, since they set the 128-bit security level.
- **Key sizes** per party and gate instance (`ars`):

  | Ring width | Shift | Key size |
  |---|---|---|
  | 32 | 12 | ≈ 1.2 KB |
  | 128 | 64 | ≈ 8.8 KB |
  | 256 | 128 | ≈ 27.7 KB |

- **Key check.** Each gate instance has a public random ID from the dealer. In the first round
  the parties exchange a digest of their IDs and their party numbers, which catches mixed-up
  keys before any result is computed.
- **Alternative considered:** [`fss-rs`](https://github.com/myl7/fss) has a DCF with
  byte-granular input domains. It would work, but it fits arbitrary bit widths less well than
  libfss's bit-by-bit DCF.
