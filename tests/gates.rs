use fss_gates::online::{eval_shared, reveal};
use fss_gates::transport::{Channel, LocalChannel, TcpChannel};
use fss_gates::{deal, simulate, GateKind, PartyKey, Ring, I256, U256};
use rand::Rng;
use std::time::Duration;

mod common;
use common::bigint_reference;

fn kinds(ring: Ring) -> Vec<GateKind> {
    let mut k = vec![GateKind::Lt0];
    k.extend((0..ring.bits()).map(|shift| GateKind::Ars { shift }));
    k
}

fn check(kind: GateKind, ring: Ring, x: U256) {
    let got = simulate(kind, ring, x).unwrap();
    let want = bigint_reference(kind, ring.bits(), x);
    assert_eq!(kind.reference(ring, x), want, "GateKind::reference disagrees with BigInt: {kind}, n={}, x={x}", ring.bits());
    assert_eq!(
        got,
        want,
        "{kind} on {}-bit ring, x = {} (raw {x}): got {}, want {}",
        ring.bits(),
        ring.to_signed(x),
        ring.to_signed(got),
        ring.to_signed(want)
    );
}

/// Every input, every shift, several fresh mask pairs each, for n = 1..=8.
#[test]
fn exhaustive_small_rings() {
    for n in 1..=8 {
        let ring = Ring::new(n).unwrap();
        for kind in kinds(ring) {
            for x in (0..=ring.mask().as_u128()).map(U256::new) {
                for _ in 0..3 {
                    check(kind, ring, x);
                }
            }
        }
    }
}

#[test]
fn edge_values_wide_rings() {
    for n in [9, 16, 31, 32, 33, 63, 64, 65, 100, 127, 128, 129, 192, 200, 255, 256] {
        let ring = Ring::new(n).unwrap();
        let (lo, hi) = ring.signed_range();
        let small = [-2, -1, 0, 1, 2].map(I256::new);
        let edges = [lo, lo + 1, lo / 2, hi / 2, hi - 1, hi].into_iter().chain(small);
        let shifts = [0, 1, n / 2, n - 2, n - 1];
        for v in edges {
            let x = ring.from_signed(v);
            check(GateKind::Lt0, ring, x);
            for &shift in &shifts {
                check(GateKind::Ars { shift }, ring, x);
            }
        }
    }
}

#[test]
fn random_values_all_widths() {
    let mut rng = rand::thread_rng();
    for n in 1..=256 {
        let ring = Ring::new(n).unwrap();
        for _ in 0..20 {
            let x = ring.random();
            check(GateKind::Lt0, ring, x);
            check(GateKind::Ars { shift: rng.gen_range(0, n) }, ring, x);
        }
    }
}

#[test]
fn invalid_parameters_are_rejected() {
    let ring = Ring::new(16).unwrap();
    assert!(deal(GateKind::Ars { shift: 16 }, ring, 1).is_err());
}

fn party<C: Channel>(keys: Vec<PartyKey>, xs: Vec<U256>, mut ch: C) -> (Vec<U256>, Vec<U256>) {
    let shares = eval_shared(&keys, &xs, &mut ch).unwrap();
    let out = reveal(&keys, &shares, &mut ch).unwrap();
    (shares, out)
}

fn run_parties<C: Channel + Send + 'static>(kind: GateKind, ring: Ring, xs: &[U256], ch0: C, ch1: C) -> Vec<U256> {
    let (k0, k1) = deal(kind, ring, xs.len()).unwrap();
    let (x0, x1): (Vec<U256>, Vec<U256>) = xs.iter().map(|&x| ring.share(x)).unzip();
    let h1 = std::thread::spawn(move || party(k1, x1, ch1));
    let (s0, out0) = party(k0, x0, ch0);
    let (s1, out1) = h1.join().unwrap();
    assert_eq!(out0, out1, "both parties must reveal the same outputs");
    let sums: Vec<U256> = s0.iter().zip(&s1).map(|(&a, &b)| ring.add(a, b)).collect();
    assert_eq!(sums, out0);
    out0
}

fn tcp_pair() -> (TcpChannel, TcpChannel) {
    // Port 0: the OS picks a free port, so parallel tests and other programs cannot collide.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let t = std::thread::spawn(move || TcpChannel::connect(addr, Duration::from_secs(10)).unwrap());
    let c0 = TcpChannel::accept(&listener).unwrap();
    (c0, t.join().unwrap())
}

#[test]
fn online_protocol_local_and_tcp() {
    for &use_tcp in &[false, true] {
        for (n, shift) in [(16, 8), (64, 20), (128, 64), (200, 100), (256, 128)] {
            let ring = Ring::new(n).unwrap();
            let xs: Vec<U256> = (0..50).map(|_| ring.random()).collect();
            for kind in [GateKind::Lt0, GateKind::Ars { shift }] {
                let out = if use_tcp {
                    let (c0, c1) = tcp_pair();
                    run_parties(kind, ring, &xs, c0, c1)
                } else {
                    let (c0, c1) = LocalChannel::pair();
                    run_parties(kind, ring, &xs, c0, c1)
                };
                let want: Vec<U256> = xs.iter().map(|&x| bigint_reference(kind, n, x)).collect();
                assert_eq!(out, want, "{kind} n={n} tcp={use_tcp}");
            }
        }
    }
}

#[test]
fn keys_roundtrip_through_serialization() {
    // Serialized keys keep only the low n bits of each payload; results must be unchanged.
    let mut rng = rand::thread_rng();
    for n in [1, 2, 7, 8, 9, 31, 32, 33, 64, 100, 127, 128, 129, 255, 256] {
        let ring = Ring::new(n).unwrap();
        for kind in [GateKind::Lt0, GateKind::Ars { shift: rng.gen_range(0, n) }] {
            let (k0, k1) = deal(kind, ring, 20).unwrap();
            let k0: Vec<PartyKey> = bincode::deserialize(&bincode::serialize(&k0).unwrap()).unwrap();
            let k1: Vec<PartyKey> = bincode::deserialize(&bincode::serialize(&k1).unwrap()).unwrap();
            assert!(k0.iter().zip(&k1).all(|(a, b)| a.id == b.id), "instance IDs survive serialization");
            for (a, b) in k0.iter().zip(&k1) {
                let x = ring.random();
                let x_hat = ring.add(x, ring.add(a.r_in_share, b.r_in_share));
                let y = ring.add(a.gate.eval(x_hat), b.gate.eval(x_hat));
                let y = ring.sub(y, ring.add(a.r_out_share, b.r_out_share));
                assert_eq!(y, bigint_reference(kind, n, x), "{kind} n={n} x={}", ring.to_signed(x));
            }
        }
    }
}

#[test]
fn tcp_rejects_oversized_message() {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let t = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        // Announce a huge message; a correct receiver must refuse before allocating it.
        peer.write_all(&u64::MAX.to_le_bytes()).unwrap();
        peer.write_all(&[32]).unwrap();
    });
    let mut c1 = TcpChannel::connect(addr, Duration::from_secs(10)).unwrap();
    let err = c1.exchange(&[U256::new(1), U256::new(2), U256::new(3)]).unwrap_err();
    assert!(err.to_string().contains("expected at most 3"), "{err}");
    t.join().unwrap();
}

#[test]
fn tcp_messages_use_ring_width_bytes() {
    // Masked values in Z_{2^n} travel as at most ceil(n/8) bytes each.
    use std::io::{Read, Write};
    for n in [32, 256] {
        let ring = Ring::new(n).unwrap();
        let xs: Vec<U256> = (0..10).map(|_| ring.random()).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let sent = xs.clone();
        // The connecting side is party 1, which receives before it sends.
        let t = std::thread::spawn(move || {
            let mut c1 = TcpChannel::connect(addr, Duration::from_secs(10)).unwrap();
            c1.exchange(&sent).unwrap()
        });
        let (mut peer, _) = listener.accept().unwrap();
        // Play party 0: send an empty message (count 0, width 0), then read party 1's.
        peer.write_all(&[0u8; 9]).unwrap();
        let mut header = [0u8; 9];
        peer.read_exact(&mut header).unwrap();
        assert_eq!(u64::from_le_bytes(header[..8].try_into().unwrap()), 10);
        let width = header[8] as usize;
        assert!(width <= ring.bytes(), "n={n}: width {width}");
        let mut body = vec![0u8; 10 * width];
        peer.read_exact(&mut body).unwrap();
        let got: Vec<U256> = body
            .chunks(width)
            .map(|c| {
                let mut buf = [0u8; 32];
                buf[..width].copy_from_slice(c);
                U256::from_le_bytes(buf)
            })
            .collect();
        assert_eq!(got, xs, "n={n}");
        assert!(t.join().unwrap().is_empty());
    }
}

/// Run the online phase with the given keys and return both parties' results.
fn try_parties(k0: Vec<PartyKey>, k1: Vec<PartyKey>, ring: Ring) -> [Result<Vec<U256>, String>; 2] {
    let x0: Vec<U256> = (0..k0.len()).map(|_| ring.random()).collect();
    let x1: Vec<U256> = (0..k1.len()).map(|_| ring.random()).collect();
    let (mut c0, mut c1) = LocalChannel::pair();
    let h = std::thread::spawn(move || eval_shared(&k1, &x1, &mut c1).map_err(|e| e.to_string()));
    let r0 = eval_shared(&k0, &x0, &mut c0).map_err(|e| e.to_string());
    [r0, h.join().unwrap()]
}

fn assert_both_fail(results: [Result<Vec<U256>, String>; 2], what: &str, msg: &str) {
    for (id, r) in results.into_iter().enumerate() {
        let err = r.expect_err(&format!("party {id} accepted {what}"));
        assert!(err.contains(msg), "party {id}, {what}: {err}");
    }
}

#[test]
fn mismatched_keys_are_detected() {
    let ring = Ring::new(32).unwrap();
    let kind = GateKind::Ars { shift: 12 };
    let (a0, a1) = deal(kind, ring, 3).unwrap();
    let (b0, b1) = deal(kind, ring, 3).unwrap();

    // Matching keys work.
    for r in try_parties(a0.clone(), a1.clone(), ring) {
        r.unwrap();
    }
    assert_both_fail(try_parties(a0.clone(), b1.clone(), ring), "keys from two deals", "do not match");
    let mut reordered = a1.clone();
    reordered.swap(0, 2);
    assert_both_fail(try_parties(a0.clone(), reordered, ring), "reordered keys", "do not match");
    assert_both_fail(try_parties(a0.clone(), b0.clone(), ring), "two party-0 key sets", "both parties hold party 0");
    assert_both_fail(try_parties(a1.clone(), b1.clone(), ring), "two party-1 key sets", "both parties hold party 1");
    assert_both_fail(try_parties(a0.clone(), a1[..2].to_vec(), ring), "a shorter key set", "peer sent");

    // Same deal but a different ring width on the other side.
    let wide = Ring::new(64).unwrap();
    let (_, w1) = deal(kind, wide, 3).unwrap();
    let mut mixed = w1;
    for (k, a) in mixed.iter_mut().zip(&a1) {
        k.id = a.id;
    }
    let (x0, x1): (Vec<U256>, Vec<U256>) = (0..3).map(|_| ring.share(ring.random())).unzip();
    let (mut c0, mut c1) = LocalChannel::pair();
    let h = std::thread::spawn(move || eval_shared(&mixed, &x1, &mut c1).map_err(|e| e.to_string()));
    let r0 = eval_shared(&a0, &x0, &mut c0).map_err(|e| e.to_string());
    assert_both_fail([r0, h.join().unwrap()], "different ring widths", "do not match");

    // Mixing both parties' keys in one set is caught before anything is sent.
    let mut both = a0.clone();
    both[1] = a1[1].clone();
    let (mut c, _peer) = LocalChannel::pair();
    let err = eval_shared(&both, &[U256::ZERO; 3], &mut c).unwrap_err();
    assert!(err.to_string().contains("mix party 0 and party 1"), "{err}");
}
