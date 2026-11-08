//! Phase 1 (block-bounded staleness) verification: RNG/L decoupling, per-L
//! determinism across thread counts (edges AND node state), and the
//! overdraft-vs-L mechanism curve. All runs are real seed-42 finance runs.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use saga_core::{generate, EventBatch, EventSink, GenParams, NodeBatch, NodeColumn};
use saga_rules::load_builtin_rulebase;

/// Captures the full edge stream and the final node state, so both can be
/// hashed for a bit-identical comparison.
#[derive(Default)]
struct HashSink {
    edges: DefaultHasher,
    nodes: DefaultHasher,
    n_edges: u64,
}

impl EventSink for HashSink {
    fn write_batch(&mut self, b: &EventBatch) -> anyhow::Result<()> {
        self.n_edges += b.len() as u64;
        for i in 0..b.len() {
            b.src[i].hash(&mut self.edges);
            b.dst[i].hash(&mut self.edges);
            b.t[i].hash(&mut self.edges);
            b.event_type[i].hash(&mut self.edges);
            b.label[i].hash(&mut self.edges);
            for col in &b.attrs {
                col[i].to_bits().hash(&mut self.edges);
            }
        }
        Ok(())
    }
    fn write_nodes(&mut self, b: &NodeBatch) -> anyhow::Result<()> {
        b.entity_type.hash(&mut self.nodes);
        for (ci, col) in b.attrs.iter().enumerate() {
            b.attr_names[ci].hash(&mut self.nodes);
            match col {
                NodeColumn::Numeric(v) => {
                    for &x in v {
                        x.to_bits().hash(&mut self.nodes);
                    }
                }
                NodeColumn::Category { codes, .. } => codes.hash(&mut self.nodes),
            }
        }
        Ok(())
    }
}

impl HashSink {
    fn edge_hash(&self) -> u64 {
        self.edges.finish()
    }
    fn node_hash(&self) -> u64 {
        self.nodes.finish()
    }
}

fn params(threads: usize, block_len_days: f64, guards: bool) -> GenParams {
    GenParams {
        rulebase: load_builtin_rulebase("finance").unwrap(),
        nodes: 40_000,
        target_edges: 1_000_000,
        span_days: 14.0,
        granularity_s: 1,
        epoch_unix: 1_735_689_600,
        seed: 42,
        threads,
        block_len_days,
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

const SPAN: f64 = 14.0;

/// Determinism per fixed L: threads 1 vs 8 must produce bit-identical edges AND
/// node state, for sub-day, day, and multi-day L.
#[test]
fn determinism_across_threads_per_l() {
    for &l in &[0.25, 1.0, 4.0] {
        let mut a = HashSink::default();
        let mut b = HashSink::default();
        generate(&params(1, l, true), &mut a, None).unwrap();
        generate(&params(8, l, true), &mut b, None).unwrap();
        assert_eq!(a.edge_hash(), b.edge_hash(), "L={l}: edges differ across threads");
        assert_eq!(a.node_hash(), b.node_hash(), "L={l}: node state differs across threads");
        println!(
            "L={l:<4} edges=0x{:016x} nodes=0x{:016x} n={}",
            a.edge_hash(),
            a.node_hash(),
            a.n_edges
        );
    }
}

/// Decoupling proof: with guards OFF, two different L must give bit-identical
/// edges (L touches output only via guards, never via RNG re-keying).
#[test]
fn guards_off_is_l_independent() {
    let ls = [0.5, 2.0, SPAN];
    let mut hashes = Vec::new();
    for &l in &ls {
        let mut s = HashSink::default();
        generate(&params(8, l, false), &mut s, None).unwrap();
        hashes.push((l, s.edge_hash(), s.node_hash()));
    }
    for w in hashes.windows(2) {
        assert_eq!(w[0].1, w[1].1, "guards off: edges depend on L ({} vs {})", w[0].0, w[1].0);
        assert_eq!(w[0].2, w[1].2, "guards off: node state depends on L");
    }
    println!("guards off: edges 0x{:016x} identical for L in {ls:?}", hashes[0].1);
}

/// The overdraft-vs-L mechanism curve: with the guard ON, the violation rate
/// must rise with L (peak at L=span) and fall toward 0 as L -> 0.
#[test]
fn overdraft_rate_grows_with_l() {
    let ls = [0.001_f64, 0.25, 1.0, 4.0, SPAN];
    let mut rates = Vec::new();
    for &l in &ls {
        let mut s = HashSink::default();
        let summary = generate(&params(8, l, true), &mut s, None).unwrap();
        let rate = summary.overdraft_violations as f64 / summary.overdraft_guarded.max(1) as f64;
        println!(
            "L={l:<7} guarded={} violations={} rate={:.4}%",
            summary.overdraft_guarded,
            summary.overdraft_violations,
            100.0 * rate
        );
        rates.push((l, rate));
    }
    // Monotone non-decreasing in L.
    for w in rates.windows(2) {
        assert!(
            w[1].1 >= w[0].1 - 1e-9,
            "violation rate not monotone in L: {:?} -> {:?}",
            w[0],
            w[1]
        );
    }
    // Smallest L is near sequential-exact (few overdrafts); L=span is the max.
    assert!(rates[0].1 < 0.01, "L->0 should be near-zero overdrafts, got {:.4}", rates[0].1);
    assert!(
        rates.last().unwrap().1 > rates[0].1,
        "L=span must have strictly more overdrafts than L->0"
    );
}

/// L=span reproduces option A (reads see only the initial state S_0): with the
/// guard on and one block for the whole run, every guard read is against S_0,
/// so exactly the transfers whose INITIAL balance cannot cover the amount are
/// suppressed. Cross-check that guards-off L=span matches the default engine
/// path (L=1 day) bit-for-bit.
#[test]
fn l_span_guards_off_matches_default() {
    let mut span = HashSink::default();
    let mut day = HashSink::default();
    generate(&params(8, SPAN, false), &mut span, None).unwrap();
    generate(&params(8, 1.0, false), &mut day, None).unwrap();
    assert_eq!(span.edge_hash(), day.edge_hash(), "guards-off output must be L-invariant");
    assert_eq!(span.node_hash(), day.node_hash());
}
