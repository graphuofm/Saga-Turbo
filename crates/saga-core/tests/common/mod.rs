//! Shared helpers for the ORACLE cross-validation and edge-case audit tests.
//!
//! The oracle here is an INDEPENDENT sequential reference simulator for
//! Phase B (the block-bounded-staleness sweep). It deliberately does NOT call
//! any of the engine's Phase-B code (BlockSweep, fold_canonical, apply_event
//! are private to sim.rs, so the compiler enforces that): it replays a
//! candidate event stream one event at a time, maintains per-node state
//! directly, evaluates the overdraft guard per event, and applies effects
//! immediately. The candidate stream itself is obtained from the engine's
//! public API by running with guards OFF, which by construction (consistency
//! model section 8: L and guards touch nothing in Phase A) emits every
//! candidate unfiltered, in the exact stream order Phase B consumes.

#![allow(dead_code)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use saga_core::world::{EffectKind, World};
use saga_core::{generate, EventBatch, EventSink, GenParams, GenSummary, NodeBatch, NodeColumn};
use saga_rules::load_builtin_rulebase;

// ---------------------------------------------------------------------------
// Parameter builders
// ---------------------------------------------------------------------------

pub fn finance_params(
    nodes: u64,
    edges: u64,
    span_days: f64,
    seed: u64,
    threads: usize,
    block_len_days: f64,
    guards: bool,
    anomalies: bool,
) -> GenParams {
    domain_params("finance", nodes, edges, span_days, seed, threads, block_len_days, guards, anomalies)
}

/// Same builder for any built-in domain (the guard machinery is
/// domain-generic; finance and crypto both wire guards).
pub fn domain_params(
    domain: &str,
    nodes: u64,
    edges: u64,
    span_days: f64,
    seed: u64,
    threads: usize,
    block_len_days: f64,
    guards: bool,
    anomalies: bool,
) -> GenParams {
    GenParams {
        rulebase: load_builtin_rulebase(domain).unwrap(),
        nodes,
        target_edges: edges,
        span_days,
        granularity_s: 1,
        epoch_unix: 1_735_689_600,
        seed,
        threads,
        block_len_days,
        guards_enabled: guards,
        anomaly_rate: None,
        anomaly_difficulty: None,
        anomaly_type_mix: Vec::new(),
        anomaly_cascade: None,
        anomaly_communities: None,
        anomalies_disabled: !anomalies,
        shard_index: 0,
        shard_count: 1,
        exchange_dir: None,
        exchange_timeout_s: 300,
        calibrate_passes: 0,
    }
}

/// One second expressed in days (the smallest meaningful L).
pub const ONE_SECOND_DAYS: f64 = 1.0 / 86_400.0;

// ---------------------------------------------------------------------------
// Capture sink: the full edge stream (in engine emission order) + node tables
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct CaptureSink {
    pub rows: EventBatch,
    pub nodes: Vec<NodeBatch>,
}

impl EventSink for CaptureSink {
    fn write_batch(&mut self, b: &EventBatch) -> anyhow::Result<()> {
        if self.rows.attr_names.is_empty() {
            self.rows.attr_names = b.attr_names.clone();
            self.rows.attrs = vec![Vec::new(); b.attrs.len()];
        }
        self.rows.src.extend_from_slice(&b.src);
        self.rows.dst.extend_from_slice(&b.dst);
        self.rows.t.extend_from_slice(&b.t);
        self.rows.event_type.extend_from_slice(&b.event_type);
        self.rows.label.extend_from_slice(&b.label);
        self.rows.anomaly_id.extend_from_slice(&b.anomaly_id);
        for (c, col) in b.attrs.iter().enumerate() {
            self.rows.attrs[c].extend_from_slice(col);
        }
        Ok(())
    }
    fn write_nodes(&mut self, b: &NodeBatch) -> anyhow::Result<()> {
        self.nodes.push(b.clone());
        Ok(())
    }
}

/// Run the engine and capture everything.
pub fn run_capture(params: &GenParams) -> (CaptureSink, GenSummary) {
    let mut sink = CaptureSink::default();
    let summary = generate(params, &mut sink, None).expect("engine run failed");
    (sink, summary)
}

/// Extract the exported final node state from captured node batches, shaped
/// exactly like `World::state` ([entity][var][local]). State columns are
/// appended after the node attribute columns, so match names from the END.
pub fn exported_state(world: &World, nodes: &[NodeBatch]) -> Vec<Vec<Vec<f64>>> {
    let mut out: Vec<Vec<Vec<f64>>> = Vec::new();
    for ti in 0..world.entities.names.len() {
        let ename = &world.entities.names[ti];
        let nb = nodes
            .iter()
            .find(|nb| &nb.entity_type == ename)
            .unwrap_or_else(|| panic!("no node batch for entity `{ename}`"));
        let mut vars = Vec::new();
        for vname in &world.state_names[ti] {
            let ci = nb
                .attr_names
                .iter()
                .rposition(|n| n == vname)
                .unwrap_or_else(|| panic!("state var `{vname}` missing in node export"));
            match &nb.attrs[ci] {
                NodeColumn::Numeric(v) => vars.push(v.clone()),
                NodeColumn::Category { .. } => panic!("state var `{vname}` not numeric"),
            }
        }
        out.push(vars);
    }
    out
}

// ---------------------------------------------------------------------------
// The independent oracle
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum OracleMode {
    /// Sequential-exact: every guard reads the CURRENT state (L -> 0).
    PerEvent,
    /// Block-bounded staleness with block length `l_s` seconds: all events in
    /// a block share the block-start snapshot; effects merge at the boundary.
    /// `l_s = 1.0` is the engine at L = granularity (1 s): events in the same
    /// second share a snapshot (the documented same-second-tie semantics).
    Block { l_s: f64 },
}

pub struct OracleOut {
    /// Per candidate row: did the oracle keep (emit) it?
    pub kept: Vec<bool>,
    /// Final per-node state after folding the kept events' effects.
    pub state: Vec<Vec<Vec<f64>>>,
    /// Kept events that were subject to a guard.
    pub guarded_kept: u64,
    /// Candidates suppressed by the guard.
    pub suppressed: u64,
}

/// Replay `cand` (assumed non-decreasing in t, which is asserted) one event at
/// a time in stream order, maintaining per-node state directly.
///
/// Fold order note: the engine's stream is canonically sorted per day by the
/// FINAL (granularity-rounded) `(t, src, dst, event_type)` key, and its state
/// folds use the CANONICAL merge order — stream order, with runs of events
/// sharing a full key folded in the content order `(effect_index, amount
/// bits)`, so the fold is a pure function of the kept delta multiset rather
/// than of tie-order sort artifacts (that is what makes sharded runs
/// bit-reproducible, consistency model Phase 4). The oracle implements the
/// same canonical order independently so state can be compared BITWISE, not
/// just approximately. Guard decisions are order-free within a block (frozen
/// snapshot), so this loses no generality.
pub fn oracle_replay(
    world: &World,
    cand: &EventBatch,
    epoch: i64,
    guards: bool,
    mode: OracleMode,
) -> OracleOut {
    let n = cand.len();
    let mut kept = vec![false; n];
    let mut state = deep_clone(&world.state);
    let mut guarded_kept = 0u64;
    let mut suppressed = 0u64;

    // Own guard evaluation, written from the spec: suppress iff the amount is
    // finite and the src's guarded var cannot cover it (bal < amt; equality
    // passes, i.e. the guard is a >= check on the balance).
    let decide = |state: &[Vec<Vec<f64>>], i: usize| -> bool {
        let ev = &world.events[cand.event_type[i] as usize];
        let Some(g) = &ev.guard else { return true };
        let start = world.entities.starts[g.entity];
        let bal = state[g.entity][g.var][(cand.src[i] - start) as usize];
        let amt = cand.attrs[g.from_col][i];
        !(amt.is_finite() && bal < amt)
    };
    // Apply ONE effect (by index) of event `i` with a pre-resolved amount:
    // Add/Sub skip non-finite amounts, Increment adds 1, Set writes the
    // constant.
    let apply_one = |state: &mut [Vec<Vec<f64>>], i: usize, ei: usize, amount: f64| {
        let ev = &world.events[cand.event_type[i] as usize];
        let eff = &ev.effects[ei];
        let node = if eff.on_src { cand.src[i] } else { cand.dst[i] };
        let local = (node - world.entities.starts[eff.entity]) as usize;
        let slot = &mut state[eff.entity][eff.var][local];
        match eff.kind {
            EffectKind::Add => {
                if amount.is_finite() {
                    *slot += amount;
                }
            }
            EffectKind::Sub => {
                if amount.is_finite() {
                    *slot -= amount;
                }
            }
            EffectKind::Increment => *slot += 1.0,
            EffectKind::Set => *slot = amount,
        }
    };
    // The resolved operand of effect `ei` of event `i`, per rule semantics.
    let amount_of = |i: usize, ei: usize| -> f64 {
        let eff = &world.events[cand.event_type[i] as usize].effects[ei];
        match eff.kind {
            EffectKind::Add | EffectKind::Sub => cand.attrs[eff.from_col][i],
            EffectKind::Increment => 1.0,
            EffectKind::Set => eff.value,
        }
    };
    // Own effect fold in stream order (all effects of one event, in order).
    let apply = |state: &mut [Vec<Vec<f64>>], i: usize| {
        let n_eff = world.events[cand.event_type[i] as usize].effects.len();
        for ei in 0..n_eff {
            apply_one(state, i, ei, amount_of(i, ei));
        }
    };
    // The CANONICAL fold of the kept events in [lo, hi): stream order, except
    // that a run of events sharing the full (t, src, dst, event_type) key
    // folds its effect deltas in the content order (effect_index, amount
    // bits). Written independently from the spec (consistency model §1 as
    // refined by Phase 4).
    let fold_canonical = |state: &mut [Vec<Vec<f64>>], lo: usize, hi: usize, kept: &[bool]| {
        let mut k = lo;
        while k < hi {
            let mut m = k + 1;
            while m < hi
                && cand.t[m] == cand.t[k]
                && cand.src[m] == cand.src[k]
                && cand.dst[m] == cand.dst[k]
                && cand.event_type[m] == cand.event_type[k]
            {
                m += 1;
            }
            let rows: Vec<usize> = (k..m).filter(|&r| kept[r]).collect();
            if rows.len() <= 1 {
                for &r in &rows {
                    apply(state, r);
                }
            } else {
                let n_eff = world.events[cand.event_type[k] as usize].effects.len();
                let mut deltas: Vec<(usize, u64, usize)> = Vec::new();
                for &r in &rows {
                    for ei in 0..n_eff {
                        deltas.push((ei, amount_of(r, ei).to_bits(), r));
                    }
                }
                deltas.sort_by_key(|&(ei, bits, _)| (ei, bits));
                for &(ei, bits, r) in &deltas {
                    apply_one(state, r, ei, f64::from_bits(bits));
                }
            }
            k = m;
        }
    };

    for i in 1..n {
        assert!(cand.t[i] >= cand.t[i - 1], "candidate stream must be time-sorted");
    }

    match mode {
        OracleMode::PerEvent => {
            if guards {
                for i in 0..n {
                    if !decide(&state, i) {
                        suppressed += 1;
                        continue;
                    }
                    if world.events[cand.event_type[i] as usize].guard.is_some() {
                        guarded_kept += 1;
                    }
                    kept[i] = true;
                    apply(&mut state, i);
                }
            } else {
                // Pure fold, no decisions: the canonical order applies (the
                // guards-off engine folds canonically too).
                for k in kept.iter_mut() {
                    *k = true;
                }
                fold_canonical(&mut state, 0, n, &kept);
            }
        }
        OracleMode::Block { l_s } => {
            assert!(l_s >= 1.0, "oracle block length below 1 s makes no sense");
            let block_of = |t: i64| ((t - epoch) as f64 / l_s).floor() as i64;
            let mut i = 0usize;
            while i < n {
                let blk = block_of(cand.t[i]);
                let mut j = i;
                while j < n && block_of(cand.t[j]) == blk {
                    j += 1;
                }
                // Two straightforward passes: decide every event in the block
                // against the UNTOUCHED state (the block-start snapshot),
                // then fold the kept events' effects in the canonical order.
                for k in i..j {
                    if guards && !decide(&state, k) {
                        suppressed += 1;
                        continue;
                    }
                    if guards && world.events[cand.event_type[k] as usize].guard.is_some() {
                        guarded_kept += 1;
                    }
                    kept[k] = true;
                }
                fold_canonical(&mut state, i, j, &kept);
                i = j;
            }
        }
    }

    OracleOut { kept, state, guarded_kept, suppressed }
}

fn deep_clone(s: &[Vec<Vec<f64>>]) -> Vec<Vec<Vec<f64>>> {
    s.to_vec()
}

// ---------------------------------------------------------------------------
// Comparison helpers
// ---------------------------------------------------------------------------

/// Full-row identity for multiset comparison: every column, attrs by f64 bits.
pub type RowKey = (i64, u64, u64, u16, u16, i64, Vec<u64>);

pub fn row_key(b: &EventBatch, i: usize) -> RowKey {
    (
        b.t[i],
        b.src[i],
        b.dst[i],
        b.event_type[i],
        b.label[i],
        b.anomaly_id[i],
        b.attrs.iter().map(|c| c[i].to_bits()).collect(),
    )
}

/// Sorted multiset of all rows (or of the kept subset when `keep` is given).
pub fn row_multiset(b: &EventBatch, keep: Option<&[bool]>) -> Vec<RowKey> {
    let mut v: Vec<RowKey> = (0..b.len())
        .filter(|&i| keep.is_none_or(|k| k[i]))
        .map(|i| row_key(b, i))
        .collect();
    v.sort_unstable();
    v
}

/// Count state cells that differ BITWISE between two state arrays.
pub fn state_mismatches(a: &[Vec<Vec<f64>>], b: &[Vec<Vec<f64>>]) -> usize {
    let mut n = 0;
    assert_eq!(a.len(), b.len(), "entity count differs");
    for (ea, eb) in a.iter().zip(b) {
        assert_eq!(ea.len(), eb.len(), "state var count differs");
        for (va, vb) in ea.iter().zip(eb) {
            assert_eq!(va.len(), vb.len(), "node count differs");
            for (x, y) in va.iter().zip(vb) {
                if x.to_bits() != y.to_bits() {
                    n += 1;
                }
            }
        }
    }
    n
}

// ---------------------------------------------------------------------------
// Hash sink (bit-identity checks without materializing rows)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct HashSink {
    edges: DefaultHasher,
    nodes: DefaultHasher,
    pub n_edges: u64,
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
            b.anomaly_id[i].hash(&mut self.edges);
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
    pub fn edge_hash(&self) -> u64 {
        self.edges.finish()
    }
    pub fn node_hash(&self) -> u64 {
        self.nodes.finish()
    }
}

/// Run and return (edge_hash, node_hash, summary).
pub fn run_hashes(params: &GenParams) -> (u64, u64, GenSummary) {
    let mut s = HashSink::default();
    let summary = generate(params, &mut s, None).expect("engine run failed");
    (s.edge_hash(), s.node_hash(), summary)
}
