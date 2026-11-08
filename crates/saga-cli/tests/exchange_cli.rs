//! Phase 4 CLI verification: N REAL `saga` processes (the built binary,
//! `std::process::Command`), each a shard of the same guarded finance run,
//! exchanging per-block state deltas through a shared --exchange-dir, must
//! together reproduce the single-machine guarded run exactly:
//!   * the union of the shard processes' edge CSVs equals the single run's
//!     edges (logical tuples, full rows), and
//!   * each shard process's nodes.csv equals the single run's rows for the
//!     ids that shard owns, line for line, and
//!   * both agree with the `sim::shard_equivalence` emulation.
//! Plus: a lone shard times out with a clear error (never hangs), and the
//! rail refuses guarded sharding without --exchange-dir by naming the flag.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_saga");

/// Small guarded finance config; must stay in lockstep with `core_params`.
const NODES: u64 = 20_000;
const EDGES: u64 = 200_000;
const SPAN: f64 = 14.0;
const SEED: u64 = 42;

fn core_params(guards: bool) -> saga_core::GenParams {
    saga_core::GenParams {
        rulebase: saga_rules::load_builtin_rulebase("finance").unwrap(),
        nodes: NODES,
        target_edges: EDGES,
        span_days: SPAN,
        granularity_s: 1,
        epoch_unix: 1_735_689_600, // the config scaffold default
        seed: SEED,
        threads: 2,
        block_len_days: 1.0,
        guards_enabled: guards,
        state_behavior: false,
        anomaly_rate: None,
        anomaly_difficulty: None,
        anomaly_type_mix: Vec::new(),
        anomaly_cascade: None,
        anomaly_communities: None,
        anomalies_disabled: true,
        shard_index: 0,
        shard_count: 1,
        exchange_dir: None,
        exchange_timeout_s: 300,
        calibrate_passes: 0,
        node_dynamics: false,
        state_trajectory: false,
    }
}

fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "agora_xcli_{}_{}_{}",
        std::process::id(),
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn gen_cmd(out: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.args([
        "generate",
        "--domain",
        "finance",
        "--nodes",
        &NODES.to_string(),
        "--edges",
        &EDGES.to_string(),
        "--time-span",
        &SPAN.to_string(),
        "--seed",
        &SEED.to_string(),
        "--no-anomalies",
        "--guards",
        "--threads",
        "2",
        "--format",
        "csv",
        "--quiet",
        "--out",
    ]);
    c.arg(out);
    c
}

/// All data lines (headers stripped) of every edges_*.csv under `dir`.
fn edge_lines(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("edges_") && n.ends_with(".csv"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no edge CSVs in {}", dir.display());
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        out.extend(text.lines().skip(1).map(str::to_string));
    }
    out
}

/// All data lines of nodes.csv under `dir` (header stripped).
fn node_lines(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join("nodes.csv")).unwrap();
    text.lines().skip(1).map(str::to_string).collect()
}

fn node_id(line: &str) -> u64 {
    line.split(',').next().unwrap().parse().unwrap()
}

/// Render a state value exactly as the CSV node writer does.
fn fmt_state(x: f64) -> String {
    if x.is_nan() {
        String::new()
    } else if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x:.4}")
    }
}

#[test]
fn real_processes_equal_single_machine_and_emulation() {
    let root = fresh_dir("equiv");

    // Single-machine guarded reference.
    let single_out = root.join("single");
    let st = gen_cmd(&single_out).output().unwrap();
    assert!(st.status.success(), "single run failed: {}", String::from_utf8_lossy(&st.stderr));
    let mut single_edges = edge_lines(&single_out);
    single_edges.sort_unstable();
    let single_nodes = node_lines(&single_out);

    for &n in &[2u64, 4] {
        let xdir = root.join(format!("xchg_{n}"));
        let out = root.join(format!("sharded_{n}"));
        // N REAL concurrent shard processes of the built binary.
        let children: Vec<_> = (0..n)
            .map(|k| {
                let mut c = gen_cmd(&out);
                c.args([
                    "--shard-count",
                    &n.to_string(),
                    "--shard-index",
                    &k.to_string(),
                    "--exchange-timeout-s",
                    "120",
                    "--exchange-dir",
                ]);
                c.arg(&xdir);
                c.spawn().unwrap()
            })
            .collect();
        for (k, child) in children.into_iter().enumerate() {
            let o = child.wait_with_output().unwrap();
            assert!(
                o.status.success(),
                "N={n} shard {k} failed: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        }

        // (1) Edge union (logical tuples) == single machine.
        let mut union: Vec<String> = Vec::new();
        for k in 0..n {
            union.extend(edge_lines(&out.join(format!("shard_{k:02}"))));
        }
        union.sort_unstable();
        assert_eq!(
            union.len(),
            single_edges.len(),
            "N={n}: union kept {} edges, single machine {}",
            union.len(),
            single_edges.len()
        );
        assert_eq!(union, single_edges, "N={n}: edge union != single machine");

        // (2) Node state: each shard's nodes.csv == the single run's rows for
        //     the ids it owns, line for line (same order, same rendering).
        for k in 0..n {
            let shard_rows = node_lines(&out.join(format!("shard_{k:02}")));
            let expect: Vec<String> = single_nodes
                .iter()
                .filter(|l| node_id(l) % n == k)
                .cloned()
                .collect();
            assert_eq!(
                shard_rows, expect,
                "N={n}: shard {k} nodes.csv != single-machine slice"
            );
        }

        // (3) The emulation agrees with the real processes.
        let eq = saga_core::sim::shard_equivalence(&core_params(true), n).unwrap();
        assert!(eq.edges_match() && eq.state_matches(), "N={n}: emulation self-check");
        assert_eq!(
            eq.whole_edges.len(),
            single_edges.len(),
            "N={n}: emulation edge count != real CSV edge count"
        );
        // Logical edge tuples: parse the CSV back to the emulation's key.
        let params = core_params(true);
        let et_index: HashMap<&str, u16> = params
            .rulebase
            .event_types
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.as_str(), i as u16))
            .collect();
        let mut csv_keys: Vec<(u64, u64, i64, u16, u16, i64)> = single_edges
            .iter()
            .map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                assert_eq!(f[4], "normal", "anomalies are disabled in this config");
                (
                    f[0].parse().unwrap(),
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    et_index[f[3]],
                    0u16,
                    f[5].parse().unwrap(),
                )
            })
            .collect();
        csv_keys.sort_unstable();
        assert_eq!(csv_keys, eq.whole_edges, "N={n}: CSV edges != emulation edges");
        // Node state values: each shard row's trailing state columns must
        // render exactly the emulation's per-node state.
        let starts: HashMap<&str, u64> = eq
            .entity_names
            .iter()
            .enumerate()
            .map(|(e, name)| {
                let start = single_nodes
                    .iter()
                    .filter(|l| l.split(',').nth(1).unwrap() == name)
                    .map(|l| node_id(l))
                    .min()
                    .unwrap_or_else(|| panic!("entity {name} missing from nodes.csv"));
                let _ = e;
                (name.as_str(), start)
            })
            .collect();
        for k in 0..n {
            for line in node_lines(&out.join(format!("shard_{k:02}"))) {
                let f: Vec<&str> = line.split(',').collect();
                let (id, ename) = (f[0].parse::<u64>().unwrap(), f[1]);
                let e = eq.entity_names.iter().position(|x| x == ename).unwrap();
                let n_state = eq.state_names[e].len();
                if n_state == 0 {
                    continue;
                }
                let local = (id - starts[ename]) as usize;
                let state_cols = &f[f.len() - n_state..];
                for (v, got) in state_cols.iter().enumerate() {
                    let want = fmt_state(eq.shard_state[e][v][local]);
                    assert_eq!(
                        *got, want,
                        "N={n} shard {k}: node {ename}/{id} state var {} differs",
                        eq.state_names[e][v]
                    );
                }
            }
        }

        // (4) No exchange files left behind.
        let leftovers = std::fs::read_dir(&xdir).map(|rd| rd.count()).unwrap_or(0);
        assert_eq!(leftovers, 0, "N={n}: {leftovers} exchange files left behind");
        println!("N={n}: {} edges, real processes == single machine == emulation", union.len());
    }
    std::fs::remove_dir_all(&root).unwrap();
}

/// A lone shard of an N=2 run must exit with the timeout error quickly, not
/// hang forever waiting for its missing peer.
#[test]
fn lone_shard_times_out() {
    let root = fresh_dir("timeout");
    let mut c = gen_cmd(&root.join("out"));
    c.args(["--shard-count", "2", "--shard-index", "0", "--exchange-timeout-s", "2", "--exchange-dir"]);
    c.arg(root.join("xchg"));
    let mut child = c.stderr(std::process::Stdio::piped()).spawn().unwrap();
    // Watchdog: the whole point is that this must not hang.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match child.try_wait().unwrap() {
            Some(_) => break,
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                panic!("lone shard hung past the watchdog instead of timing out");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    let o = child.wait_with_output().unwrap();
    assert!(!o.status.success(), "lone shard must fail");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("timed out") && err.contains("peer shard 1"),
        "expected a clear timeout error, got: {err}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// The rail: --guards --shard-count 2 without --exchange-dir errors and the
/// message names --exchange-dir.
#[test]
fn rail_names_exchange_dir() {
    let root = fresh_dir("rail");
    let mut c = gen_cmd(&root.join("out"));
    c.args(["--shard-count", "2", "--shard-index", "0"]);
    let o = c.output().unwrap();
    assert!(!o.status.success(), "guarded sharding without exchange dir must be refused");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("--exchange-dir"), "rail must name --exchange-dir: {err}");
    // Nothing may have been generated.
    assert!(!root.join("out").exists(), "rail must fire before any output is created");
    std::fs::remove_dir_all(&root).unwrap();
}
