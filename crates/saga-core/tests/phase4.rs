//! Phase 4 (real multi-process per-block exchange, consistency model §4) -
//! engine-level verification on the REAL `sim::run` path.
//!
//! Unlike phase2.rs (which drives the single-process EMULATION), every run
//! here goes through the production `generate` entry point with real shard
//! params (`shard_index`/`shard_count`) and a shared exchange directory, so
//! the filesystem barrier protocol itself (delta files, atomic rename,
//! polling, canonical merge, cleanup) is what is being proven. The N shard
//! runs execute CONCURRENTLY (they must: each blocks on its peers' per-block
//! delta files), one OS thread per shard, each with its own rayon pool -
//! exactly the code path N separate `saga` processes execute; the CLI-level
//! suite (crates/saga-cli/tests/exchange_cli.rs) repeats the proof with real
//! subprocesses of the built binary.
//!
//! Proven for guards ON, finance, N in {2, 4}:
//!   * the union of the shard runs' edges equals the single-machine guarded
//!     run's edges (full-row multiset, attrs by f64 bits),
//!   * every shard's exported node state equals the single-machine run's
//!     slice BITWISE (the §4 claim: the exchange completes each owner's
//!     state), and
//!   * both agree with the `sim::shard_equivalence` emulation's result.
//! Also proven: guards OFF with the exchange completes the node state (the
//! documented no-exchange dst-effect caveat disappears), and a missing peer
//! turns into a clear timeout error instead of a hang.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use common::*;
use saga_core::sim::shard_equivalence;
use saga_core::world::World;
use saga_core::{generate, NodeColumn};

/// Small guarded finance config (no anomalies), the phase2 test scale.
fn base_params(guards: bool) -> saga_core::GenParams {
    finance_params(20_000, 200_000, 14.0, 42, 2, 1.0, guards, false)
}

/// A fresh, unique exchange directory under the system temp dir.
fn fresh_exchange_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "agora_phase4_{}_{}_{}",
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

/// Run all N shards of `base` concurrently on the REAL engine path, sharing
/// one exchange dir; returns each shard's captured output.
fn run_shards(
    base: &saga_core::GenParams,
    n: u64,
    dir: &std::path::Path,
) -> Vec<(CaptureSink, saga_core::GenSummary)> {
    let handles: Vec<_> = (0..n)
        .map(|k| {
            let mut p = base.clone();
            p.shard_index = k;
            p.shard_count = n;
            p.exchange_dir = Some(dir.to_path_buf());
            p.exchange_timeout_s = 120;
            std::thread::spawn(move || run_capture(&p))
        })
        .collect();
    handles.into_iter().map(|h| h.join().expect("shard run panicked")).collect()
}

/// Per-shard slice of a whole-graph node-batch list: only ids owned by shard
/// `k` of `n`, with every column filtered in step.
fn node_rows(nodes: &[saga_core::NodeBatch]) -> HashMap<(String, u64), Vec<u64>> {
    let mut out = HashMap::new();
    for nb in nodes {
        for (i, &id) in nb.ids.iter().enumerate() {
            let mut row = Vec::new();
            for col in &nb.attrs {
                match col {
                    NodeColumn::Numeric(v) => row.push(v[i].to_bits()),
                    NodeColumn::Category { codes, .. } => row.push(codes[i] as u64),
                }
            }
            out.insert((nb.entity_type.clone(), id), row);
        }
    }
    out
}

/// The real N-shard exchange run equals the single-machine run AND the
/// emulation, guards ON: edges as a full-row multiset, node state bitwise.
#[test]
fn real_exchange_matches_single_machine_and_emulation() {
    let base = base_params(true);
    let (single, single_summary) = run_capture(&base);
    let world = World::build(&base).unwrap();
    let single_state = exported_state(&world, &single.nodes);
    let single_rows = row_multiset(&single.rows, None);
    let single_nodes = node_rows(&single.nodes);

    for &n in &[2u64, 4] {
        let dir = fresh_exchange_dir(&format!("g_on_{n}"));
        let shards = run_shards(&base, n, &dir);

        // (1) Edges: the union of the shard runs == the single-machine run.
        let mut union: Vec<RowKey> = Vec::new();
        for (cap, _) in &shards {
            union.extend((0..cap.rows.len()).map(|i| row_key(&cap.rows, i)));
        }
        union.sort_unstable();
        assert_eq!(
            union.len(),
            single_rows.len(),
            "N={n}: sharded union kept {} edges, single machine {}",
            union.len(),
            single_rows.len()
        );
        assert_eq!(union, single_rows, "N={n}: sharded edge union != single machine");

        // (2) Node state: every shard's export is exactly the single-machine
        //     slice of the nodes it owns, bitwise, and the shard slices
        //     together cover every node exactly once.
        let mut covered = 0usize;
        for (k, (cap, summary)) in shards.iter().enumerate() {
            let rows = node_rows(&cap.nodes);
            for ((ename, id), row) in &rows {
                assert_eq!(
                    id % n,
                    k as u64,
                    "N={n}: shard {k} exported node {id} it does not own"
                );
                assert_eq!(
                    Some(row),
                    single_nodes.get(&(ename.clone(), *id)).as_deref(),
                    "N={n}: shard {k} node {ename}/{id} state != single machine"
                );
            }
            covered += rows.len();
            // The telemetry says the exchange really ran: one round per block.
            assert_eq!(
                summary.exchange_rounds, 14,
                "N={n}: shard {k} should run one barrier round per L-block"
            );
            if n > 1 {
                assert!(
                    summary.exchange_deltas_received > 0,
                    "N={n}: shard {k} received no deltas (exchange not exercised?)"
                );
            }
        }
        assert_eq!(covered, single_nodes.len(), "N={n}: shard node slices must tile the graph");

        // (3) Equality with the Phase-2 emulation on the same params.
        let eq = shard_equivalence(&base, n).unwrap();
        assert!(eq.edges_match() && eq.state_matches(), "N={n}: emulation self-check");
        assert_eq!(
            eq.whole_edges.len(),
            single_rows.len(),
            "N={n}: emulation kept a different edge count than the real engine"
        );
        let mut engine_keys: Vec<_> = (0..single.rows.len())
            .map(|i| {
                (
                    single.rows.src[i],
                    single.rows.dst[i],
                    single.rows.t[i],
                    single.rows.event_type[i],
                    single.rows.label[i],
                    single.rows.anomaly_id[i],
                )
            })
            .collect();
        engine_keys.sort_unstable();
        assert_eq!(engine_keys, eq.whole_edges, "N={n}: emulation edges != real engine edges");
        assert_eq!(
            saga_core::sim::states_bit_equal(&single_state, &eq.shard_state),
            true,
            "N={n}: emulation sharded state != real engine state"
        );

        // The exchange dir must not accumulate files: every delta file was
        // consumed by its one reader.
        let leftovers = std::fs::read_dir(&dir)
            .map(|rd| rd.count())
            .unwrap_or(0);
        assert_eq!(leftovers, 0, "N={n}: {leftovers} exchange files left behind");
        let _ = std::fs::remove_dir_all(&dir);

        let comm = &eq.comm;
        println!(
            "N={n}: edges {} == single {} ; rounds {} ; emulation exchange volume {} deltas",
            union.len(),
            single_rows.len(),
            shards[0].1.exchange_rounds,
            comm.total_cross_shard_deltas,
        );
    }
    assert!(single_summary.overdraft_guarded > 0, "guards must have been live");
}

/// Guards OFF + exchange: each shard's exported node state is COMPLETE (equal
/// to the single-machine run's slice, bitwise) - the exchange fixes the
/// documented no-exchange dst-effect caveat - while edges stay the proven
/// union. The no-exchange guards-off mode is untouched (engine.rs covers it).
#[test]
fn guards_off_exchange_completes_node_state() {
    let base = base_params(false);
    let (single, _) = run_capture(&base);
    let single_rows = row_multiset(&single.rows, None);
    let single_nodes = node_rows(&single.nodes);

    let n = 2u64;
    let dir = fresh_exchange_dir("g_off");
    let shards = run_shards(&base, n, &dir);
    let mut union: Vec<RowKey> = Vec::new();
    for (cap, _) in &shards {
        union.extend((0..cap.rows.len()).map(|i| row_key(&cap.rows, i)));
    }
    union.sort_unstable();
    assert_eq!(union, single_rows, "guards-off exchange changed the edge union");
    for (k, (cap, _)) in shards.iter().enumerate() {
        let rows = node_rows(&cap.nodes);
        for ((ename, id), row) in &rows {
            assert_eq!(
                Some(row),
                single_nodes.get(&(ename.clone(), *id)).as_deref(),
                "shard {k} node {ename}/{id}: guards-off exchanged state != single machine"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A lone shard whose peer never starts must FAIL with the timeout error
/// (naming the missing peer's file), not hang.
#[test]
fn missing_peer_times_out_with_clear_error() {
    let mut p = base_params(true);
    p.shard_index = 0;
    p.shard_count = 2;
    let dir = fresh_exchange_dir("timeout");
    p.exchange_dir = Some(dir.clone());
    p.exchange_timeout_s = 1;

    let started = std::time::Instant::now();
    let mut sink = saga_core::CountingSink::default();
    let err = generate(&p, &mut sink, None).expect_err("lone shard must not succeed");
    let msg = format!("{err}");
    assert!(
        msg.contains("timed out") && msg.contains("peer shard 1"),
        "unexpected timeout error: {msg}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "timeout took too long: {:?}",
        started.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The engine rail: guards + sharding WITHOUT an exchange dir is refused with
/// an error that names --exchange-dir.
#[test]
fn guarded_sharding_without_exchange_dir_is_refused() {
    let mut p = base_params(true);
    p.shard_index = 0;
    p.shard_count = 2;
    let mut sink = saga_core::CountingSink::default();
    let err = generate(&p, &mut sink, None).expect_err("must refuse");
    assert!(
        format!("{err}").contains("--exchange-dir"),
        "rail error must name --exchange-dir: {err}"
    );
}
