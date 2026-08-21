//! Phase 2 (distributed equivalence, consistency model §4) verification.
//!
//! With state feedback live (Phase 1), the sharded run is only CORRECT if a
//! node's owner receives the dst-effects of its cross-shard in-edges each block
//! (§4). This suite drives the single-process N-shard emulation and proves, for
//! guards ON, finance, N in {2,4}:
//!   * the sharded run's EDGE multiset equals the single-machine whole-graph
//!     run's, and
//!   * every per-node final STATE value equals the whole-graph run's (logical,
//!     compared by f64 bits, not raw parquet bytes),
//! then measures the per-block cross-shard dst-effect exchange volume, and
//! demonstrates that WITHOUT the exchange (the old path) node state (and, via
//! the stale guard, edges) DIVERGE — so the exchange is necessary.

use saga_core::sim::shard_equivalence;
use saga_core::{generate, GenParams, NodeBatch, NodeColumn};
use saga_rules::load_builtin_rulebase;

fn params(nodes: u64, edges: u64, threads: usize) -> GenParams {
    GenParams {
        rulebase: load_builtin_rulebase("finance").unwrap(),
        nodes,
        target_edges: edges,
        span_days: 14.0,
        granularity_s: 1,
        epoch_unix: 1_735_689_600,
        seed: 42,
        threads,
        block_len_days: 1.0,
        guards_enabled: true,
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
    }
}

/// Capture the whole-graph run's exported per-node state from the real engine
/// path (`generate`), keyed by (entity_type, state-var name) -> values. Used to
/// tie the emulation's N==1 reference to the ACTUAL production engine output.
#[derive(Default)]
struct StateSink {
    // (entity_type, attr_name) -> column, only for numeric columns.
    cols: std::collections::HashMap<(String, String), Vec<f64>>,
}
impl saga_core::EventSink for StateSink {
    fn write_batch(&mut self, _b: &saga_core::EventBatch) -> anyhow::Result<()> {
        Ok(())
    }
    fn write_nodes(&mut self, b: &NodeBatch) -> anyhow::Result<()> {
        for (ci, col) in b.attrs.iter().enumerate() {
            if let NodeColumn::Numeric(v) = col {
                self.cols
                    .insert((b.entity_type.clone(), b.attr_names[ci].clone()), v.clone());
            }
        }
        Ok(())
    }
}

/// THE missing test: with guards ON, the sharded run's edges AND per-node final
/// state must equal the single-machine whole-graph run's, logically, for N in
/// {2,4}. Also cross-checks that the emulation's whole-graph reference matches
/// the real `generate` engine path, and reports the exchange volume.
#[test]
fn sharded_state_equals_whole_graph() {
    for &shard_count in &[2u64, 4] {
        let p = params(20_000, 200_000, 8);
        let eq = shard_equivalence(&p, shard_count).unwrap();

        // (1) Edges: sharded-with-exchange == whole-graph, exactly.
        assert_eq!(
            eq.whole_edges.len(),
            eq.shard_edges.len(),
            "N={shard_count}: edge counts differ (whole {} vs sharded {})",
            eq.whole_edges.len(),
            eq.shard_edges.len()
        );
        assert!(eq.edges_match(), "N={shard_count}: sharded edge multiset != whole-graph");

        // (2) Per-node final state: every state var, every node, bit-equal.
        assert!(
            eq.state_matches(),
            "N={shard_count}: sharded per-node state != whole-graph (with exchange)"
        );

        // (3) The emulation's whole-graph reference must equal the REAL engine
        //     (`generate`) exported node state, tying the proof to production.
        let mut sink = StateSink::default();
        generate(&p, &mut sink, None).unwrap();
        for (ety, ename) in eq.entity_names.iter().enumerate() {
            for (vi, vname) in eq.state_names[ety].iter().enumerate() {
                let real = sink
                    .cols
                    .get(&(ename.clone(), vname.clone()))
                    .unwrap_or_else(|| panic!("engine missing state {ename}.{vname}"));
                let emu = &eq.whole_state[ety][vi];
                assert_eq!(real.len(), emu.len(), "{ename}.{vname}: length differs");
                for (a, b) in real.iter().zip(emu.iter()) {
                    assert_eq!(
                        a.to_bits(),
                        b.to_bits(),
                        "{ename}.{vname}: emulation reference != real engine export"
                    );
                }
            }
        }

        // (4) Communication: report the per-block and total cross-shard volume.
        let c = &eq.comm;
        let n_blocks = c.cross_shard_deltas_per_block.len();
        let mean = c.total_cross_shard_deltas as f64 / n_blocks.max(1) as f64;
        let peak = c.cross_shard_deltas_per_block.iter().copied().max().unwrap_or(0);
        println!(
            "N={shard_count}: edges={} blocks={} exchange: total={} deltas ({} bytes @ {} B/delta) \
             mean={:.0}/block peak={}/block shipped_frac={:.3}",
            eq.shard_edges.len(),
            n_blocks,
            c.total_cross_shard_deltas,
            c.total_bytes,
            saga_core::sim::SHARD_DELTA_BYTES,
            mean,
            peak,
            c.total_cross_shard_deltas as f64 / c.total_effect_deltas.max(1) as f64,
        );

        // (5) Necessity: WITHOUT the exchange the node state DIVERGES (the bug),
        //     and the stale guard even changes which edges are emitted.
        let bug = eq.no_exchange_state_mismatches();
        assert!(
            bug > 0,
            "N={shard_count}: no-exchange state must differ from whole-graph (else \
             the exchange would be unnecessary)"
        );
        let edges_diverge = eq.no_exchange_edges != eq.whole_edges;
        println!(
            "N={shard_count}: WITHOUT exchange -> {bug} node-state cells wrong, \
             edges_diverge={edges_diverge} (old_edges={} vs whole={})",
            eq.no_exchange_edges.len(),
            eq.whole_edges.len()
        );
    }
}

/// Larger-scale (1M-edge) equivalence + volume evidence, guards ON, N in {2,4}.
/// `#[ignore]`d so the default suite stays fast; run with
/// `cargo test --release -p saga-core --test phase2 -- --ignored --nocapture`.
#[test]
#[ignore]
fn sharded_state_equals_whole_graph_1m() {
    for &shard_count in &[2u64, 4] {
        let p = params(40_000, 1_000_000, 8);
        let eq = shard_equivalence(&p, shard_count).unwrap();
        assert_eq!(eq.whole_edges.len(), eq.shard_edges.len());
        assert!(eq.edges_match(), "N={shard_count}: 1M edges differ");
        assert!(eq.state_matches(), "N={shard_count}: 1M per-node state differs");
        let c = &eq.comm;
        let n_blocks = c.cross_shard_deltas_per_block.len();
        println!(
            "1M N={shard_count}: edges={} blocks={} exchange total={} deltas ({} bytes) \
             mean={:.0}/block peak={}/block; WITHOUT exchange {} state cells wrong, \
             edges_diverge={}",
            eq.shard_edges.len(),
            n_blocks,
            c.total_cross_shard_deltas,
            c.total_bytes,
            c.total_cross_shard_deltas as f64 / n_blocks.max(1) as f64,
            c.cross_shard_deltas_per_block.iter().copied().max().unwrap_or(0),
            eq.no_exchange_state_mismatches(),
            eq.no_exchange_edges != eq.whole_edges,
        );
    }
}

/// The exchange volume must GROW with N (more shards => more cross-shard
/// in-edges), and be zero for N==1 (nothing to ship).
#[test]
fn exchange_volume_grows_with_shards() {
    let p = params(20_000, 200_000, 8);
    let n1 = shard_equivalence(&p, 1).unwrap();
    assert_eq!(
        n1.comm.total_cross_shard_deltas, 0,
        "N=1 must ship nothing (whole graph is one shard)"
    );
    assert!(n1.edges_match() && n1.state_matches(), "N=1 must be trivially equivalent");

    let mut prev = 0u64;
    for &shard_count in &[2u64, 4, 8] {
        let eq = shard_equivalence(&p, shard_count).unwrap();
        let total = eq.comm.total_cross_shard_deltas;
        assert!(
            total > prev,
            "N={shard_count}: cross-shard volume {total} should exceed N<{shard_count} ({prev})"
        );
        assert!(eq.state_matches(), "N={shard_count}: state must still match with exchange");
        prev = total;
    }
}
