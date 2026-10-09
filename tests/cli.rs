//! End-to-end tests of the `fss-gates` binary: dealer, input owner and both parties as separate
//! processes talking over TCP, with key and share files on disk.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_fss-gates");

/// A fresh directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("fss-gates-{tag}-{}-{:x}", std::process::id(), rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_str().unwrap().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Run `fss-gates` and require exit code 0.
fn ok(args: &[&str]) -> String {
    let o = run(args);
    assert!(o.status.success(), "fss-gates {}\nstdout:\n{}\nstderr:\n{}", args.join(" "), stdout(&o), stderr(&o));
    stdout(&o)
}

/// Run `fss-gates`, require exit code 2 (an error) and return stderr.
fn fails(args: &[&str]) -> String {
    let o = run(args);
    assert_eq!(o.status.code(), Some(2), "fss-gates {} should fail\nstdout:\n{}\nstderr:\n{}", args.join(" "), stdout(&o), stderr(&o));
    stderr(&o)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Both parties as separate processes. `extra[id]` is appended to party `id`'s arguments.
fn run_parties(dir: &TempDir, extra: [&[&str]; 2]) -> [Output; 2] {
    let addr = format!("127.0.0.1:{}", free_port());
    let spawn = |id: usize, net: [&str; 2]| {
        let (key, id_s) = (dir.path(&format!("keys/party{id}.key")), id.to_string());
        let mut args = vec!["party", "--id", &id_s, "--key", &key, net[0], net[1]];
        args.extend_from_slice(extra[id]);
        Command::new(BIN).args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap()
    };
    let p0 = spawn(0, ["--listen", &addr]);
    let p1 = spawn(1, ["--connect", &addr]);
    [p0.wait_with_output().unwrap(), p1.wait_with_output().unwrap()]
}

#[cfg(unix)]
fn assert_private(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "{path} has mode {mode:o}");
}

#[cfg(not(unix))]
fn assert_private(_: &str) {}

#[test]
fn demo_flow_with_share_files() {
    let dir = TempDir::new("demo");
    let (keys, shares) = (dir.path("keys"), dir.path("shares"));
    ok(&["deal", "--gate", "ars", "--bits", "144", "--shift", "53", "--count", "3", "--out", &keys]);
    ok(&["share", "--bits", "144", "--frac", "53", "--out", &shares, "--", "-1234.5678", "3.75", "raw:-1"]);
    for f in ["keys/party0.key", "keys/party1.key", "shares/party0.shares", "shares/party1.shares"] {
        assert_private(&dir.path(f));
    }
    let header = std::fs::read_to_string(dir.path("shares/party1.shares")).unwrap();
    assert!(header.starts_with("# fss-gates shares bits=144 party=1\n"), "{header}");

    let (s0, s1) = (dir.path("shares/party0.shares"), dir.path("shares/party1.shares"));
    let outs = run_parties(&dir, [&["--input-file", &s0, "--reveal"], &["--input-file", &s1, "--reveal"]]);
    for (id, o) in outs.iter().enumerate() {
        assert!(o.status.success(), "party {id}:\n{}{}", stdout(o), stderr(o));
        let out = stdout(o);
        // floor(-1234.5678 * 2^53 / 2^53) = -1235, floor(3.75) = 3, floor(-1 / 2^53) = -1.
        for want in ["ars(s=53) -> -1235 (raw -1235)", "ars(s=53) -> 3 (raw 3)", "ars(s=53) -> -1 (raw -1)"] {
            assert!(out.contains(want), "party {id} output lacks `{want}`:\n{out}");
        }
    }
    // Keys are single-use: deleted after the run, and a second run says so.
    assert!(!Path::new(&dir.path("keys/party0.key")).exists());
    assert!(!Path::new(&dir.path("keys/party1.key")).exists());
    let err = fails(&["party", "--id", "0", "--key", &dir.path("keys/party0.key"), "--input-file", &s0, "--listen", "127.0.0.1:0"]);
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn printed_shares_and_output_shares_add_up() {
    let dir = TempDir::new("print");
    let keys = dir.path("keys");
    ok(&["deal", "--gate", "lt0", "--bits", "256", "--count", "2", "--out", &keys]);
    let printed = ok(&["share", "--bits", "256", "--frac", "100", "--", "-0.5", "0.5"]);
    let list = |id: u32| {
        printed.lines().find_map(|l| l.strip_prefix(&format!("party {id} --input "))).unwrap().to_string()
    };
    let (l0, l1) = (list(0), list(1));
    let outs = run_parties(&dir, [&["--input", &l0], &["--input", &l1]]);
    // Without --reveal the parties print only shares; they must add up to 1{x < 0} mod 2^256.
    let shares = |o: &Output| -> Vec<num_bigint::BigUint> {
        let out = stdout(o);
        let line = out.lines().find_map(|l| l.split_once("output share(s): ").map(|(_, s)| s.to_string())).unwrap();
        line.split(',').map(|v| v.parse().unwrap()).collect()
    };
    let modulus = num_bigint::BigUint::from(1u8) << 256usize;
    let sums: Vec<_> = shares(&outs[0]).iter().zip(shares(&outs[1])).map(|(a, b)| (a + b) % &modulus).collect();
    assert_eq!(sums, [1u8, 0].map(num_bigint::BigUint::from));
}

#[test]
fn check_command() {
    let out = ok(&["check", "--bits", "144", "--frac", "53", "--trials", "3", "--", "-1234.5678", "raw:-1"]);
    assert!(out.contains("all checks passed"), "{out}");
    assert_eq!(out.matches("PASS").count(), 4, "{out}");
    assert!(fails(&["check", "--bits", "16", "--frac", "8", "--", "128"]).contains("outside the signed 16-bit range"));
    assert!(fails(&["check", "--bits", "257", "--frac", "8", "--", "1"]).contains("1..=256"));
    assert!(fails(&["check", "--bits", "16", "--frac", "8", "--shift", "16", "--", "1"]).contains("shift"));
}

#[test]
fn wrong_inputs_are_rejected_before_connecting() {
    let dir = TempDir::new("inputs");
    let keys = dir.path("keys");
    ok(&["deal", "--gate", "ars", "--bits", "32", "--shift", "8", "--count", "2", "--out", &keys]);
    ok(&["share", "--bits", "32", "--frac", "8", "--out", &dir.path("s32"), "--", "1", "2"]);
    ok(&["share", "--bits", "64", "--frac", "8", "--out", &dir.path("s64"), "--", "1", "2"]);
    let key0 = dir.path("keys/party0.key");
    // Nothing listens on port 1; every case must fail before trying to connect.
    let party0 = |input: &[&str]| {
        let mut args = vec!["party", "--id", "0", "--key", &key0, "--listen", "127.0.0.1:1"];
        args.extend_from_slice(input);
        fails(&args)
    };
    let e = party0(&["--input-file", &dir.path("s32/party1.shares")]);
    assert!(e.contains("party=1, but the key is for bits=32 party=0"), "{e}");
    let e = party0(&["--input-file", &dir.path("s64/party0.shares")]);
    assert!(e.contains("bits=64 party=0, but the key is for bits=32 party=0"), "{e}");
    let e = party0(&["--input", "1,4294967296"]);
    assert!(e.contains("does not fit the 32-bit ring"), "{e}");
    let e = party0(&["--input", "1,2,3"]);
    assert!(e.contains("2 gate instances but --input has 3"), "{e}");
    let e = party0(&["--input", "1,2", "--input-file", &dir.path("s32/party0.shares")]);
    assert!(e.contains("exactly one of"), "{e}");
    let e = fails(&["party", "--id", "1", "--key", &key0, "--input", "1,2", "--connect", "127.0.0.1:1"]);
    assert!(e.contains("key file is for party 0, not 1"), "{e}");
    // None of the failures may consume the key.
    assert!(Path::new(&key0).exists());
}

#[test]
fn keys_from_different_deals_are_rejected() {
    let dir = TempDir::new("mixed");
    ok(&["deal", "--gate", "lt0", "--bits", "64", "--count", "2", "--out", &dir.path("keys")]);
    ok(&["deal", "--gate", "lt0", "--bits", "64", "--count", "2", "--out", &dir.path("other")]);
    std::fs::rename(dir.path("other/party1.key"), dir.path("keys/party1.key")).unwrap();
    let outs = run_parties(&dir, [&["--input", "1,2"], &["--input", "3,4"]]);
    for (id, o) in outs.iter().enumerate() {
        assert_eq!(o.status.code(), Some(2), "party {id} should fail:\n{}", stdout(o));
        assert!(stderr(o).contains("do not match"), "party {id}: {}", stderr(o));
        assert!(!stdout(o).contains("output share"), "party {id} printed outputs:\n{}", stdout(o));
    }
}
