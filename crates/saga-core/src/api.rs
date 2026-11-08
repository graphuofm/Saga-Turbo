//! Public engine API: parameters in, columnar event batches out.

use saga_rules::RuleBase;
use serde::{Deserialize, Serialize};

/// Resolved generation parameters (config + rule base + host decisions).
#[derive(Debug, Clone)]
pub struct GenParams {
    pub rulebase: RuleBase,
    pub nodes: u64,
    /// Target edge/event count; the engine calibrates behavior rates so the
    /// expected total matches this.
    pub target_edges: u64,
    pub span_days: f64,
    pub granularity_s: u64,
    /// Unix seconds of simulated t=0.
    pub epoch_unix: i64,
    pub seed: u64,
    pub threads: usize,
    /// Staleness/merge block length L, in days (consistency model §6, §8).
    /// Behavior in a block reads the state snapshot as of the block start, so a
    /// read is stale by at most L. `L -> 0` is sequential-exact; `L >= span` is
    /// one block for the whole run (option A: every read sees the initial
    /// state). Candidate event draws are keyed L-INDEPENDENTLY (per absolute
    /// day), so L affects output ONLY through state guards, never through RNG
    /// re-keying.
    pub block_len_days: f64,
    /// Enable event state guards (e.g. the finance overdraft guard). When
    /// false, guards are bypassed so the output is independent of L (the RNG/L
    /// decoupling proof). Default off preserves the pre-Phase-1 output.
    pub guards_enabled: bool,
    /// Enable state-conditioned behavior switches (the Phase-B transducer:
    /// thinning and attribute overrides driven by the block-start snapshot).
    /// Off by default, so output is unchanged unless it is requested.
    pub state_behavior: bool,
    /// Anomaly-axis overrides; `None` fields fall back to rule-base control.
    /// Axis 1 (prevalence) and axis 2 (difficulty):
    pub anomaly_rate: Option<f64>,
    pub anomaly_difficulty: Option<f64>,
    /// Axis 3 (type-mix): intent → weight overrides; empty = rule-base default.
    pub anomaly_type_mix: Vec<(String, f64)>,
    /// Axis 5 (cascade): multiplier on per-process cascade probability; `None`
    /// = rule-base default.
    pub anomaly_cascade: Option<f64>,
    /// Axis 4 (placement): override the clustered community count; `None` =
    /// rule-base default placement.
    pub anomaly_communities: Option<u32>,
    pub anomalies_disabled: bool,
    /// Distributed sharding: emit only the edges whose SOURCE node falls in
    /// this shard's id range, `[shard_index, shard_count)` partitioned. The
    /// whole-graph world (all nodes, all skeletons, the full anomaly plan) is
    /// still built identically on every shard — only emission is filtered — so
    /// N independent processes/machines produce disjoint slices that union to
    /// the exact single-process output, bit-for-bit. `shard_count == 1` is the
    /// normal whole-graph run.
    pub shard_index: u64,
    pub shard_count: u64,
    /// Real multi-process sharded runs (consistency model §4, Phase 4): a
    /// shared directory through which the N shard processes exchange their
    /// per-block cross-shard dst-effect deltas (one file per (block, sender,
    /// receiver), write-then-atomic-rename, deleted after folding). Required
    /// when `guards_enabled` with `shard_count > 1`; with guards off it
    /// additionally makes each shard's exported node state complete (the
    /// documented no-exchange dst-effect caveat). `None` = no exchange: the
    /// whole-graph run, or the guards-off edge-only sharded mode.
    pub exchange_dir: Option<std::path::PathBuf>,
    /// Seconds to wait for a peer shard's per-block delta file before failing
    /// with an error (instead of hanging on a dead/missing peer).
    pub exchange_timeout_s: u64,
    /// Guard-aware anomaly calibration (consistency model section 17). Guards
    /// suppress anomalous and normal candidates at DIFFERENT rates (a
    /// structuring-sized transfer exceeds a typical balance more often than a
    /// normal one), so with guards on the realized anomalous-edge fraction
    /// drifts from the planned one. `calibrate_passes > 0` runs that many
    /// measurement passes of the full guarded pipeline into a counting sink
    /// before the real run; each pass measures, per event type, the survival
    /// of campaign candidates against the survival of the background stream
    /// and sets the campaign rate multiplier of that event type to their
    /// ratio (a fixed-point iteration; one pass closes most of the gap). The
    /// multipliers are a pure function of the measured integer tallies, so
    /// the corrected output is deterministic across thread counts and reruns.
    /// 0 (the default) changes nothing: the guards-off and uncorrected
    /// guards-on outputs are byte-identical to the pre-section-17 engine.
    /// Requires `guards_enabled` and a whole-graph run (`shard_count == 1`).
    pub calibrate_passes: u32,
    /// Node dynamics: apply each entity type's declared lifecycle (birth and
    /// death). A node emits no event and is never chosen as a counterparty
    /// outside its lifetime. Birth and death are pure functions of the seed and
    /// the node id, so both theorems hold unchanged. Off by default: a fixed
    /// population, byte-identical to the engine without lifecycles.
    pub node_dynamics: bool,
    /// Emit the node-state trajectory: the initial state, then after every
    /// L-block merge the new value of every state cell the block changed,
    /// stamped with the block end. Each row is a cell of a merged snapshot
    /// S_b, so the table is schedule-independent by Theorem 1. Off by default.
    pub state_trajectory: bool,
}

/// One columnar batch of generated temporal edges.
///
/// Column meaning: edge i is `src[i] --event_type[i]@t[i]--> dst[i]` with
/// `label[i]` the intent id (0 = normal) and `attrs` the per-event-type
/// attribute columns.
#[derive(Debug, Default, Clone)]
pub struct EventBatch {
    /// Global node ids.
    pub src: Vec<u64>,
    pub dst: Vec<u64>,
    /// Unix seconds.
    pub t: Vec<i64>,
    /// Index into [`GenSummary::event_type_names`].
    pub event_type: Vec<u16>,
    /// Intent label: 0 = normal, else index into intent table.
    pub label: Vec<u16>,
    /// Ground-truth anomaly instance id: -1 for normal edges, else the id of
    /// the campaign/incident that caused this edge. Joins to the per-instance
    /// records in [`GenSummary::ground_truth`] (the omniscient causal record:
    /// which campaign, which difficulty, which community — free & exact, §11b).
    pub anomaly_id: Vec<i64>,
    /// Numeric attribute columns, keyed per event type schema; attribute j of
    /// edge i lives at `attrs[j][i]` (NaN where not applicable).
    pub attrs: Vec<Vec<f64>>,
    /// Names of `attrs` columns (union schema across event types).
    pub attr_names: Vec<String>,
}

impl EventBatch {
    pub fn len(&self) -> usize {
        self.src.len()
    }
    pub fn is_empty(&self) -> bool {
        self.src.is_empty()
    }
}

/// Where generated batches go (Parquet/CSV writers, stats, tests).
pub trait EventSink: Send {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()>;
    /// Same as [`EventSink::write_batch`], but the caller gives the batch
    /// away. The engine calls this for every batch it no longer needs (a
    /// guards-off day, a swept block's kept rows), so a sink that hands rows
    /// to another thread can move the buffers instead of copying them
    /// (consistency model section 15). Semantically identical to
    /// `write_batch(&batch)`, which is the default.
    fn write_batch_owned(&mut self, batch: EventBatch) -> anyhow::Result<()> {
        self.write_batch(&batch)
    }
    /// Node tables, emitted once after the event stream (default: ignored).
    fn write_nodes(&mut self, _batch: &NodeBatch) -> anyhow::Result<()> {
        Ok(())
    }
    /// A chunk of an auxiliary table (default: ignored).
    fn write_table(&mut self, _table: &AuxTable) -> anyhow::Result<()> {
        Ok(())
    }
    /// Called once after the last event batch.
    fn finish(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Tee: duplicate the stream into two sinks (writer + stats collector) so
/// generation and introspection stay a single pass (§11b).
pub struct TeeSink<A: EventSink, B: EventSink> {
    pub a: A,
    pub b: B,
}

impl<A: EventSink, B: EventSink> EventSink for TeeSink<A, B> {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()> {
        self.a.write_batch(batch)?;
        self.b.write_batch(batch)
    }
    fn write_batch_owned(&mut self, batch: EventBatch) -> anyhow::Result<()> {
        self.a.write_batch(&batch)?;
        self.b.write_batch_owned(batch)
    }
    fn write_nodes(&mut self, batch: &NodeBatch) -> anyhow::Result<()> {
        self.a.write_nodes(batch)?;
        self.b.write_nodes(batch)
    }
    fn write_table(&mut self, table: &AuxTable) -> anyhow::Result<()> {
        self.a.write_table(table)?;
        self.b.write_table(table)
    }
    fn finish(&mut self) -> anyhow::Result<()> {
        self.a.finish()?;
        self.b.finish()
    }
}

/// A sink that only counts (dry runs, benchmarks, tests).
#[derive(Default)]
pub struct CountingSink {
    pub edges: u64,
    pub labeled: u64,
}

impl EventSink for CountingSink {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()> {
        self.edges += batch.len() as u64;
        self.labeled += batch.label.iter().filter(|&&l| l != 0).count() as u64;
        Ok(())
    }
}

/// One columnar batch of nodes (per entity type), emitted after simulation
/// with final state values.
#[derive(Debug, Clone)]
pub struct NodeBatch {
    pub entity_type: String,
    pub ids: Vec<u64>,
    pub attr_names: Vec<String>,
    /// Parallel to `attr_names`.
    pub attrs: Vec<NodeColumn>,
}

#[derive(Debug, Clone)]
pub enum NodeColumn {
    Numeric(Vec<f64>),
    /// Dictionary-encoded categorical/ordinal values.
    Category { codes: Vec<u16>, names: Vec<String> },
}

/// One column of an auxiliary output table.
#[derive(Debug, Clone)]
pub enum TableColumn {
    U64(Vec<u64>),
    I64(Vec<i64>),
    F64(Vec<f64>),
    /// Dictionary-encoded strings.
    Category { codes: Vec<u16>, names: Vec<String> },
}

impl TableColumn {
    pub fn len(&self) -> usize {
        match self {
            TableColumn::U64(v) => v.len(),
            TableColumn::I64(v) => v.len(),
            TableColumn::F64(v) => v.len(),
            TableColumn::Category { codes, .. } => codes.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A chunk of an auxiliary output table beside the edges and node tables:
/// `node_labels` (node-level ground truth) and `node_states` (the state
/// trajectory). A sink may receive several chunks of one table, always with
/// the same schema, and appends them in order.
#[derive(Debug, Clone)]
pub struct AuxTable {
    pub name: String,
    pub columns: Vec<(String, TableColumn)>,
}

impl AuxTable {
    pub fn len(&self) -> usize {
        self.columns.first().map_or(0, |(_, c)| c.len())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Periodic progress callback: (edges_so_far, fraction_complete).
pub type ProgressFn<'a> = &'a mut dyn FnMut(u64, f64);

/// The omniscient ground-truth record of ONE anomaly instance (a campaign or a
/// failure incident). Because the simulator IS the mechanism, this is free and
/// exact — impossible for a real-data curator (§3, §11b). Multi-attribute
/// ground truth: not just "which edges are anomalous" but which instance,
/// which typology, at what difficulty, in which community, over which window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyRecord {
    /// Stable instance id; matches `anomaly_id` on the edges it caused.
    pub id: i64,
    /// Intent / typology (e.g. "structuring", "congestion_cascade").
    pub intent: String,
    /// "adversary" (intentional) or "failure" (emergent/natural).
    pub kind: String,
    /// Effective camouflage/difficulty in [0,1] that shaped this instance
    /// (the difficulty axis applied to the process). NaN if not applicable.
    pub camouflage: f64,
    /// Node ids participating (ring members / affected entity), complete.
    /// The node_labels table carries the same membership with roles.
    pub members: Vec<u64>,
    pub n_members: u64,
    /// Placement community block index, if clustered (else -1).
    pub community: i64,
    /// Active time window (unix seconds).
    pub start_t: i64,
    pub end_t: i64,
    /// True if spawned by a cascade from another instance.
    pub cascade: bool,
    /// The member that runs the scheme, or -1: the centre of a hub (star
    /// fan-out) stage, or the member a fan-in stage collects to. Gives the
    /// `role` column of labels_nodes.csv (operator vs member).
    #[serde(default = "minus_one")]
    pub operator: i64,
}

/// What actually happened, for `saga_meta.json` and the end-of-run summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenSummary {
    pub nodes: u64,
    pub edges_written: u64,
    pub anomalous_edges: u64,
    /// Intent id -> name (index 0 = "normal").
    pub intent_names: Vec<String>,
    pub event_type_names: Vec<String>,
    /// Per-intent edge counts (parallel to intent_names).
    pub edges_per_intent: Vec<u64>,
    /// Categorical attribute dictionaries: (attr, values) — code i = values[i].
    pub attr_dictionaries: Vec<(String, Vec<String>)>,
    /// The omniscient per-instance ground-truth record (written to
    /// `ground_truth.json`). Edges join to it via their `anomaly_id` column.
    pub ground_truth: Vec<AnomalyRecord>,
    pub wall_time_s: f64,
    pub events_per_sec: f64,
    /// Overdraft-guard telemetry (Phase 1 mechanism). `overdraft_guarded` is the
    /// number of emitted transfers subject to a guard; `overdraft_violations` is
    /// how many of those exceeded the payer's TRUE up-to-date balance yet were
    /// emitted because the guard read a stale (block-start) snapshot. The
    /// violation rate rises with the block length L and falls toward 0 as
    /// L -> 0. Both 0 when guards are disabled.
    #[serde(default)]
    pub overdraft_guarded: u64,
    /// Behavior-switch telemetry: candidates whose switch condition held,
    /// how many were thinned away by it, and how many were emitted with
    /// switched attributes.
    #[serde(default)]
    pub switch_active: u64,
    #[serde(default)]
    pub switch_thinned: u64,
    #[serde(default)]
    pub switch_applied: u64,
    /// Candidates whose switch decision differed from the event-serial
    /// decision because the snapshot was stale (the transducer's analogue of
    /// guard leakage; bounded by L).
    #[serde(default)]
    pub switch_misprofiled: u64,
    #[serde(default)]
    pub overdraft_violations: u64,
    /// Multi-process exchange telemetry (Phase 4); all 0 when no exchange ran.
    /// One exchange round per L-block: `exchange_rounds` is the number of
    /// per-block barriers this shard participated in; sent/received count the
    /// cross-shard dst-effect delta records this shard shipped to / folded
    /// from its peers, and their wire bytes.
    #[serde(default)]
    pub exchange_rounds: u64,
    #[serde(default)]
    pub exchange_deltas_sent: u64,
    #[serde(default)]
    pub exchange_bytes_sent: u64,
    #[serde(default)]
    pub exchange_deltas_received: u64,
    #[serde(default)]
    pub exchange_bytes_received: u64,
    /// Discovery diagnostics (consistency model section 16): poll iterations
    /// that found no new inbound file, revalidation nudges performed on a
    /// round directory (only after a round has waited a few ms), and opens
    /// that failed right after a directory listing showed the name (NFS
    /// cache skew, retried). All 0 when every peer was on time on a local
    /// filesystem.
    #[serde(default)]
    pub exchange_idle_polls: u64,
    #[serde(default)]
    pub exchange_nudges: u64,
    #[serde(default)]
    pub exchange_skewed_opens: u64,
    /// Planned anomalous-edge fraction: the planner's expected campaign
    /// events over the expected delivered total (what the run realizes with
    /// guards off, up to Poisson noise). NaN when anomalies are disabled.
    #[serde(default = "f64_nan")]
    pub planned_anomalous_fraction: f64,
    /// Guard-aware calibration telemetry (section 17). `calibration_passes`
    /// is the number of measurement passes that ran (0 = none, the
    /// default). `calibration_target_fraction` is the fraction the
    /// calibration aimed at: the candidate stream's labeled fraction, i.e.
    /// the guards-off realized rate. `calibration_uncorrected_fraction` is
    /// the realized fraction measured in the first pass (the uncorrected
    /// guards-on rate). `calibration_multipliers` are the per-event-type
    /// campaign rate multipliers the final run used (event types whose
    /// multiplier is exactly 1 are omitted).
    #[serde(default)]
    pub calibration_passes: u32,
    #[serde(default = "f64_nan")]
    pub calibration_target_fraction: f64,
    #[serde(default = "f64_nan")]
    pub calibration_uncorrected_fraction: f64,
    #[serde(default)]
    pub calibration_multipliers: Vec<(String, f64)>,
}

fn minus_one() -> i64 {
    -1
}

fn f64_nan() -> f64 {
    f64::NAN
}

/// Run the simulation, streaming event batches (and finally node tables) to
/// the sink.
pub fn generate(
    params: &GenParams,
    sink: &mut dyn EventSink,
    progress: Option<ProgressFn<'_>>,
) -> anyhow::Result<GenSummary> {
    crate::sim::run(params, sink, progress)
}
