//! Phase 3 measurement: per-block cross-shard exchange volume vs the block
//! length L (consistency model §4 + §10), N = 4 virtual shards, finance,
//! seed 42, guards ON, 10k nodes / 1M target edges / 30-day span (the
//! headline-experiment 1M scale). Measurement only; run with
//!   cargo test --release --test phase3_comm -- --ignored --nocapture
//! No shipped code paths are touched.

use saga_core::sim::shard_equivalence;
use saga_core::GenParams;
use saga_rules::load_builtin_rulebase;

fn params(block_len_days: f64) -> GenParams {
    GenParams {
        rulebase: load_builtin_rulebase("finance").unwrap(),
        nodes: 10_000,
        target_edges: 1_000_000,
        span_days: 30.0,
        granularity_s: 1,
        epoch_unix: 1_735_689_600,
        seed: 42,
        threads: 8,
        block_len_days,
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

#[test]
#[ignore = "Phase 3 measurement, run explicitly with --ignored --nocapture"]
fn comm_volume_vs_block_length() {
    println!("L_days,n_blocks,total_deltas,mean_per_block,peak_per_block,total_MB,shipped_frac,edges_match,state_match");
    for l in [0.05, 0.25, 1.0, 2.0, 7.0, 30.0] {
        let eq = shard_equivalence(&params(l), 4).unwrap();
        let c = &eq.comm;
        let n_blocks = c.cross_shard_deltas_per_block.len();
        let peak = c.cross_shard_deltas_per_block.iter().max().copied().unwrap_or(0);
        let mean = c.total_cross_shard_deltas as f64 / n_blocks.max(1) as f64;
        println!(
            "{l},{n_blocks},{},{mean:.1},{peak},{:.2},{:.4},{},{}",
            c.total_cross_shard_deltas,
            c.total_bytes as f64 / 1e6,
            c.total_cross_shard_deltas as f64 / c.total_effect_deltas.max(1) as f64,
            eq.edges_match(),
            eq.state_matches(),
        );
    }
}
