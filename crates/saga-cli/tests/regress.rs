//! Pinned regression suite: the invariants we used to re-check by hand,
//! frozen as tests so any future change that breaks one fails loudly.
//!
//! Every test here drives the REAL built binary (`saga generate`) through
//! `std::process::Command`, parses its `--json` summary, and reads the
//! output files back with `saga_io::read`. Generated graphs go to a unique
//! temp directory that is removed when the test ends (pass or fail).
//!
//! Two kinds of pin are used for output identity:
//!   * file sha256: exact bytes of edges_00000.parquet / labels_nodes.csv /
//!     ground_truth.json. This catches any change, including a writer change
//!     that keeps content.
//!   * logical digest: sha256 over the SORTED lines
//!     `src,dst,t,event_type,label,amount-or-first-numeric-attr`, read back
//!     from the edge shards. Independent of the writer, the shard layout, the
//!     row-group size and the row order.
//! When both are pinned the assertion message states WHICH one failed, so a
//! writer-only change (bytes differ, logical equal) is distinguishable from
//! a semantic change (logical differs).
//!
//! Default suite (fast, runs under `cargo test --release`):
//!   * bytecompat_100k_logical_all_domains
//!   * sweep_spot_points_finance_tiny
//!   * guards_on_thread_determinism_all_domains
//!   * calibration_ecommerce_1m_within_tolerance
//! Heavy pins (`#[ignore]`), run with
//!   `cargo test --release -p saga-cli --test regress -- --ignored --nocapture`
//! or via `scripts/regress.sh`:
//!   * bytecompat_1m_all_domains          (full file sha256 + logical digest)
//!   * exchange_1m_two_processes_equal_single (union == single, 887,156 edges)
//! Every pinned value below was computed from the binary on 2026-08-21
//! (i9-13900K, Ubuntu 22.04); see docs/VERIFICATION_MATRIX.md
//! "Regression suite".

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_saga");

// ---------------------------------------------------------------------------
// Pins
// ---------------------------------------------------------------------------

/// Invariant 1 (heavy): guards-off default output, `--edges 1000000 --seed 42
/// --threads 8` (Small scaffold: 100k nodes, 90 days), parquet.
/// (domain, edges_00000.parquet sha256, labels_nodes.csv sha256,
///  ground_truth.json sha256, logical digest, edges_written)
const PIN_1M: [(&str, &str, &str, &str, &str, u64); 6] = [
    (
        "finance",
        "5893b7aac3dfa4e953389ddb8850d55babe8f59c0908c6027af3a0a951fdb657",
        "334ba9ab03befa008508f054ed11c833de3cbefed53c63440b49f55d245955c9",
        "a231a0d2148567d020c4dee3f68ee9513d3300dab370d7c20f810dfe46b52206",
        "a94e44c674178bbdb5cf383f2081f7aed4bb2f3d562efeb5cbe793605396e925",
        998371,
    ),
    (
        "crypto",
        "9cb42c182962c420f691c256f0703bbd86af9dc2919f369fb8db8abd37a8f42c",
        "d8104bf5a60a23528f786df9bbc2456b5a2ae2d4d673076ab7102d20a13743ae",
        "82d361e01c7b9a5c7a5d8028c0ed28043ddff6719f8fe009414a4857ea09dfba",
        "072cbc88938746a37396cbe9fcbf7b04c3833fef8256eb5463d2f7d0dc189409",
        981636,
    ),
    (
        "cyber",
        "288479830e781b731328786c094eaa5a85ec26d140e73b8a602e2a197305a58f",
        "3395dcd929d1066c5df627b028accaeaba232e9cf621fab7b68e83b56fac307f",
        "43399d8a00daaaea449b90d2c92ba47fc6dc03d6de224162531a075448661e19",
        "ca00c2e6d93ca3d5c1098f970728e869c2e0384b8011f4199f3ab3ed9a678768",
        993400,
    ),
    (
        "ecommerce",
        "e655061634f06d3a5494f71ae4b6c1956a342d8b2796707c4712e09a028a73e2",
        "e503581b5d472fb983c5a9edd8811e3c4cc9be79d92ac5cd48d3dfc97f645830",
        "067476482702b5e345ed12af3d691a54f8a3dc6c8ca13ece18f797f4dcb1a1b7",
        "fe8683737fed13ef5032d8e559cf43d5e17d10217694bf5daafca08a73a4a184",
        997103,
    ),
    (
        "healthcare",
        "efe4e66cc613c1e4fb136848b3ec5ba983b17efd47d032558897cf8a18685538",
        "266b05f3e93d764b49882d94bf107f94b317c67c358bdb812fb40681af7d52bf",
        "9657e70e84f788357480027a746e762aea872904dda3e95f56413879484dc9fe",
        "c3c1f93fbc67f141c93801bd7bd7700cdab45aa2628ac3ccb52b5c2c339dd870",
        993791,
    ),
    (
        "transport",
        "78f81253944b05f42db19ee7d4bd1b962235176a634fb132065f94daf323120d",
        "99297fe455c034921f302dab4d3d523c61c3f3eeb56cd9335460d1bb99332b53",
        "5adb949fdd3c4a6a7b4bea8ba4c01d2d2ad9c45663ccdc1174741759881c33b2",
        "a44e8f6096d700b1656711344ac9186d67154641781290960aa52ee2b161d358",
        998236,
    ),
];

/// Invariant 1 (fast): same command at `--edges 100000`.
/// (domain, logical digest, edges_written)
const PIN_100K: [(&str, &str, u64); 6] = [
    ("finance", "7707b2e33450baefde03c4767dbe90fa84b74cede03ebb5688ef9b0809be5d40", 100195),
    ("crypto", "d8841318f4e183e85f4f136e99f356fe756599f34ab6a58c9e92240775c5caf2", 98618),
    ("cyber", "b2acf855fad8ffe057f04fa122ec1dc56fbfcd8bb4fedb9ff82b5e7e84ada5f7", 99427),
    ("ecommerce", "813a221999cb511d7150b4bbf25b7d53d66a9824640d95a52b2c2832a4958288", 99435),
    ("healthcare", "ca9b318a4eaadb81f6db4264f72ff233c95ee56425d8abeae291ce40b7de17e0", 99224),
    ("transport", "cfa8a0b574b384a777e02b30879fb6effe04e7bed0c214ca83fd1b3006fce9eb", 100153),
];

/// Invariant 2: finance `--preset tiny --no-anomalies --guards --seed 42
/// --threads 8`, L in days: (L, overdraft_violations, edges_written).
const PIN_SWEEP: [(&str, u64, u64); 3] =
    [("0.001", 1214, 878315), ("1", 66205, 821880), ("30", 291420, 961453)];

/// Invariant 3: guards on, anomalies on, 20k nodes / 200k edges / 14 days,
/// seed 42, L=1, CSV; threads 1 and 8 must agree AND match these values.
/// (domain, edges_written, anomalous_edges, overdraft_violations)
const PIN_GUARDS_SMALL: [(&str, u64, u64, u64); 6] = [
    ("finance", 175768, 1187, 7332),
    ("crypto", 58954, 34, 9782),
    ("cyber", 161014, 109, 7803),
    ("ecommerce", 168165, 8231, 4060),
    ("healthcare", 189520, 6103, 863),
    ("transport", 183982, 139, 1149),
];

/// Invariant 5 (heavy): finance `--nodes 100000 --edges 1000000 --seed 42
/// --guards --block-len-days 1 --threads 8`: single-machine edge count and
/// overdraft counters; N=2 real shard processes must union to the same
/// logical digest.
const PIN_XCHG_EDGES: u64 = 887_156;
const PIN_XCHG_GUARDED: u64 = 421_982;
const PIN_XCHG_VIOLATIONS: u64 = 20_437;

// ---------------------------------------------------------------------------
// Test support
// ---------------------------------------------------------------------------

/// A unique temp directory removed on drop (pass or fail).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "agora_regress_{}_{}_{}",
            std::process::id(),
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run `saga generate <args> --json --quiet --out <out>` and return the
/// parsed summary JSON printed on stdout.
fn generate(args: &[&str], out: &Path) -> serde_json::Value {
    let mut c = Command::new(BIN);
    c.arg("generate").args(args).args(["--json", "--quiet", "--out"]).arg(out);
    let o = c.output().expect("spawn saga");
    assert!(
        o.status.success(),
        "saga generate {:?} failed:\n{}",
        args,
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!("bad --json summary for {args:?}: {e}\n{}", String::from_utf8_lossy(&o.stdout))
    })
}

fn u64_of(v: &serde_json::Value, key: &str) -> u64 {
    v.get(key)
        .and_then(|x| x.as_u64())
        .unwrap_or_else(|| panic!("summary has no u64 field `{key}`"))
}

fn f64_of(v: &serde_json::Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|x| x.as_f64())
        .unwrap_or_else(|| panic!("summary has no f64 field `{key}`"))
}

/// Writer-independent digest of the edge content under `dir` (all
/// edges_*.parquet / edges_*.csv shards): sha256 over the sorted lines
/// `src,dst,t,event_type,label,amt`, where `amt` is the numeric attribute
/// named `amount` when present, else the first numeric attribute, else NaN.
fn logical_digest(dir: &Path) -> (String, usize) {
    let mut lines = logical_lines(dir);
    lines.sort_unstable();
    (sha256_hex_of_lines(&lines), lines.len())
}

/// Union-friendly form: lines from several directories, one digest.
fn logical_digest_union(dirs: &[PathBuf]) -> (String, usize) {
    let mut lines = Vec::new();
    for d in dirs {
        lines.extend(logical_lines(d));
    }
    lines.sort_unstable();
    (sha256_hex_of_lines(&lines), lines.len())
}

fn logical_lines(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for shard in saga_io::read::edge_shards(dir).unwrap() {
        saga_io::read::read_shard(&shard, &mut |b| {
            let amt_col = b
                .numeric_attrs
                .iter()
                .position(|(n, _)| n == "amount")
                .or(if b.numeric_attrs.is_empty() { None } else { Some(0) });
            for i in 0..b.src.len() {
                let amt = amt_col.map(|c| b.numeric_attrs[c].1[i]).unwrap_or(f64::NAN);
                out.push(format!(
                    "{},{},{},{},{},{:?}",
                    b.src[i], b.dst[i], b.t[i], b.event_type[i], b.label[i], amt
                ));
            }
            Ok(())
        })
        .unwrap();
    }
    out
}

fn sha256_hex_of_lines(lines: &[String]) -> String {
    let mut h = Sha256::new();
    for l in lines {
        h.update(l.as_bytes());
        h.update(b"\n");
    }
    h.finish_hex()
}

fn sha256_file(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut h = Sha256::new();
    h.update(&bytes);
    h.finish_hex()
}

/// Compare a pinned value, collecting a readable failure instead of
/// panicking immediately so one test reports every broken pin.
fn check<T: PartialEq + std::fmt::Debug>(fails: &mut Vec<String>, what: &str, got: T, want: T) {
    if got != want {
        fails.push(format!("{what}: got {got:?}, pinned {want:?}"));
    }
}

fn check_str(fails: &mut Vec<String>, what: &str, got: &str, want: &str) {
    if got != want {
        fails.push(format!("{what}: got \"{got}\", pinned \"{want}\""));
    }
}

fn finish(test: &str, fails: Vec<String>) {
    assert!(fails.is_empty(), "{test}: {} pinned invariant(s) broken:\n  {}", fails.len(), fails.join("\n  "));
}

// ---------------------------------------------------------------------------
// Minimal SHA-256 (FIPS 180-4), so the suite needs no extra crate and no
// external `sha256sum` binary. Verified against the file hashes that the
// CONSISTENCY_MODEL reference table recorded with coreutils sha256sum.
// ---------------------------------------------------------------------------

struct Sha256 {
    state: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: Vec::with_capacity(128),
            len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        let mut data = data;
        if !self.buf.is_empty() {
            let need = 64 - self.buf.len();
            let take = need.min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == 64 {
                let block: [u8; 64] = self.buf[..].try_into().unwrap();
                self.compress(&block);
                self.buf.clear();
            }
        }
        let mut chunks = data.chunks_exact(64);
        for c in &mut chunks {
            self.compress(c.try_into().unwrap());
        }
        self.buf.extend_from_slice(chunks.remainder());
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }

    fn finish_hex(mut self) -> String {
        let bit_len = self.len.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        while (self.buf.len() + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&bit_len.to_be_bytes());
        self.len = 0; // update() would otherwise count the padding
        self.update(&pad);
        debug_assert!(self.buf.is_empty());
        self.state.iter().map(|w| format!("{w:08x}")).collect()
    }
}

#[test]
fn sha256_self_test() {
    let mut h = Sha256::new();
    h.update(b"abc");
    assert_eq!(h.finish_hex(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    let h = Sha256::new();
    assert_eq!(h.finish_hex(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    // Multi-block, fed in odd-sized pieces.
    let mut h = Sha256::new();
    let msg = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    h.update(&msg[..5]);
    h.update(&msg[5..]);
    assert_eq!(h.finish_hex(), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
}

// ---------------------------------------------------------------------------
// Invariant 1: byte-compat / logical digest of the guards-off default output
// ---------------------------------------------------------------------------

fn guards_off_args(domain: &str, edges: &str) -> Vec<String> {
    ["--domain", domain, "--edges", edges, "--seed", "42", "--threads", "8"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn bytecompat_100k_logical_all_domains() {
    let root = TempDir::new("bc100k");
    let mut fails = Vec::new();
    for (domain, want_digest, want_edges) in PIN_100K {
        let out = root.path().join(domain);
        let args = guards_off_args(domain, "100000");
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let s = generate(&args, &out);
        let (digest, n) = logical_digest(&out);
        println!("100k {domain}: edges_written {} logical {digest}", u64_of(&s, "edges_written"));
        check(&mut fails, &format!("{domain} 100k edges_written"), u64_of(&s, "edges_written"), want_edges);
        check(&mut fails, &format!("{domain} 100k edge rows read back"), n as u64, want_edges);
        check_str(&mut fails, &format!("{domain} 100k LOGICAL digest (semantic change)"), &digest, want_digest);
    }
    finish("bytecompat_100k", fails);
}

/// Heavy: full sha256 of the three reference files plus the logical digest
/// for all six domains at 1M edges. A failure names which layer moved:
/// "PARQUET BYTES" alone means a writer change that kept the content;
/// "LOGICAL" means the generated graph itself changed.
#[test]
#[ignore = "heavy pin; run: cargo test --release -p saga-cli --test regress -- --ignored --nocapture"]
fn bytecompat_1m_all_domains() {
    let root = TempDir::new("bc1m");
    let mut fails = Vec::new();
    for (domain, want_parquet, want_labels, want_gt, want_digest, want_edges) in PIN_1M {
        let out = root.path().join(domain);
        let args = guards_off_args(domain, "1000000");
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let s = generate(&args, &out);
        let parquet = sha256_file(&out.join("edges_00000.parquet"));
        let labels = sha256_file(&out.join("labels_nodes.csv"));
        let gt = sha256_file(&out.join("ground_truth.json"));
        let (digest, n) = logical_digest(&out);
        println!(
            "1M {domain}: edges_written {} parquet {parquet} labels {labels} gt {gt} logical {digest}",
            u64_of(&s, "edges_written")
        );
        let logical_ok = digest == want_digest;
        check(&mut fails, &format!("{domain} 1M edges_written"), u64_of(&s, "edges_written"), want_edges);
        check(&mut fails, &format!("{domain} 1M edge rows read back"), n as u64, want_edges);
        check_str(&mut fails, &format!("{domain} 1M LOGICAL digest (semantic change)"), &digest, want_digest);
        let layer = if logical_ok { "writer-only change, content identical" } else { "content also changed" };
        check_str(&mut fails, &format!("{domain} 1M PARQUET BYTES edges_00000.parquet ({layer})"), &parquet, want_parquet);
        check_str(&mut fails, &format!("{domain} 1M labels_nodes.csv sha256"), &labels, want_labels);
        check_str(&mut fails, &format!("{domain} 1M ground_truth.json sha256"), &gt, want_gt);
    }
    finish("bytecompat_1m", fails);
}

// ---------------------------------------------------------------------------
// Invariant 2: guarded sweep spot-points
// ---------------------------------------------------------------------------

#[test]
fn sweep_spot_points_finance_tiny() {
    let root = TempDir::new("sweep");
    let mut fails = Vec::new();
    for (l, want_viol, want_edges) in PIN_SWEEP {
        let out = root.path().join(format!("L{l}"));
        let s = generate(
            &[
                "--domain", "finance", "--preset", "tiny", "--no-anomalies", "--guards",
                "--block-len-days", l, "--seed", "42", "--threads", "8",
            ],
            &out,
        );
        println!(
            "L={l}: edges_written {} guarded {} violations {}",
            u64_of(&s, "edges_written"),
            u64_of(&s, "overdraft_guarded"),
            u64_of(&s, "overdraft_violations")
        );
        check(&mut fails, &format!("L={l} overdraft_violations"), u64_of(&s, "overdraft_violations"), want_viol);
        check(&mut fails, &format!("L={l} edges_written"), u64_of(&s, "edges_written"), want_edges);
    }
    finish("sweep_spot_points", fails);
}

// ---------------------------------------------------------------------------
// Invariant 3: guards-on determinism across thread counts, all domains
// ---------------------------------------------------------------------------

#[test]
fn guards_on_thread_determinism_all_domains() {
    let root = TempDir::new("gdet");
    let mut fails = Vec::new();
    for (domain, want_edges, want_anom, want_viol) in PIN_GUARDS_SMALL {
        let mut seen: HashMap<&str, (String, u64, u64, u64)> = HashMap::new();
        for threads in ["1", "8"] {
            let out = root.path().join(format!("{domain}_t{threads}"));
            let s = generate(
                &[
                    "--domain", domain, "--nodes", "20000", "--edges", "200000", "--time-span", "14",
                    "--seed", "42", "--guards", "--block-len-days", "1", "--threads", threads,
                    "--format", "csv",
                ],
                &out,
            );
            let (digest, _) = logical_digest(&out);
            let rec = (
                digest,
                u64_of(&s, "edges_written"),
                u64_of(&s, "anomalous_edges"),
                u64_of(&s, "overdraft_violations"),
            );
            println!("{domain} t={threads}: edges {} anomalous {} violations {} logical {}", rec.1, rec.2, rec.3, rec.0);
            seen.insert(threads, rec);
        }
        let (d1, e1, a1, v1) = &seen["1"];
        let (d8, e8, a8, v8) = &seen["8"];
        check_str(&mut fails, &format!("{domain} logical digest threads 1 vs 8"), d8, d1);
        check(&mut fails, &format!("{domain} edges_written threads 1 vs 8"), *e8, *e1);
        check(&mut fails, &format!("{domain} anomalous_edges threads 1 vs 8"), *a8, *a1);
        check(&mut fails, &format!("{domain} overdraft_violations threads 1 vs 8"), *v8, *v1);
        check(&mut fails, &format!("{domain} edges_written"), *e1, want_edges);
        check(&mut fails, &format!("{domain} anomalous_edges"), *a1, want_anom);
        check(&mut fails, &format!("{domain} overdraft_violations"), *v1, want_viol);
    }
    finish("guards_on_thread_determinism", fails);
}

// ---------------------------------------------------------------------------
// Invariant 4: guard-aware calibration lands within tolerance (ecommerce 1M)
// ---------------------------------------------------------------------------

#[test]
fn calibration_ecommerce_1m_within_tolerance() {
    let root = TempDir::new("cal");
    let s = generate(
        &[
            "--domain", "ecommerce", "--edges", "1000000", "--seed", "42", "--threads", "8",
            "--guards", "--block-len-days", "1", "--calibrate-under-guards", "1",
        ],
        &root.path().join("out"),
    );
    let planned = f64_of(&s, "planned_anomalous_fraction");
    let edges = u64_of(&s, "edges_written");
    let anomalous = u64_of(&s, "anomalous_edges");
    let realized = anomalous as f64 / edges as f64;
    let gap = (realized - planned).abs() / planned;
    println!(
        "ecommerce 1M calibrated: planned {:.4}% realized {:.4}% ({anomalous}/{edges}) gap {:.3}% relative",
        100.0 * planned,
        100.0 * realized,
        100.0 * gap
    );
    assert_eq!(u64_of(&s, "calibration_passes"), 1);
    assert!(planned > 0.008 && planned < 0.0095, "planned fraction moved: {planned}");
    assert!(gap < 0.02, "realized anomalous fraction off planned by {:.3}% relative (pinned tolerance 2%)", 100.0 * gap);
}

// ---------------------------------------------------------------------------
// Invariant 5 (heavy): two real shard processes at the 1M exchange config
// union to the single-machine run. (The small N in {2,4} variant with node
// state and emulation cross-checks is tests/exchange_cli.rs, default suite.)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "heavy pin; run: cargo test --release -p saga-cli --test regress -- --ignored --nocapture"]
fn exchange_1m_two_processes_equal_single() {
    let root = TempDir::new("x1m");
    let base = [
        "--domain", "finance", "--nodes", "100000", "--edges", "1000000", "--seed", "42", "--guards",
        "--block-len-days", "1", "--threads", "8",
    ];
    let single_out = root.path().join("single");
    let s = generate(&base, &single_out);
    let (single_digest, single_n) = logical_digest(&single_out);
    println!(
        "single: edges {} guarded {} violations {} logical {single_digest}",
        u64_of(&s, "edges_written"),
        u64_of(&s, "overdraft_guarded"),
        u64_of(&s, "overdraft_violations")
    );
    let mut fails = Vec::new();
    check(&mut fails, "single edges_written", u64_of(&s, "edges_written"), PIN_XCHG_EDGES);
    check(&mut fails, "single edge rows read back", single_n as u64, PIN_XCHG_EDGES);
    check(&mut fails, "single overdraft_guarded", u64_of(&s, "overdraft_guarded"), PIN_XCHG_GUARDED);
    check(&mut fails, "single overdraft_violations", u64_of(&s, "overdraft_violations"), PIN_XCHG_VIOLATIONS);

    let n = 2u64;
    let xdir = root.path().join("xchg");
    let out = root.path().join("sharded");
    let children: Vec<_> = (0..n)
        .map(|k| {
            let mut c = Command::new(BIN);
            c.arg("generate").args(base).args([
                "--shard-count",
                &n.to_string(),
                "--shard-index",
                &k.to_string(),
                "--exchange-timeout-s",
                "120",
                "--exchange-dir",
            ]);
            c.arg(&xdir).args(["--json", "--quiet", "--out"]).arg(&out);
            c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
            c.spawn().unwrap()
        })
        .collect();
    let mut shard_dirs = Vec::new();
    let mut shard_edges = 0u64;
    for (k, child) in children.into_iter().enumerate() {
        let o = child.wait_with_output().unwrap();
        assert!(o.status.success(), "shard {k} failed: {}", String::from_utf8_lossy(&o.stderr));
        let s: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        shard_edges += u64_of(&s, "edges_written");
        shard_dirs.push(out.join(format!("shard_{k:02}")));
    }
    let (union_digest, union_n) = logical_digest_union(&shard_dirs);
    println!("N={n}: union edges {union_n} (summaries {shard_edges}) logical {union_digest}");
    check(&mut fails, "N=2 union edges_written", shard_edges, PIN_XCHG_EDGES);
    check(&mut fails, "N=2 union edge rows read back", union_n as u64, PIN_XCHG_EDGES);
    check_str(&mut fails, "N=2 union logical digest vs single machine", &union_digest, &single_digest);
    let leftovers = std::fs::read_dir(&xdir).map(|rd| rd.count()).unwrap_or(0);
    check(&mut fails, "exchange files left behind", leftovers, 0);
    finish("exchange_1m", fails);
}
