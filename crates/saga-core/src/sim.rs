//! The simulation loop (M1: normal behavior; M5 adds anomaly processes).
//!
//! Two passes (consistency model §6, §8):
//!   Phase A (parallel) — draw CANDIDATE events per absolute day. Each actor
//!     draws from its own `(seed, actor, day, behavior)` stream, keyed on the
//!     day and NOT on the staleness block length L, so the candidate stream is
//!     identical for every L and bit-identical for any thread count.
//!   Phase B (single-threaded) — sweep the time-sorted candidates in L-blocks.
//!     Behavior that reads state (the overdraft guard) reads the snapshot as of
//!     the block start (stale by at most L); the block's guarded effects then
//!     merge in canonical order. L touches the output ONLY here, via guards, so
//!     the fidelity-vs-L knob is not confounded with RNG re-keying.
//!
//! Determinism (Theorem 1): candidate draws are schedule-independent and the
//! per-L-block guard+merge is over a canonical order, so at a fixed L both the
//! edges AND the final node state are bit-identical for any thread count.

use std::collections::HashSet;

use rand::Rng;
use rand_distr::Distribution as _;
use rayon::prelude::*;
use saga_sample::{stream, Rng64, StreamPurpose};

use crate::anomaly::{
    AnomalyPlan, Campaign, FailureSpan, PlannedFailureMode, PlannedScope,
};
use crate::api::{EventBatch, EventSink, GenParams, GenSummary, ProgressFn};
use crate::exchange::{Exchanger, WireDelta};
use crate::world::{
    AttrSampler, CompiledBehavior, CompiledCounterparty, EffectKind, World,
};

/// Whether this shard owns `node` (and thus emits its source-edges / node row).
/// `shard_count <= 1` is the normal whole-graph run.
#[inline]
fn in_shard(node: u64, params: &GenParams) -> bool {
    params.shard_count <= 1 || node % params.shard_count == params.shard_index
}

/// Keep only the rows of a node batch this shard owns (partition by id).
fn filter_node_batch(nb: &mut crate::api::NodeBatch, params: &GenParams) {
    use crate::api::NodeColumn;
    let keep: Vec<usize> = (0..nb.ids.len())
        .filter(|&i| in_shard(nb.ids[i], params))
        .collect();
    nb.ids = keep.iter().map(|&i| nb.ids[i]).collect();
    for col in nb.attrs.iter_mut() {
        match col {
            NodeColumn::Numeric(v) => *v = keep.iter().map(|&i| v[i]).collect(),
            NodeColumn::Category { codes, .. } => {
                *codes = keep.iter().map(|&i| codes[i]).collect()
            }
        }
    }
}

/// Mean intra-burst gap in seconds (follow-up events cluster within minutes).
const BURST_GAP_MEAN_S: f64 = 300.0;
/// Actors per parallel task.
const CHUNK: usize = 4096;
/// Repeat-partner memory slots per (actor, emission).
const MEM_K: usize = 2;
const MEM_EMPTY: u64 = u64::MAX;

/// Expected number of burst FOLLOW-UPS that land inside the window, exactly.
///
/// Model (mirrors the emitter): the burst starts at `t_s ~ U(0, W)`; follow-up
/// `j` is at `t_s + G_j` with `G_j ~ Gamma(j, gap)` (a sum of `j` Exp(gap)
/// gaps); the chain has geometric length with mean `m`, so it reaches step `j`
/// with probability `q^j`, `q = m/(1+m)`; and it BREAKS at the first follow-up
/// past `W`. Because `G_j` is increasing in `j`, "step `j` lands inside" already
/// implies every earlier step did, so the events are nested and
///
/// ```text
/// E[surviving] = Σ_{j≥1} q^j · P(t_s + G_j < W).
/// ```
///
/// The old code bounded the inner probability with Jensen —
/// `P(t_s + G_j < W) ≥ 1 − E[G_j]/W = 1 − j·gap/W` — and summed that to
/// `m − (gap/W)·m·(1+m)`, then used the result AS IF it were the expectation.
/// It is a strict LOWER bound (and, clamped at 0, a badly loose one once
/// `j·gap` approaches `W`), so it understates delivered normal volume and hence
/// understates the anomaly cap.
///
/// The exact inner probability is available in closed form. Conditioning on
/// `G_j` and using `t_s ~ U(0, W)`:
///
/// ```text
/// P(t_s + G_j < W) = E[(1 − G_j/W)^+]
///                  = F_j(W) − (1/W)·E[G_j · 1{G_j < W}]
///                  = F_j(W) − (j/λ)·F_{j+1}(W),      λ = W/gap
/// ```
///
/// using the standard Gamma identity `E[G·1{G<W}] = j·gap·F_{j+1}(W)`. For
/// integer `j`, `F_j(W) = P(N ≥ j)` for `N ~ Poisson(λ)` (the Erlang/Poisson
/// duality), so the whole sum is computable from Poisson tail probabilities with
/// no special functions. Terms carry the factor `q^j`, so the series converges
/// geometrically; we truncate once the remaining mass is negligible.
fn burst_survival(m: f64, gap_s: f64, window_s: f64) -> f64 {
    if m <= 0.0 || window_s <= 0.0 || gap_s <= 0.0 {
        return 0.0;
    }
    let q = m / (1.0 + m);
    let lambda = window_s / gap_s;
    // Poisson(λ) pmf/tail, built up incrementally: pmf_i = e^{-λ} λ^i / i!.
    // tail_j = P(N ≥ j) = 1 − Σ_{i<j} pmf_i.
    let mut pmf = (-lambda).exp();
    let mut cdf = pmf; // P(N ≤ 0)
    // tails[j] = P(N ≥ j); tails[1] = 1 − P(N = 0).
    let mut tail_j = 1.0 - cdf; // P(N ≥ 1)
    let mut qj = q; // q^j
    let mut sum = 0.0;
    // q^j decays geometrically; 4096 steps is far past any practical burst.
    for j in 1..4096u32 {
        // advance to P(N ≥ j+1)
        pmf *= lambda / j as f64;
        cdf += pmf;
        let tail_j1 = (1.0 - cdf).max(0.0);
        let p_inside = (tail_j - (j as f64 / lambda) * tail_j1).clamp(0.0, 1.0);
        sum += qj * p_inside;
        qj *= q;
        tail_j = tail_j1;
        // Remaining terms are bounded by Σ_{i>j} q^i = q^{j+1}/(1−q).
        if qj / (1.0 - q) < 1e-12 || (tail_j <= 0.0 && p_inside <= 0.0) {
            break;
        }
    }
    sum
}


/// Expected edges the NORMAL stream actually DELIVERS at the calibrated
/// `world.rate_scale`, before the anomaly rebalance.
///
/// `World::build`'s calibration is the *nominal* budget: it solves
/// `rate_scale` so that `Σ_b activity_sum·rate·span·burst_factor` equals the
/// edge target. Two things make the simulator deliver slightly less (or more):
///
///   * burst chains are truncated at the window boundary (`bt >= window_len_s
///     => break`), but `burst_factor = 1 + burst_p·burst_mean_len` assumes
///     every follow-up fits. Follow-up `j` survives iff `t_s + Gamma(j, gap) <
///     W`; since the chain breaks at the first overflow, the survival events
///     are nested and `E[surviving] = Σ_j q^j·P(t_s + G_j < W)` with
///     `q = m/(1+m)`. `burst_survival` evaluates that sum EXACTLY (see there).
///   * the span covers a whole number of days whose weekday mix is generally
///     unbalanced (30 days = 4 weeks + 2), so the `weekly` profile does not
///     average to 1 over the run even though it is normalized to mean 1.
///
/// The anomaly cap bounds the anomalous fraction of the DELIVERED graph, so it
/// must be computed against this rather than against the nominal target.
pub(crate) fn expected_normal_delivered(params: &GenParams, world: &World) -> f64 {
    let n_windows = params.span_days.ceil() as u64;
    world
        .behaviors
        .iter()
        .map(|b| {
            let burst_factor = if b.branching_ratio > 0.0 {
                // Hawkes children are dropped past the window edge too, but no
                // built-in rule base uses self-excitation; keep the closed form.
                1.0 / (1.0 - b.branching_ratio)
            } else {
                // Exact expectation (the window is one day of simulated time).
                let survived = burst_survival(b.burst_mean_len, BURST_GAP_MEAN_S, 86_400.0);
                1.0 + b.burst_p * survived
            };
            // Exact weekday mix over the actual windows.
            let weekday_days: f64 = (0..n_windows)
                .map(|w| {
                    let len = (params.span_days - w as f64).min(1.0);
                    let wd = (((params.epoch_unix / 86_400) + w as i64 + 3).rem_euclid(7)) as usize;
                    b.weekly[wd] * len
                })
                .sum();
            b.activity_sum * b.rate_per_day * world.rate_scale * weekday_days * burst_factor
        })
        .sum()
}

/// Build the world and anomaly plan and derive the homophily indices, exactly
/// as [`run`] does before Phase A. Factored out so the single-machine engine
/// and the Phase-2 sharded-equivalence emulation share one setup path (they
/// must observe the identical `World`, `rate_scale` rebalance and marked sets,
/// or the equivalence proof would be comparing two different worlds).
type Prepared = (World, AnomalyPlan, Vec<Vec<u64>>, HashSet<u64>);
fn prepare(params: &GenParams) -> anyhow::Result<Prepared> {
    let mut world = World::build(params)?;

    // Plan anomaly campaigns/incidents up front (M5), then rebalance the
    // normal-behavior budget so total output still hits the edge target:
    // calibrated emergence, not injection on top (§3).
    let mut plan = crate::anomaly::plan(params, &world)?;
    let target = params.target_edges as f64;
    let normal_share = ((target - plan.expected_events) / target).clamp(0.3, 1.0);
    // The planned anomalous-edge fraction of the DELIVERED graph (section 17
    // telemetry): expected campaign events over expected campaign + rebalanced
    // normal events. Computed against the same delivered-normal estimate the
    // cap uses, BEFORE the rebalance multiplies into `rate_scale`.
    if !params.anomalies_disabled {
        let normal_full = expected_normal_delivered(params, &world);
        let total = plan.expected_events + normal_share * normal_full;
        plan.planned_fraction = if total > 0.0 { plan.expected_events / total } else { 0.0 };
    }
    world.rate_scale *= normal_share;

    // Anomaly homophily (fidelity axis): index every marked actor by entity
    // type so a campaign can redirect a counterparty to another marked actor of
    // the SAME type. Built ONCE from the deterministic plan (identical on every
    // shard/thread), so the redirect draws stay bit-identical. `marked_set` is
    // the guard that skips counterparties already marked (structural motifs).
    let marked_by_type: Vec<Vec<u64>> = {
        let mut buckets = vec![Vec::new(); world.entities.starts.len()];
        for c in &plan.campaigns {
            for &id in &c.members {
                buckets[world.entities.type_of(id)].push(id);
            }
        }
        buckets
    };
    let marked_set: HashSet<u64> =
        plan.campaigns.iter().flat_map(|c| c.members.iter().copied()).collect();
    Ok((world, plan, marked_by_type, marked_set))
}

/// Repeat-partner memory buffers, one per behavior: interleaved
/// [actor][emission][K]. Must persist across days so partner memory carries
/// from one window to the next (same lifetime as in the pre-refactor engine).
fn init_memories(world: &World) -> Vec<Vec<u64>> {
    world
        .behaviors
        .iter()
        .map(|b| vec![MEM_EMPTY; b.actors.len() * b.emissions.len() * MEM_K])
        .collect()
}

/// Phase A for ONE absolute day `w` (consistency model §6, §8): draw the day's
/// CANDIDATE events, L-independently, keyed on the absolute day. State is NOT
/// read here. The day is merge-sorted and granularity-rounded before it is
/// returned (the same order of operations as the pre-Phase-1 streaming
/// engine). Under a real shard (`shard_count > 1`) the `in_shard` gates inside
/// `gen_chunk`/`gen_campaign_window` keep only this shard's edges; the
/// Phase-2 emulation instead passes whole-graph params and partitions by
/// `src % N` itself (identical because sharding partitions strictly by src).
#[allow(clippy::too_many_arguments)]
fn phase_a_day(
    params: &GenParams,
    world: &World,
    plan: &AnomalyPlan,
    pool: &rayon::ThreadPool,
    memories: &mut [Vec<u64>],
    w: u64,
    marked_by_type: &[Vec<u64>],
    marked_set: &HashSet<u64>,
) -> EventBatch {
    let window_len_days = (params.span_days - w as f64).min(1.0);
    let weekday = (((params.epoch_unix / 86_400) + w as i64 + 3).rem_euclid(7)) as usize;

    // Generate per behavior (sequential), per actor-chunk (parallel).
    let mut window_parts: Vec<EventBatch> = Vec::new();
    for (bi, b) in world.behaviors.iter().enumerate() {
        if b.actors.is_empty() {
            continue;
        }
        let mem_stride = b.emissions.len() * MEM_K;
        let parts: Vec<EventBatch> = pool.install(|| {
            b.actors
                .par_chunks(CHUNK)
                .zip(memories[bi].par_chunks_mut(CHUNK * mem_stride))
                .enumerate()
                .map(|(chunk_i, (actors, mem))| {
                    gen_chunk(
                        params, world, plan, b, bi as u64, w, window_len_days, weekday,
                        actors, chunk_i * CHUNK, mem, marked_by_type, marked_set,
                    )
                })
                .collect()
        });
        window_parts.extend(parts);
    }

    // Campaign events for campaigns overlapping this window (parallel,
    // fixed order — deterministic regardless of thread count).
    let window_start_s = params.epoch_unix + (w as i64) * 86_400;
    let window_end_s = window_start_s + (window_len_days * 86_400.0) as i64;
    let campaign_parts: Vec<EventBatch> = pool.install(|| {
        plan.campaigns
            .par_iter()
            .enumerate()
            .filter(|(_, c)| {
                c.stages
                    .iter()
                    .any(|s| s.start_s < window_end_s && s.end_s > window_start_s)
            })
            .map(|(ci, c)| {
                gen_campaign_window(
                    params, world, c, ci as u64, w, window_start_s, window_end_s,
                    plan.rate_scale, &plan.guard_calibration, marked_by_type, marked_set,
                )
            })
            .collect()
    });
    window_parts.extend(campaign_parts);

    // Round to the output granularity BEFORE the merge-sort, so the day's
    // stream is ordered by its FINAL (rounded) timestamps and events sharing
    // a full sort key are adjacent. Phase B's canonical state fold and the
    // Phase-4 exchange merge key on the rounded timestamp (it is the only one
    // that exists downstream), so the stream order must agree with that key
    // at any granularity. For the default granularity of 1 s the rounding is
    // the identity and the batch is byte-identical to the historical
    // round-after-sort order.
    if params.granularity_s > 1 {
        let g = params.granularity_s as i64;
        for part in window_parts.iter_mut() {
            for t in part.t.iter_mut() {
                *t -= t.rem_euclid(g);
            }
        }
    }
    merge_sorted(window_parts, world.attr_names.clone())
}

/// Phase A for the WHOLE run: every day's candidates concatenated into one
/// globally time-sorted batch.
///
/// MEMORY: this MATERIALIZES the full run's candidates in RAM, so it is only
/// used by the Phase-2 sharded-equivalence emulation ([`shard_equivalence`],
/// test scale, which must route one shared candidate array to N virtual
/// shards). The production path [`run`] streams day by day with bounded memory
/// instead (§9 notes); do not call this from a scale path.
fn phase_a(
    params: &GenParams,
    world: &World,
    plan: &AnomalyPlan,
    pool: &rayon::ThreadPool,
    marked_by_type: &[Vec<u64>],
    marked_set: &HashSet<u64>,
) -> EventBatch {
    let mut memories = init_memories(world);
    let n_windows = params.span_days.ceil() as u64;
    let mut cand = EventBatch {
        attr_names: world.attr_names.clone(),
        attrs: vec![Vec::new(); world.attr_names.len()],
        ..Default::default()
    };
    for w in 0..n_windows {
        let day = phase_a_day(
            params, world, plan, pool, &mut memories, w, marked_by_type, marked_set,
        );
        append_batch(&mut cand, &day);
    }
    cand
}

/// Streaming Phase-B sweep state: the running block-merged state, the
/// never-stale measurement ledger, and the output counters. One instance lives
/// for the whole run; [`BlockSweep::sweep`] consumes complete L-blocks off the
/// front of the pending candidate buffer as days stream in.
struct BlockSweep {
    /// Block length L in seconds and the epoch, defining `block_of`.
    l_s: f64,
    epoch: i64,
    guards_enabled: bool,
    /// Sharding identity (whole-graph run: 0 of 1).
    shard_index: u64,
    shard_count: u64,
    /// Next canonical block index to process, exchange mode only: the barrier
    /// schedule must cover EVERY block (a shard with no local events in a
    /// block still receives its peers' deltas there), so blocks advance one by
    /// one from the first possible block instead of skipping empty ones.
    next_block: i64,
    /// Frozen block-start snapshot during a block; merged at block boundaries.
    state: Vec<Vec<Vec<f64>>>,
    /// True (never-stale) ledger, only for the overdraft-violation
    /// measurement: `state` as it would be under L -> 0, folded per event.
    exact: Vec<Vec<Vec<f64>>>,
    overdraft_guarded: u64,
    overdraft_violations: u64,
    edges_total: u64,
    edges_per_intent: Vec<u64>,
    /// Section-17 survival tallies, guards-on path only: per event type,
    /// candidates seen and candidates kept, split by stream class
    /// (`CLASS_NORMAL`, `CLASS_CAMPAIGN`, `CLASS_FAILURE`).
    tally: SurvivalTally,
    /// Labels `1..=n_adversary` are campaign intents; higher labels are
    /// failure intents (the intent table order).
    n_adversary: u16,
}

const CLASS_NORMAL: usize = 0;
const CLASS_CAMPAIGN: usize = 1;
const CLASS_FAILURE: usize = 2;

/// Per-event-type candidate/kept counts by stream class (section 17).
#[derive(Clone, Debug, Default)]
pub(crate) struct SurvivalTally {
    /// `[event_type][class]`.
    pub candidates: Vec<[u64; 3]>,
    pub kept: Vec<[u64; 3]>,
}

impl SurvivalTally {
    fn new(n_events: usize) -> Self {
        SurvivalTally { candidates: vec![[0; 3]; n_events], kept: vec![[0; 3]; n_events] }
    }
    fn class_total(counts: &[[u64; 3]], class: usize) -> u64 {
        counts.iter().map(|c| c[class]).sum()
    }
    /// Realized labeled fraction of the KEPT stream.
    pub fn kept_labeled_fraction(&self) -> f64 {
        let total: u64 = self.kept.iter().map(|c| c[0] + c[1] + c[2]).sum();
        let labeled = Self::class_total(&self.kept, CLASS_CAMPAIGN)
            + Self::class_total(&self.kept, CLASS_FAILURE);
        if total == 0 { 0.0 } else { labeled as f64 / total as f64 }
    }
    /// Labeled fraction of the CANDIDATE stream: exactly the guards-off
    /// realized rate (Phase A is guard- and L-independent).
    pub fn candidate_labeled_fraction(&self) -> f64 {
        let total: u64 = self.candidates.iter().map(|c| c[0] + c[1] + c[2]).sum();
        let labeled = Self::class_total(&self.candidates, CLASS_CAMPAIGN)
            + Self::class_total(&self.candidates, CLASS_FAILURE);
        if total == 0 { 0.0 } else { labeled as f64 / total as f64 }
    }
}

/// Widest multiplier one calibration pass may assign to an event type. Only
/// reached when a type's campaign candidates were (almost) all suppressed,
/// where the ratio estimate is unstable; reported, never silent.
const MAX_CALIBRATION_MULTIPLIER: f64 = 8.0;

/// Solve the section-17 fixed point from one pass's tallies: for every event
/// type `e` with campaign candidates, the multiplier `g_e = s_bg / s_camp(e)`
/// where `s_bg` is the survival of the background stream (normal plus
/// failure-labeled normal events, all event types) and `s_camp(e)` the
/// survival of the campaign candidates of type `e` under the multipliers the
/// pass ran with. Rationale: candidate counts scale with the multiplier, so
/// kept campaign edges of type `e` become `g_e * s_camp(e) * C_e = s_bg * C_e`
/// and the kept labeled fraction returns to the candidate (guards-off)
/// labeled fraction. Pure integer-derived arithmetic: deterministic.
pub(crate) fn solve_calibration(t: &SurvivalTally) -> Vec<f64> {
    let bg_c = SurvivalTally::class_total(&t.candidates, CLASS_NORMAL)
        + SurvivalTally::class_total(&t.candidates, CLASS_FAILURE);
    let bg_k = SurvivalTally::class_total(&t.kept, CLASS_NORMAL)
        + SurvivalTally::class_total(&t.kept, CLASS_FAILURE);
    let s_bg = if bg_c == 0 { 1.0 } else { bg_k as f64 / bg_c as f64 };
    t.candidates
        .iter()
        .zip(&t.kept)
        .map(|(c, k)| {
            let (cc, ck) = (c[CLASS_CAMPAIGN], k[CLASS_CAMPAIGN]);
            if cc == 0 {
                return 1.0; // nothing of this type to calibrate
            }
            if ck == 0 {
                return MAX_CALIBRATION_MULTIPLIER;
            }
            let s_camp = ck as f64 / cc as f64;
            (s_bg / s_camp).clamp(1.0 / MAX_CALIBRATION_MULTIPLIER, MAX_CALIBRATION_MULTIPLIER)
        })
        .collect()
}

/// Round `t` down to the output granularity, exactly as `phase_a_day` rounds
/// event timestamps. Used to derive block-schedule bounds from params only,
/// so every shard computes the identical barrier schedule.
#[inline]
fn round_to_granularity(t: i64, granularity_s: u64) -> i64 {
    if granularity_s > 1 {
        let g = granularity_s as i64;
        t - t.rem_euclid(g)
    } else {
        t
    }
}

impl BlockSweep {
    fn new(params: &GenParams, world: &World, n_intents: usize) -> Self {
        let l_s = (params.block_len_days * 86_400.0).max(1.0);
        let epoch = params.epoch_unix;
        // First possible (granularity-rounded) event time is day 0's start.
        let first_t = round_to_granularity(epoch, params.granularity_s);
        BlockSweep {
            l_s,
            epoch,
            guards_enabled: params.guards_enabled,
            shard_index: params.shard_index,
            shard_count: params.shard_count.max(1),
            next_block: (((first_t - epoch) as f64) / l_s).floor() as i64,
            state: world.state.clone(),
            exact: if params.guards_enabled { world.state.clone() } else { Vec::new() },
            overdraft_guarded: 0,
            overdraft_violations: 0,
            edges_total: 0,
            edges_per_intent: vec![0u64; n_intents],
            tally: SurvivalTally::new(world.events.len()),
            n_adversary: params.rulebase.adversaries.len() as u16,
        }
    }

    #[inline]
    fn class_of(&self, label: u16) -> usize {
        if label == 0 {
            CLASS_NORMAL
        } else if label <= self.n_adversary {
            CLASS_CAMPAIGN
        } else {
            CLASS_FAILURE
        }
    }

    #[inline]
    fn block_of(&self, t: i64) -> i64 {
        ((t - self.epoch) as f64 / self.l_s).floor() as i64
    }

    /// One past the last block any event of the run can fall into, from params
    /// only (identical on every shard): events are strictly before the last
    /// day's end and granularity rounding only moves them down.
    fn final_block_limit(&self, params: &GenParams) -> i64 {
        let n_windows = params.span_days.ceil() as u64;
        let last_w = n_windows.saturating_sub(1);
        let last_len_days = (params.span_days - last_w as f64).min(1.0);
        let last_day_end =
            self.epoch + (last_w as i64) * 86_400 + (last_len_days * 86_400.0) as i64;
        self.block_of(round_to_granularity(last_day_end - 1, params.granularity_s)) + 1
    }

    /// Guards-off fast path: the sweep reads no state at all, so the whole
    /// `batch` is kept verbatim, with no row copies and no buffering. Effects
    /// still fold into `state` in canonical (time-sorted) order for the node
    /// export; because that order is the same under any block partition, the
    /// final state is identical to a per-L-block merge.
    fn keep_all(&mut self, world: &World, batch: &EventBatch) {
        fold_canonical(world, &mut self.state, batch);
        self.edges_total += batch.len() as u64;
        for &l in &batch.label {
            self.edges_per_intent[l as usize] += 1;
        }
    }

    /// Guard-filter the pending rows `[i, j)` (one block) against the frozen
    /// block-start snapshot in `state`, returning the kept rows and updating
    /// the exact-ledger telemetry. `state` is NOT mutated here.
    fn filter_block(
        &mut self,
        world: &World,
        pending: &EventBatch,
        i: usize,
        j: usize,
    ) -> EventBatch {
        let mut kept = EventBatch {
            attr_names: pending.attr_names.clone(),
            attrs: vec![Vec::new(); pending.attrs.len()],
            ..Default::default()
        };
        for k in i..j {
            let ev = &world.events[pending.event_type[k] as usize];
            if self.guards_enabled {
                let class = self.class_of(pending.label[k]);
                self.tally.candidates[pending.event_type[k] as usize][class] += 1;
                if let Some(g) = &ev.guard {
                    let start = world.entities.starts[g.entity];
                    let bal = self.state[g.entity][g.var][(pending.src[k] - start) as usize];
                    let amt = pending.attrs[g.from_col][k];
                    // Suppress an outgoing event the SRC cannot cover on
                    // the (possibly stale) snapshot: it never happens.
                    if amt.is_finite() && bal < amt {
                        continue;
                    }
                    // Emitted: measure against the TRUE up-to-date balance.
                    // A violation is a transfer that slipped through the
                    // stale guard yet overdrew the real balance.
                    let true_bal =
                        self.exact[g.entity][g.var][(pending.src[k] - start) as usize];
                    self.overdraft_guarded += 1;
                    if amt.is_finite() && amt > true_bal {
                        self.overdraft_violations += 1;
                    }
                }
                // Keep the exact ledger current for the next measurement.
                apply_event(world, &mut self.exact, pending, k);
                self.tally.kept[pending.event_type[k] as usize][class] += 1;
            }
            append_row(&mut kept, pending, k);
        }
        kept
    }

    /// Count and write one block's kept rows. The rows are given to the sink
    /// (moved, not copied: the sweep is done with them).
    fn emit_block(&mut self, kept: EventBatch, sink: &mut dyn EventSink) -> anyhow::Result<()> {
        self.edges_total += kept.len() as u64;
        for &l in &kept.label {
            self.edges_per_intent[l as usize] += 1;
        }
        sink.write_batch_owned(kept)
    }

    /// Sweep every COMPLETE L-block (block index < `limit`) off the front of
    /// `pending`, in candidate order: for each block, `state` is the frozen
    /// block-start snapshot every guard reads; kept events are written to the
    /// sink; the block's guarded effects merge at the boundary in the
    /// canonical fold order; the consumed prefix is freed. Stops at the first
    /// event whose block is >= `limit` (that block may still receive events
    /// from future days), which is always a block-run boundary, so the run
    /// decomposition and processing order are identical to a single whole-run
    /// sweep. `limit = i64::MAX` flushes everything.
    ///
    /// With `ex` (Phase 4, the real sharded run): the sweep walks the
    /// CANONICAL block schedule `next_block..limit` one block at a time, empty
    /// blocks included, because every block is a barrier round shared with the
    /// peer shards. Per block it publishes the cross-shard dst-effect deltas
    /// of its kept events, blocks until every peer's deltas for the same block
    /// arrive, and merges local owned effects + received deltas in the
    /// canonical delta order to form this shard's `S_b` slice (§4). Only
    /// owned state cells are ever written, so the exported node rows (which
    /// are filtered to owned ids) are complete and correct.
    fn sweep(
        &mut self,
        world: &World,
        pending: &mut EventBatch,
        limit: i64,
        sink: &mut dyn EventSink,
        mut ex: Option<&mut Exchanger>,
    ) -> anyhow::Result<()> {
        let n = pending.len();
        let mut i = 0usize;
        if let Some(ex) = ex.as_deref_mut() {
            while self.next_block < limit {
                let blk = self.next_block;
                debug_assert!(i >= n || self.block_of(pending.t[i]) >= blk);
                let mut j = i;
                while j < n && self.block_of(pending.t[j]) == blk {
                    j += 1;
                }
                let kept = self.filter_block(world, pending, i, j);
                // Route this block's effect deltas by the owner of the node
                // they touch: src-effects are always local (events are
                // src-sharded); dst-effects may belong to a peer.
                let mut outbound: Vec<Vec<WireDelta>> =
                    (0..self.shard_count).map(|_| Vec::new()).collect();
                for r in 0..kept.len() {
                    let ev = &world.events[kept.event_type[r] as usize];
                    for (ei, eff) in ev.effects.iter().enumerate() {
                        let amount = effect_amount(eff, &kept, r);
                        if effect_is_noop(eff, amount) {
                            continue;
                        }
                        let node = if eff.on_src { kept.src[r] } else { kept.dst[r] };
                        outbound[(node % self.shard_count) as usize].push(WireDelta {
                            t: kept.t[r],
                            src: kept.src[r],
                            dst: kept.dst[r],
                            event_type: kept.event_type[r],
                            effect_index: ei as u8,
                            amount,
                        });
                    }
                }
                // Barrier: publish outbound, collect every peer's inbound.
                let inbound = ex.round(blk, &outbound)?;
                // Merge local owned deltas + received deltas in the canonical
                // delta order. Sorting by the full content key reproduces the
                // whole-graph fold restricted to this shard's nodes: leading
                // (t, src, dst, event_type) is the batch order, the trailing
                // content fields canonicalize full-key ties (identical keys
                // are identical deltas), and no key ever collides ACROSS
                // shards because equal keys share their src and hence their
                // generating shard.
                let mut merged = std::mem::take(&mut outbound[self.shard_index as usize]);
                for (origin, deltas) in inbound.iter().enumerate() {
                    for d in deltas {
                        let ev = world.events.get(d.event_type as usize);
                        let owned = ev
                            .and_then(|e| e.effects.get(d.effect_index as usize))
                            .map(|eff| if eff.on_src { d.src } else { d.dst })
                            .is_some_and(|node| node % self.shard_count == self.shard_index);
                        anyhow::ensure!(
                            owned,
                            "exchange delta from shard {origin} targets a node this shard \
                             ({}/{}) does not own; the exchange dir is stale or the shard \
                             processes disagree on seed/params",
                            self.shard_index,
                            self.shard_count
                        );
                    }
                    merged.extend_from_slice(deltas);
                }
                merged.sort_unstable_by_key(delta_key);
                for d in &merged {
                    apply_wire_delta(world, &mut self.state, d)?;
                }
                if self.guards_enabled {
                    // Fold the RECEIVED deltas into the exact ledger too (the
                    // local kept events were folded per event in filter_block),
                    // so the telemetry ledger tracks S_b plus local intra-block
                    // updates. Cross-shard in-edges reach it once per block,
                    // so per-shard violation telemetry is an approximation
                    // under sharding (documented); correctness of edges and
                    // state does not depend on it.
                    for deltas in &inbound {
                        for d in deltas {
                            apply_wire_delta(world, &mut self.exact, d)?;
                        }
                    }
                }
                self.emit_block(kept, sink)?;
                i = j;
                self.next_block += 1;
            }
            drain_prefix(pending, i);
            return Ok(());
        }
        while i < n {
            let blk = self.block_of(pending.t[i]);
            if blk >= limit {
                break;
            }
            let mut j = i;
            while j < n && self.block_of(pending.t[j]) == blk {
                j += 1;
            }
            // `state` is the block-start snapshot for the whole of [i, j): it
            // is not mutated until the block's guarded effects merge below.
            let kept = self.filter_block(world, pending, i, j);
            // Merge the block's guarded effects into the running state
            // (canonical fold order over the already-sorted slice).
            fold_canonical(world, &mut self.state, &kept);
            self.emit_block(kept, sink)?;
            i = j;
        }
        drain_prefix(pending, i);
        Ok(())
    }
}

pub fn run(
    params: &GenParams,
    sink: &mut dyn EventSink,
    mut progress: Option<ProgressFn<'_>>,
) -> anyhow::Result<GenSummary> {
    params.rulebase.validate()?;
    // The staleness bound L must be a real, positive length. Anything else
    // (0, negative, NaN, infinite) would previously be clamped silently to a
    // 1-second block inside BlockSweep::new and produce plausible-looking but
    // unintended output. Refuse it instead. (Positive values below 1 second
    // still floor at 1 second, the engine's time granularity; values >= the
    // span are one whole-run block, option A. Both are documented semantics.)
    anyhow::ensure!(
        params.block_len_days.is_finite() && params.block_len_days > 0.0,
        "block_len_days must be a finite value > 0, got {}; use any value >= \
         span_days for a single whole-run block (option A)",
        params.block_len_days
    );
    // Phase 4: the real multi-process per-block state exchange. With an
    // exchange dir, the N shard processes run the §4 protocol over a shared
    // filesystem directory and each shard's edges AND owned node state equal
    // the single-machine run's.
    if params.exchange_dir.is_some() {
        anyhow::ensure!(
            params.shard_count > 1,
            "exchange_dir (--exchange-dir) requires shard_count > 1: the exchange \
             is the cross-process protocol between shard processes; a whole-graph \
             run has no peers to exchange with"
        );
        anyhow::ensure!(
            params.shard_index < params.shard_count,
            "shard_index {} must be < shard_count {}",
            params.shard_index,
            params.shard_count
        );
    }
    // Correctness rail: WITHOUT the exchange, the sharded path ships no
    // cross-shard state (consistency model §4, §10), so with guards on each
    // shard would read state missing the dst-effects of other shards' edges
    // and silently produce wrong edges AND wrong node state. Refuse the
    // combination instead of emitting wrong data.
    if params.guards_enabled && params.shard_count > 1 && params.exchange_dir.is_none() {
        anyhow::bail!(
            "state guards (--guards) with --shard-count > 1 require the per-block \
             cross-shard state exchange: pass --exchange-dir <shared directory> \
             (the same fresh directory to all {} shard processes, e.g. on a shared \
             filesystem). Without it each shard misses the dst-effects of other \
             shards' edges and guarded generation would silently produce wrong \
             edges and wrong node state. Alternatively run without --guards for \
             edge-only sharded generation.",
            params.shard_count
        );
    }
    // Section 17: guard-aware anomaly calibration is a whole-graph, guards-on
    // procedure (it measures the guard's differential suppression).
    if params.calibrate_passes > 0 {
        anyhow::ensure!(
            params.guards_enabled,
            "calibrate_passes = {} requires guards_enabled: the calibration measures \
             how the state guards suppress anomalous vs normal candidates, which is \
             a no-op with guards off (guards-off output already realizes the planned \
             anomaly rate)",
            params.calibrate_passes
        );
        anyhow::ensure!(
            params.shard_count <= 1,
            "calibrate_passes = {} requires a whole-graph run (shard_count == 1): a \
             shard sees only its own slice of the stream, so per-shard measurements \
             would disagree and the shards would no longer union to one graph",
            params.calibrate_passes
        );
    }
    let t0 = std::time::Instant::now();
    let (mut world, mut plan, marked_by_type, marked_set) = prepare(params)?;

    // Calibration passes: run the full guarded pipeline into a counting sink,
    // measure per-event-type survival, solve the multipliers, repeat. Every
    // pass is a complete deterministic run, so the multipliers are a pure
    // function of (params, seed) and the final run is schedule-independent.
    let mut calibration_target = f64::NAN;
    let mut calibration_uncorrected = f64::NAN;
    for pass in 0..params.calibrate_passes {
        let mut counter = crate::api::CountingSink::default();
        let mut relay = |edges: u64, frac: f64| {
            if let Some(p) = progress.as_mut() {
                p(edges, frac);
            }
        };
        let (_, tally) = run_pass(
            params, &mut world, &plan, &marked_by_type, &marked_set, &mut counter,
            Some(&mut relay), t0,
        )?;
        if pass == 0 {
            calibration_target = tally.candidate_labeled_fraction();
            calibration_uncorrected = tally.kept_labeled_fraction();
        }
        plan.guard_calibration = solve_calibration(&tally);
    }

    let (mut s, _) = run_pass(
        params, &mut world, &plan, &marked_by_type, &marked_set, sink, progress, t0,
    )?;
    s.planned_anomalous_fraction = plan.planned_fraction;
    s.calibration_passes = params.calibrate_passes;
    s.calibration_target_fraction = calibration_target;
    s.calibration_uncorrected_fraction = calibration_uncorrected;
    s.calibration_multipliers = plan
        .guard_calibration
        .iter()
        .enumerate()
        .filter(|(_, &g)| g != 1.0)
        .map(|(e, &g)| (world.events[e].name.clone(), g))
        .collect();
    Ok(s)
}

/// One complete generation pass over a prepared world and plan: Phases A + B
/// streamed day by day, node export, sink finish. `run` calls it once for a
/// plain run and `calibrate_passes + 1` times for a calibrated one; the
/// world's initial state is left exactly as found (the export swaps the final
/// state in and back out), so successive passes observe the identical world.
#[allow(clippy::too_many_arguments)]
fn run_pass(
    params: &GenParams,
    world_mut: &mut World,
    plan: &AnomalyPlan,
    marked_by_type: &[Vec<u64>],
    marked_set: &HashSet<u64>,
    sink: &mut dyn EventSink,
    mut progress: Option<ProgressFn<'_>>,
    t0: std::time::Instant,
) -> anyhow::Result<(GenSummary, SurvivalTally)> {
    let world: &World = &*world_mut;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(params.threads.max(1))
        .build()
        .map_err(|e| anyhow::anyhow!("cannot build thread pool: {e}"))?;

    // Fixed intent table (M1 emits only "normal"; the table already carries
    // every declared process so labels are stable across milestones).
    let mut intent_names = vec!["normal".to_string()];
    intent_names.extend(params.rulebase.adversaries.iter().map(|a| a.intent.clone()));
    intent_names.extend(params.rulebase.failures.iter().map(|f| f.intent.clone()));
    let event_type_names: Vec<String> = world.events.iter().map(|e| e.name.clone()).collect();

    // ---- Phases A + B, STREAMED day by day (bounded memory, §9 notes) ------
    // Phase A draws candidates keyed on the absolute DAY, so the candidate
    // stream is identical for every staleness block length L and any thread
    // count (§8). Phase B consumes candidates as days are produced, instead of
    // materializing the whole run in RAM first:
    //   * guards OFF (default): the Phase-B sweep reads no state at all (the
    //     output is proven L-independent), so each day is written out as soon
    //     as it is generated and immediately freed — the pre-Phase-1 fully
    //     streaming pipeline and its memory profile, byte-identical output.
    //     Effects still fold into `state` in canonical time order for the
    //     node export; that order is the same under any block partition, so
    //     the exported state is L-independent too.
    //   * guards ON: candidates buffer only up to the current L-block
    //     boundary. A block is swept as soon as no future day can add events
    //     to it: guards read the frozen block-start snapshot (S_{b-1}), kept
    //     events are written, the block's effects merge at the boundary in
    //     canonical order, and the consumed prefix is freed. Peak buffering is
    //     O(max(L, 1 day)) of candidates. For L = span this degenerates to
    //     buffering the whole run — inherent to the semantics (the single
    //     block only completes when the run ends), documented in §9/§10.
    // The sweep is single-threaded over the deterministic candidate stream,
    // and the streamed block/run decomposition is identical to a whole-run
    // sweep, so guard decisions AND final node state are schedule-independent
    // (Theorem 1): bit-identical for any thread count at a fixed L.
    let memories = init_memories(&world);
    let n_windows = params.span_days.ceil() as u64;
    let mut sweep = BlockSweep::new(params, &world, intent_names.len());
    // Phase 4: the exchange endpoint of the real sharded run. Active for
    // guards-on sharded runs (required) and for guards-off sharded runs that
    // opt in (which completes the exported node state, fixing the documented
    // no-exchange dst-effect caveat).
    let mut exchanger = match &params.exchange_dir {
        Some(dir) => Some(Exchanger::new(
            dir.clone(),
            params.shard_index,
            params.shard_count,
            params.exchange_timeout_s,
        )?),
        None => None,
    };
    // The exchange (like guards) needs the L-block sweep over a pending
    // buffer; plain guards-off runs stream day by day with no buffering.
    let buffered = params.guards_enabled || exchanger.is_some();
    // Pending buffer: candidates not yet in a completed L-block.
    let mut pending = EventBatch {
        attr_names: world.attr_names.clone(),
        attrs: vec![Vec::new(); world.attr_names.len()],
        ..Default::default()
    };

    // ---- Pipelined day loop (consistency model section 15) --------------
    // Phase A of day w+1 runs on the rayon pool while the main thread does
    // day w's Phase-B work (sweep / keep_all / sink handoff). This is a pure
    // scheduling overlap: Phase A reads only `params`, `world` (whose `state`
    // is the immutable initial state), `plan`, the marked sets and its own
    // per-actor `memories`, never the sweep's running state `S_b`, so it can
    // run ahead of the sweep; the sweep still consumes days strictly in order
    // w = 0, 1, 2, ... and every block of day w+1 is filtered against the
    // state left after ALL of day w's blocks merged, exactly as before. The
    // producer thread runs the days sequentially (the memories carry across
    // days) and hands each finished day over a rendezvous channel, so it is
    // at most ONE day ahead of the consumer (bounded double buffering).
    let (day_tx, day_rx) = std::sync::mpsc::sync_channel::<(u64, EventBatch)>(0);
    let consumed: anyhow::Result<()> = std::thread::scope(|scope| {
        let producer = {
            let (world, plan, marked_by_type, marked_set, pool) =
                (world, plan, marked_by_type, marked_set, &pool);
            let mut memories = memories;
            scope.spawn(move || {
                for w in 0..n_windows {
                    let day = phase_a_day(
                        params, world, plan, pool, &mut memories, w, marked_by_type, marked_set,
                    );
                    if day_tx.send((w, day)).is_err() {
                        // The consumer stopped early (sink error); nothing
                        // more to produce.
                        return;
                    }
                }
            })
        };
        let consumed = (|| -> anyhow::Result<()> {
            for (w, day) in day_rx.iter() {
                if buffered {
                    if pending.is_empty() {
                        // Nothing is waiting: the day IS the pending buffer
                        // (a move; same rows, same order, same schema).
                        pending = day;
                    } else {
                        append_batch(&mut pending, &day);
                        drop(day);
                    }
                    // Earliest timestamp any FUTURE day can emit: the next
                    // day's start, rounded down to the output granularity
                    // (per-day rounding above rounds down, so a later day's
                    // event can land just below the raw boundary when the
                    // granularity does not divide it). Every block strictly
                    // before that timestamp's block is complete.
                    let window_len_days = (params.span_days - w as f64).min(1.0);
                    let day_end_s = params.epoch_unix
                        + (w as i64) * 86_400
                        + (window_len_days * 86_400.0) as i64;
                    let min_future_t = round_to_granularity(day_end_s, params.granularity_s);
                    let limit = sweep.block_of(min_future_t);
                    sweep.sweep(&world, &mut pending, limit, sink, exchanger.as_mut())?;
                } else if !day.is_empty() {
                    // Guards off: nothing reads state; fold the effects, then
                    // give the day to the sink (moved, not copied) and drop it.
                    sweep.keep_all(&world, &day);
                    sink.write_batch_owned(day)?;
                }
                if let Some(p) = progress.as_mut() {
                    p(sweep.edges_total, (w + 1) as f64 / n_windows.max(1) as f64);
                }
            }
            Ok(())
        })();
        // On a consumer error, dropping the receiver unblocks the producer's
        // pending send so the scope can join it.
        drop(day_rx);
        if producer.join().is_err() {
            anyhow::bail!("Phase A producer thread panicked");
        }
        consumed
    });
    consumed?;
    // Flush the remaining (now complete) blocks, including the final partial
    // one; for L >= span this is where the single whole-run block is swept.
    // Under the exchange the flush limit must be the CANONICAL end of the
    // block schedule (identical on every shard), not i64::MAX, so all shards
    // run the same number of barrier rounds.
    if buffered {
        let final_limit = if exchanger.is_some() {
            sweep.final_block_limit(params)
        } else {
            i64::MAX
        };
        sweep.sweep(&world, &mut pending, final_limit, sink, exchanger.as_mut())?;
    }
    if let Some(ex) = &exchanger {
        ex.tidy();
    }
    let BlockSweep {
        state,
        overdraft_guarded,
        overdraft_violations,
        edges_total,
        edges_per_intent,
        tally,
        ..
    } = sweep;

    // Node export with final state, then close the sink. Under sharding each
    // shard writes only the node rows it owns (disjoint by id), so the union
    // of shards is the whole node set exactly once. The final state is
    // swapped into the world for the export and the initial state swapped
    // back afterwards (a calibration pass must leave the world untouched).
    let mut state = state;
    std::mem::swap(&mut world_mut.state, &mut state);
    for mut nb in world_mut.node_batches() {
        if params.shard_count > 1 {
            filter_node_batch(&mut nb, params);
        }
        sink.write_nodes(&nb)?;
    }
    std::mem::swap(&mut world_mut.state, &mut state);
    sink.finish()?;

    let wall = t0.elapsed().as_secs_f64();
    let mut s = summary(
        params, edges_total, edges_per_intent, intent_names, event_type_names,
        world_mut.attr_dictionaries.clone(), plan.records.clone(), wall,
        overdraft_guarded, overdraft_violations,
    );
    if let Some(ex) = &exchanger {
        s.exchange_rounds = ex.rounds;
        s.exchange_deltas_sent = ex.deltas_sent;
        s.exchange_bytes_sent = ex.bytes_sent;
        s.exchange_deltas_received = ex.deltas_received;
        s.exchange_bytes_received = ex.bytes_received;
        s.exchange_idle_polls = ex.idle_polls;
        s.exchange_nudges = ex.nudges;
        s.exchange_skewed_opens = ex.skewed_opens;
    }
    Ok((s, tally))
}

#[allow(clippy::too_many_arguments)]
fn summary(
    params: &GenParams,
    edges: u64,
    edges_per_intent: Vec<u64>,
    intent_names: Vec<String>,
    event_type_names: Vec<String>,
    attr_dictionaries: Vec<(String, Vec<String>)>,
    ground_truth: Vec<crate::api::AnomalyRecord>,
    wall: f64,
    overdraft_guarded: u64,
    overdraft_violations: u64,
) -> GenSummary {
    GenSummary {
        nodes: params.nodes,
        edges_written: edges,
        anomalous_edges: edges_per_intent.iter().skip(1).sum(),
        intent_names,
        event_type_names,
        edges_per_intent,
        attr_dictionaries,
        ground_truth,
        wall_time_s: wall,
        events_per_sec: edges as f64 / wall.max(1e-9),
        overdraft_guarded,
        overdraft_violations,
        exchange_rounds: 0,
        exchange_deltas_sent: 0,
        exchange_bytes_sent: 0,
        exchange_deltas_received: 0,
        exchange_bytes_received: 0,
        exchange_idle_polls: 0,
        exchange_nudges: 0,
        exchange_skewed_opens: 0,
        planned_anomalous_fraction: f64::NAN,
        calibration_passes: 0,
        calibration_target_fraction: f64::NAN,
        calibration_uncorrected_fraction: f64::NAN,
        calibration_multipliers: Vec::new(),
    }
}

/// Generate one actor-chunk of one behavior in one window.
#[allow(clippy::too_many_arguments)]
fn gen_chunk(
    params: &GenParams,
    world: &World,
    plan: &AnomalyPlan,
    b: &CompiledBehavior,
    bi: u64,
    window: u64,
    window_len_days: f64,
    weekday: usize,
    actors: &[u64],
    actor_offset: usize,
    mem: &mut [u64],
    marked_by_type: &[Vec<u64>],
    marked_set: &HashSet<u64>,
) -> EventBatch {
    let n_cols = world.attr_names.len();
    let homophily = plan.homophily;
    let mut out = EventBatch {
        attr_names: Vec::new(), // filled at merge
        attrs: vec![Vec::new(); n_cols],
        ..Default::default()
    };
    let window_start_s = params.epoch_unix + (window as i64) * 86_400;
    let window_len_s = window_len_days * 86_400.0;
    let window_end_s = window_start_s + window_len_s as i64;
    let mem_stride = b.emissions.len() * MEM_K;

    for (ai, &actor) in actors.iter().enumerate() {
        // Distributed sharding: this shard emits only edges sourced at nodes
        // it owns. Skipping is safe because each actor's stream is independent
        // of every other's — the slices union to the whole-graph output.
        if !in_shard(actor, params) {
            continue;
        }
        let mut rng = stream(
            params.seed,
            StreamPurpose::ActorWindow,
            actor,
            (window << 8) | (bi & 0xff),
        );
        let activity = b.activity[actor_offset + ai] as f64;
        let mut lam = b.rate_per_day
            * activity
            * world.rate_scale
            * b.weekly[weekday]
            * b.diurnal_max
            * window_len_days;

        // Failure incidents touching this actor in this window (M5).
        let spans: Option<&Vec<FailureSpan>> = plan.failures_by_actor.get(&actor);
        if let Some(spans) = spans {
            for sp in spans {
                if sp.start_s < window_end_s && sp.end_s > window_start_s {
                    if let PlannedFailureMode::RateShift(f) = sp.mode {
                        let ov = overlap_frac(sp, window_start_s, window_end_s);
                        lam *= 1.0 + (f - 1.0) * ov;
                    }
                }
            }
        }
        if lam <= 0.0 {
            continue;
        }
        // Candidate arrival times. The COUNT is always Poisson(lam) so the edge
        // budget is exact. Poisson arrival places them uniformly; Weibull
        // arrival gives them heavy-tailed spacing (draw N gaps ~ Weibull(1,
        // shape), cumulate, normalize to the window) — shape < 1 → bursty
        // within-window inter-event times (Barabási 2005). Budget-neutral
        // because the count is unchanged.
        let n_candidates = sample_poisson(&mut rng, lam);
        let weibull_times: Option<Vec<f64>> = b.weibull_shape.map(|k| {
            let mut cum = Vec::with_capacity(n_candidates as usize);
            let mut total = 0.0;
            for _ in 0..n_candidates {
                total += (-rng.gen::<f64>().max(1e-12).ln()).powf(1.0 / k);
                cum.push(total);
            }
            // Normalize to the window, then rotate by a uniform phase (mod the
            // window) so the heavy-tailed sequence is stationary — otherwise it
            // front-loads into the window start (midnight, low diurnal) and
            // thinning would bias the count. With the phase, diurnal acceptance
            // averages out and the edge budget matches the Poisson path.
            if total > 0.0 {
                let phase = rng.gen::<f64>() * window_len_s;
                for c in cum.iter_mut() {
                    *c = (*c / total * window_len_s + phase) % window_len_s;
                }
            }
            cum
        });
        for ci in 0..n_candidates as usize {
            let t_s = match &weibull_times {
                Some(v) => v[ci],
                None => rng.gen::<f64>() * window_len_s,
            };
            let hour = ((t_s / 3600.0) as usize).min(23) % 24;
            // Thinning: accept by diurnal shape.
            if rng.gen::<f64>() >= b.diurnal[hour] / b.diurnal_max {
                continue;
            }
            let t_abs = window_start_s + t_s as i64;
            if is_silenced(spans, t_abs) {
                continue;
            }
            let mem_base = ai * mem_stride;
            if let Some(row) = emit(world, b, actor, t_abs, &mut rng, &mut out, mem, mem_base, homophily, marked_by_type, marked_set) {
                corrupt_row(spans, &mut out, row, &mut rng);
            }

            // Hawkes self-excitation (recursive cascade): each event spawns
            // Poisson(n) children at Exp(decay) delays, each spawning its own —
            // "active stays active" clustering that lifts burstiness toward
            // real interaction data. Subcritical n<1 so it terminates; capped
            // for safety. Replaces the simple geometric burst when set.
            if b.branching_ratio > 0.0 {
                let mut stack: Vec<f64> = vec![t_s];
                let mut emitted = 0u32;
                const HAWKES_CAP: u32 = 4096;
                while let Some(parent_t) = stack.pop() {
                    let k = sample_poisson(&mut rng, b.branching_ratio);
                    for _ in 0..k {
                        let ct = parent_t + sample_exp(&mut rng, b.excitation_decay_s);
                        if ct >= window_len_s {
                            continue;
                        }
                        let t_abs = window_start_s + ct as i64;
                        if !is_silenced(spans, t_abs) {
                            if let Some(row) =
                                emit(world, b, actor, t_abs, &mut rng, &mut out, mem, mem_base, homophily, marked_by_type, marked_set)
                            {
                                corrupt_row(spans, &mut out, row, &mut rng);
                            }
                        }
                        stack.push(ct);
                        emitted += 1;
                        if emitted >= HAWKES_CAP {
                            stack.clear();
                            break;
                        }
                    }
                }
            } else if b.burst_p > 0.0 && rng.gen::<f64>() < b.burst_p {
                let extra = sample_geometric(&mut rng, b.burst_mean_len);
                let mut bt = t_s;
                for _ in 0..extra {
                    bt += sample_exp(&mut rng, BURST_GAP_MEAN_S);
                    if bt >= window_len_s {
                        break;
                    }
                    let t_abs = window_start_s + bt as i64;
                    if is_silenced(spans, t_abs) {
                        continue;
                    }
                    if let Some(row) =
                        emit(world, b, actor, t_abs, &mut rng, &mut out, mem, mem_base, homophily, marked_by_type, marked_set)
                    {
                        corrupt_row(spans, &mut out, row, &mut rng);
                    }
                }
            }
        }
    }
    out
}

#[inline]
fn overlap_frac(sp: &FailureSpan, ws: i64, we: i64) -> f64 {
    let lo = sp.start_s.max(ws);
    let hi = sp.end_s.min(we);
    ((hi - lo).max(0)) as f64 / ((we - ws).max(1)) as f64
}

#[inline]
fn is_silenced(spans: Option<&Vec<FailureSpan>>, t: i64) -> bool {
    spans.is_some_and(|spans| {
        spans.iter().any(|sp| {
            matches!(sp.mode, PlannedFailureMode::Silence) && t >= sp.start_s && t < sp.end_s
        })
    })
}

/// Apply attribute-corrupting failure modes to a just-emitted row and label
/// it with the failure's intent (label = cause, §3).
fn corrupt_row(
    spans: Option<&Vec<FailureSpan>>,
    out: &mut EventBatch,
    row: usize,
    rng: &mut Rng64,
) {
    let Some(spans) = spans else { return };
    let t = out.t[row];
    let ev = out.event_type[row] as usize;
    for sp in spans {
        if t < sp.start_s || t >= sp.end_s {
            continue;
        }
        match &sp.mode {
            PlannedFailureMode::StuckAttr(event, col, value) if *event == ev => {
                out.attrs[*col][row] = *value;
                out.label[row] = sp.intent;
                out.anomaly_id[row] = sp.id;
            }
            PlannedFailureMode::DriftAttr(event, col, per_day) if *event == ev => {
                let days = (t - sp.start_s) as f64 / 86_400.0;
                out.attrs[*col][row] += per_day * days;
                out.label[row] = sp.intent;
                out.anomaly_id[row] = sp.id;
            }
            PlannedFailureMode::NoiseAttr(event, col, sampler) if *event == ev => {
                out.attrs[*col][row] = sampler.sample(rng);
                out.label[row] = sp.intent;
                out.anomaly_id[row] = sp.id;
            }
            // RateShift: the anomaly IS the volume shift, so every event the
            // affected actor emits during the span is part of it (the slow
            // traversals of a congestion jam, the surge of an incident).
            // Label only if still normal, so attr-corrupting modes that share
            // the actor win the label.
            PlannedFailureMode::RateShift(_) if out.label[row] == 0 => {
                out.label[row] = sp.intent;
                out.anomaly_id[row] = sp.id;
            }
            // Silence drops events upstream — its signature is missing data,
            // which has no edge to label (documented in stats).
            _ => {}
        }
    }
}

/// Generate one campaign's events within one window.
#[allow(clippy::too_many_arguments)]
fn gen_campaign_window(
    params: &GenParams,
    world: &World,
    c: &Campaign,
    ci: u64,
    _window: u64,
    window_start_s: i64,
    window_end_s: i64,
    anomaly_rate_scale: f64,
    guard_calibration: &[f64],
    marked_by_type: &[Vec<u64>],
    marked_set: &HashSet<u64>,
) -> EventBatch {
    let n_cols = world.attr_names.len();
    let mut out = EventBatch {
        attr_names: Vec::new(),
        attrs: vec![Vec::new(); n_cols],
        ..Default::default()
    };
    let mut rng = stream(
        params.seed,
        StreamPurpose::Campaign,
        10_000 + ci,
        (window_start_s - params.epoch_unix).max(0) as u64 / 86_400,
    );
    let n_members = c.members.len();
    if n_members == 0 {
        return out;
    }
    for stage in &c.stages {
        // Shared with the planner (see `anomaly::stage_window`) so the volume
        // the cap is computed from is exactly the volume emitted here.
        let Some((t0, t1)) = crate::anomaly::stage_window(stage, window_start_s, window_end_s)
        else {
            continue;
        };
        let days = (t1 - t0) as f64 / 86_400.0;
        let mut stage_rate = stage.rate_per_day * anomaly_rate_scale;
        // Guard-aware calibration (section 17): a per-event-type multiplier
        // set between calibration passes. The slice is EMPTY unless a
        // calibration ran, so the uncalibrated arithmetic is untouched.
        if let Some(&g) = guard_calibration.get(stage.event) {
            stage_rate *= g;
        }
        let ev = &world.events[stage.event];
        // Which members emit in this stage. Shared with the planner so the
        // calibration cap is computed against exactly this volume.
        let acting: &[u64] = crate::anomaly::acting_slice(c, stage);
        for (mi, &member) in acting.iter().enumerate() {
            let n = sample_poisson(&mut rng, stage_rate * days);
            for _ in 0..n {
                let t = t0 + (rng.gen::<f64>() * (t1 - t0) as f64) as i64;
                // The scope picks the COUNTERPARTY; which end of the edge it
                // lands on is decided by `scope.member_is_dst()` below.
                let cp = match &stage.scope {
                    PlannedScope::Ring => {
                        if n_members < 2 {
                            continue;
                        }
                        let mut d = c.members[rng.gen_range(0..n_members)];
                        if d == member {
                            d = c.members[(mi + 1) % n_members];
                        }
                        d
                    }
                    PlannedScope::Chain => c.members[(mi + 1) % n_members],
                    PlannedScope::Collector(anchor) => *anchor,
                    // Star fan-out: `member` IS members[0] (the operator; see
                    // `acting_slice`), paying a uniformly-chosen investor. No
                    // investor-to-investor edge is ever emitted, so the motif
                    // stays a star instead of collapsing to a clique.
                    PlannedScope::Hub => {
                        if n_members < 2 {
                            continue;
                        }
                        c.members[1 + rng.gen_range(0..n_members - 1)]
                    }
                    PlannedScope::Normal(cp) => match *cp {
                        CompiledCounterparty::Neighbor { relation } => {
                            let rel = &world.skeletons[relation];
                            let neigh = rel.neighbors_of(member);
                            if neigh.is_empty() {
                                rel.dst_start + rng.gen_range(0..rel.n_dst)
                            } else {
                                neigh[rng.gen_range(0..neigh.len())]
                            }
                        }
                        CompiledCounterparty::GlobalUniform { entity } => {
                            world.entities.starts[entity]
                                + rng.gen_range(0..world.entities.counts[entity])
                        }
                        CompiledCounterparty::GlobalPopularity { entity, pop } => {
                            world.entities.starts[entity]
                                + world.popularity[pop].1.sample(&mut rng) as u64
                        }
                        CompiledCounterparty::RepeatOrNeighbor { relation, .. } => {
                            let rel = &world.skeletons[relation];
                            let neigh = rel.neighbors_of(member);
                            if neigh.is_empty() {
                                rel.dst_start + rng.gen_range(0..rel.n_dst)
                            } else {
                                neigh[rng.gen_range(0..neigh.len())]
                            }
                        }
                    },
                    PlannedScope::Victims(pool) | PlannedScope::Sources(pool) => {
                        if pool.is_empty() {
                            continue;
                        }
                        pool[rng.gen_range(0..pool.len())]
                    }
                };
                // Relation camouflage (axis 2, structural): with prob c.camouflage,
                // reroute the edge to a benign counterparty, diluting the
                // fan-in / fan-out / ring / chain motif so that a
                // structure-aware detector also degrades as difficulty rises
                // (complements the attribute camouflage applied below). The
                // replacement is drawn from the COUNTERPARTY's entity type,
                // which is the event's src type for fan-in (`sources`) scopes
                // and its dst type otherwise — camouflage must never silently
                // flip the edge's direction or its endpoint types. The rng
                // draws depend only on shared campaign state, so shards stay
                // bit-identical.
                let cp_ty = stage.scope.counterparty_type(ev);
                let cp = if c.camouflage > 0.0
                    && world.entities.counts[cp_ty] > 0
                    && rng.gen::<f64>() < c.camouflage
                {
                    world.entities.starts[cp_ty] + rng.gen_range(0..world.entities.counts[cp_ty])
                } else {
                    cp
                };
                // Homophily (fidelity axis): real illicit actors preferentially
                // transact with each other. With prob c.homophily, redirect a
                // counterparty that is not already marked to a random marked
                // actor of the SAME entity type, raising anomaly homophily toward
                // real levels without disturbing the structural motifs (Ring/
                // Chain/Hub counterparties are already marked, so the not-marked
                // guard skips them). Draws depend only on shared campaign state
                // (marked_by_type and marked_set derive from the deterministic
                // plan; cp is computed shard-independently), so shards stay
                // bit-identical.
                let cp = if c.homophily > 0.0 {
                    let pool = &marked_by_type[cp_ty];
                    if !pool.is_empty() && !marked_set.contains(&cp) && rng.gen::<f64>() < c.homophily {
                        pool[rng.gen_range(0..pool.len())]
                    } else {
                        cp
                    }
                } else {
                    cp
                };
                if cp == member {
                    continue;
                }
                // Place the member on the correct end: fan-in scopes make the
                // counterparty the SOURCE and the member the DESTINATION.
                let (e_src, e_dst) = if stage.scope.member_is_dst() {
                    (cp, member)
                } else {
                    (member, cp)
                };
                // Draw all attributes FIRST (they consume rng) regardless of
                // shard ownership, so every shard replays the identical
                // campaign rng stream and keeps bit-identical edges for the
                // members it owns. Only the append below is shard-gated.
                let mut drawn: Vec<(usize, f64)> = Vec::with_capacity(ev.attrs.len());
                for ca in &ev.attrs {
                    let v = match &ca.sampler {
                        AttrSampler::Numeric(d) => d.sample(&mut rng),
                        AttrSampler::Category { table, code_map } => {
                            code_map[table.sample(&mut rng)] as f64
                        }
                        AttrSampler::Flag(p) => f64::from(rng.gen::<f64>() < *p),
                    };
                    drawn.push((ca.col, v));
                }
                // …then the adversary's override wins with prob (1 − c_eff):
                // feature camouflage blends toward the normal distribution.
                for (col, sampler) in &stage.overrides {
                    if rng.gen::<f64>() >= c.camouflage {
                        let v = sampler.sample(&mut rng);
                        if let Some(slot) = drawn.iter_mut().find(|(c, _)| c == col) {
                            slot.1 = v;
                        } else {
                            drawn.push((*col, v));
                        }
                    }
                }
                // Shard ownership is keyed on the edge's SRC, upholding the
                // graph-wide contract that shard k holds exactly the edges
                // sourced at the nodes it owns (`filter_node_batch` partitions
                // nodes the same way). For fan-in scopes the src is the outside
                // payer rather than the acting member, so this is NOT
                // `in_shard(member)` — but every shard replays the whole
                // campaign stream and only filters at the end, so `e_src` is
                // identical everywhere and each edge is still claimed exactly
                // once. Union over shards == whole graph, bit for bit.
                if !in_shard(e_src, params) {
                    continue;
                }
                out.src.push(e_src);
                out.dst.push(e_dst);
                out.t.push(t);
                out.event_type.push(stage.event as u16);
                out.label.push(c.intent);
                out.anomaly_id.push(c.id);
                for col in out.attrs.iter_mut() {
                    col.push(f64::NAN);
                }
                let row = out.src.len() - 1;
                for (col, v) in drawn {
                    out.attrs[col][row] = v;
                }
            }
        }
    }
    out
}

/// Emit one event for `actor` at `t`: choose emission, counterparty, attrs.
/// Returns the row index written (None if the event was skipped).
#[allow(clippy::too_many_arguments)]
#[inline]
fn emit(
    world: &World,
    b: &CompiledBehavior,
    actor: u64,
    t: i64,
    rng: &mut Rng64,
    out: &mut EventBatch,
    mem: &mut [u64],
    mem_base: usize,
    homophily: f64,
    marked_by_type: &[Vec<u64>],
    marked_set: &HashSet<u64>,
) -> Option<usize> {
    let em_i = b.emission_alias.sample(rng);
    let em = &b.emissions[em_i];
    let ev = &world.events[em.event];

    let dst = match &em.counterparty {
        CompiledCounterparty::Neighbor { relation } => {
            pick_neighbor(world, *relation, actor, rng)
        }
        CompiledCounterparty::RepeatOrNeighbor { relation, repeat_p } => {
            let slot = &mut mem[mem_base + em_i * MEM_K..mem_base + em_i * MEM_K + MEM_K];
            let remembered = slot.iter().filter(|&&x| x != MEM_EMPTY).count();
            if remembered > 0 && rng.gen::<f64>() < *repeat_p {
                slot[rng.gen_range(0..remembered)]
            } else {
                let d = pick_neighbor(world, *relation, actor, rng);
                // Ring update: shift in the new partner.
                slot[1] = slot[0];
                slot[0] = d;
                d
            }
        }
        CompiledCounterparty::GlobalUniform { entity } => {
            let start = world.entities.starts[*entity];
            let n = world.entities.counts[*entity];
            let mut d = start + rng.gen_range(0..n);
            if d == actor && n > 1 {
                d = start + (d - start + 1) % n;
            }
            d
        }
        CompiledCounterparty::GlobalPopularity { entity, pop } => {
            let start = world.entities.starts[*entity];
            let mut d = start + world.popularity[*pop].1.sample(rng) as u64;
            if d == actor && world.entities.counts[*entity] > 1 {
                d = start + (d - start + 1) % world.entities.counts[*entity];
            }
            d
        }
    };
    let mut dst = dst_or_skip(dst)?;

    // Anomaly homophily on the NORMAL path (fidelity axis, extends P1-a): a real
    // illicit actor's WHOLE edge set skews illicit, not only its campaign edges.
    // So when the emitting actor is marked, redirect this background edge to a
    // random marked actor of the chosen dst's entity type with prob `homophily`.
    // This is the lever the campaign-only redirect lacked: marked actors' normal
    // traffic otherwise dilutes P(dst marked | src marked) and caps homophily.
    //
    // DETERMINISM: the normal path is sharded by the emitting node, and this draw
    // consumes the actor's OWN normal rng stream, so it is identical regardless of
    // thread/shard count (an actor is always produced on its owning shard). Guarded
    // by `homophily > 0.0` and `marked_set.contains(&actor)` so that no rng is
    // drawn when the control is 0 or the actor is benign — an exact no-op for every
    // domain that leaves homophily unset, preserving their bit-identical output.
    if homophily > 0.0 && marked_set.contains(&actor) {
        let pool = &marked_by_type[world.entities.type_of(dst)];
        if !pool.is_empty() && !marked_set.contains(&dst) && rng.gen::<f64>() < homophily {
            let cand = pool[rng.gen_range(0..pool.len())];
            if cand != actor {
                dst = cand;
            }
        }
    }

    out.src.push(actor);
    out.dst.push(dst);
    out.t.push(t);
    out.event_type.push(em.event as u16);
    out.label.push(0);
    out.anomaly_id.push(-1); // normal until a failure span corrupts/labels it
    // Union columns: NaN by default, overwritten by this event's attrs.
    for col in out.attrs.iter_mut() {
        col.push(f64::NAN);
    }
    let row = out.src.len() - 1;
    for ca in &ev.attrs {
        let v = match &ca.sampler {
            AttrSampler::Numeric(d) => d.sample(rng),
            AttrSampler::Category { table, code_map } => code_map[table.sample(rng)] as f64,
            AttrSampler::Flag(p) => f64::from(rng.gen::<f64>() < *p),
        };
        out.attrs[ca.col][row] = v;
    }
    Some(row)
}

#[inline]
fn dst_or_skip(dst: u64) -> Option<u64> {
    (dst != u64::MAX).then_some(dst)
}

#[inline]
fn pick_neighbor(world: &World, relation: usize, actor: u64, rng: &mut Rng64) -> u64 {
    let rel = &world.skeletons[relation];
    let neigh = rel.neighbors_of(actor);
    if neigh.is_empty() {
        // Isolated in this relation: fall back to a uniform dst (keeps the
        // event budget; rare because skeleton builders floor degree at 1).
        rel.dst_start + rng.gen_range(0..rel.n_dst)
    } else {
        neigh[rng.gen_range(0..neigh.len())]
    }
}

/// Merge parts and sort by (t, src, dst, event_type) — a total order that
/// makes output independent of the parallel schedule.
fn merge_sorted(parts: Vec<EventBatch>, attr_names: Vec<String>) -> EventBatch {
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let n_cols = attr_names.len();
    let mut merged = EventBatch {
        src: Vec::with_capacity(total),
        dst: Vec::with_capacity(total),
        t: Vec::with_capacity(total),
        event_type: Vec::with_capacity(total),
        label: Vec::with_capacity(total),
        anomaly_id: Vec::with_capacity(total),
        attrs: vec![Vec::with_capacity(total); n_cols],
        attr_names,
    };
    for p in parts {
        merged.src.extend_from_slice(&p.src);
        merged.dst.extend_from_slice(&p.dst);
        merged.t.extend_from_slice(&p.t);
        merged.event_type.extend_from_slice(&p.event_type);
        merged.label.extend_from_slice(&p.label);
        merged.anomaly_id.extend_from_slice(&p.anomaly_id);
        for (c, col) in p.attrs.into_iter().enumerate() {
            merged.attrs[c].extend_from_slice(&col);
        }
    }
    // Parallel argsort permutation, then apply to every column in parallel.
    let mut idx: Vec<u32> = (0..total as u32).collect();
    idx.par_sort_unstable_by_key(|&i| {
        let i = i as usize;
        (merged.t[i], merged.src[i], merged.dst[i], merged.event_type[i])
    });
    apply_perm(&mut merged, &idx);
    merged
}

fn apply_perm(b: &mut EventBatch, idx: &[u32]) {
    fn perm<T: Copy + Send + Sync>(v: &[T], idx: &[u32]) -> Vec<T> {
        idx.par_iter().map(|&i| v[i as usize]).collect()
    }
    // Permute the fixed columns and every attr column concurrently.
    let (src, dst, t, et, label) = {
        let (a, b2) = rayon::join(
            || rayon::join(|| perm(&b.src, idx), || perm(&b.dst, idx)),
            || {
                rayon::join(
                    || perm(&b.t, idx),
                    || rayon::join(|| perm(&b.event_type, idx), || perm(&b.label, idx)),
                )
            },
        );
        let ((src, dst), (t, (et, label))) = (a, b2);
        (src, dst, t, et, label)
    };
    b.src = src;
    b.dst = dst;
    b.t = t;
    b.event_type = et;
    b.label = label;
    b.anomaly_id = perm(&b.anomaly_id, idx);
    let attrs = std::mem::take(&mut b.attrs);
    b.attrs = attrs.into_par_iter().map(|col| perm(&col, idx)).collect();
}

/// Apply state effects of every event in `batch` in the CANONICAL fold order:
/// batch order (which is sorted by `(t, src, dst, event_type)`) with full-key
/// TIE runs re-ordered by the content key `(effect_index, amount bits)`.
///
/// Why ties need a content order (Phase 4): the batch sort is unstable, so two
/// events with an identical `(t, src, dst, event_type)` key can appear in
/// either order, and that artifact differs between a whole-graph array and a
/// shard's smaller array. Float folds do not associate, so an artifact-ordered
/// fold cannot be reproduced across process counts. Keying tied deltas by
/// their CONTENT makes the per-cell fold order a pure function of the delta
/// multiset: any process (or the exchange merge, which ships exactly these
/// fields) reproduces it bit for bit. Full-content duplicates are identical
/// deltas, so their mutual order is irrelevant. Non-tied rows keep batch
/// order, so runs without key collisions fold exactly as before.
fn fold_canonical(world: &World, state: &mut [Vec<Vec<f64>>], batch: &EventBatch) {
    let n = batch.len();
    let mut i = 0usize;
    while i < n {
        let mut j = i + 1;
        while j < n
            && batch.t[j] == batch.t[i]
            && batch.src[j] == batch.src[i]
            && batch.dst[j] == batch.dst[i]
            && batch.event_type[j] == batch.event_type[i]
        {
            j += 1;
        }
        if j == i + 1 {
            apply_event(world, state, batch, i);
        } else {
            // Tie run (same event type throughout): canonical content order.
            let ev = &world.events[batch.event_type[i] as usize];
            let mut deltas: Vec<(u8, u64, f64)> = Vec::new();
            for r in i..j {
                for (ei, eff) in ev.effects.iter().enumerate() {
                    let amount = effect_amount(eff, batch, r);
                    if effect_is_noop(eff, amount) {
                        continue;
                    }
                    deltas.push((ei as u8, amount.to_bits(), amount));
                }
            }
            deltas.sort_unstable_by_key(|&(ei, bits, _)| (ei, bits));
            for &(ei, _, amount) in &deltas {
                apply_resolved(world, state, &ev.effects[ei as usize], batch.src[i], batch.dst[i], amount);
            }
        }
        i = j;
    }
}

/// The resolved operand of one effect on batch row `r`: the attr value for
/// Add/Sub, 1.0 for Increment, the rule constant for Set.
#[inline]
fn effect_amount(eff: &crate::world::CompiledEffect, batch: &EventBatch, r: usize) -> f64 {
    match eff.kind {
        EffectKind::Add | EffectKind::Sub => batch.attrs[eff.from_col][r],
        EffectKind::Increment => 1.0,
        EffectKind::Set => eff.value,
    }
}

/// A non-finite Add/Sub operand is a no-op (`apply_event` skips it); it is
/// excluded from canonical folds and never shipped over the exchange.
#[inline]
fn effect_is_noop(eff: &crate::world::CompiledEffect, amount: f64) -> bool {
    matches!(eff.kind, EffectKind::Add | EffectKind::Sub) && !amount.is_finite()
}

/// Apply one resolved effect to the state cell it targets.
#[inline]
fn apply_resolved(
    world: &World,
    state: &mut [Vec<Vec<f64>>],
    eff: &crate::world::CompiledEffect,
    src: u64,
    dst: u64,
    amount: f64,
) {
    let node = if eff.on_src { src } else { dst };
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
}

/// The canonical merge key of a shipped/kept delta (Phase 4): the block-merge
/// sort key. The leading `(t, src, dst, event_type)` reproduces batch order;
/// the trailing `(effect_index, amount bits)` is the content order for
/// full-key ties, exactly as [`fold_canonical`] applies locally.
#[inline]
fn delta_key(d: &WireDelta) -> (i64, u64, u64, u16, u8, u64) {
    (d.t, d.src, d.dst, d.event_type, d.effect_index, d.amount.to_bits())
}

/// Resolve a wire delta against this shard's compiled world and apply it.
/// Fails loudly on an out-of-range event/effect (a foreign or stale exchange
/// file), never silently corrupts state.
fn apply_wire_delta(
    world: &World,
    state: &mut [Vec<Vec<f64>>],
    d: &WireDelta,
) -> anyhow::Result<()> {
    let ev = world
        .events
        .get(d.event_type as usize)
        .ok_or_else(|| anyhow::anyhow!("exchange delta names unknown event type {}", d.event_type))?;
    let eff = ev.effects.get(d.effect_index as usize).ok_or_else(|| {
        anyhow::anyhow!(
            "exchange delta names effect {} of event `{}` which has {} effects \
             (mixed runs sharing one exchange dir?)",
            d.effect_index,
            ev.name,
            ev.effects.len()
        )
    })?;
    apply_resolved(world, state, eff, d.src, d.dst, d.amount);
    Ok(())
}

/// Apply just event `i`'s effects. Used both for the per-block merge and for
/// the never-stale exact ledger (which folds one event at a time).
#[inline]
fn apply_event(world: &World, state: &mut [Vec<Vec<f64>>], batch: &EventBatch, i: usize) {
    let ev = &world.events[batch.event_type[i] as usize];
    for eff in &ev.effects {
        let node = if eff.on_src { batch.src[i] } else { batch.dst[i] };
        let local = (node - world.entities.starts[eff.entity]) as usize;
        let slot = &mut state[eff.entity][eff.var][local];
        match eff.kind {
            EffectKind::Add => {
                let v = batch.attrs[eff.from_col][i];
                if v.is_finite() {
                    *slot += v;
                }
            }
            EffectKind::Sub => {
                let v = batch.attrs[eff.from_col][i];
                if v.is_finite() {
                    *slot -= v;
                }
            }
            EffectKind::Increment => *slot += 1.0,
            EffectKind::Set => *slot = eff.value,
        }
    }
}

/// Append every column of `src` onto `dst` (both share the union schema).
fn append_batch(dst: &mut EventBatch, src: &EventBatch) {
    dst.src.extend_from_slice(&src.src);
    dst.dst.extend_from_slice(&src.dst);
    dst.t.extend_from_slice(&src.t);
    dst.event_type.extend_from_slice(&src.event_type);
    dst.label.extend_from_slice(&src.label);
    dst.anomaly_id.extend_from_slice(&src.anomaly_id);
    for (c, col) in src.attrs.iter().enumerate() {
        dst.attrs[c].extend_from_slice(col);
    }
}

/// Drop the first `n` rows of `b` in place (the consumed, already-swept
/// prefix of the streaming pending buffer). Keeps the allocation, so the
/// buffer's capacity stays at the high-water mark of one L-block + one day.
fn drain_prefix(b: &mut EventBatch, n: usize) {
    if n == 0 {
        return;
    }
    b.src.drain(..n);
    b.dst.drain(..n);
    b.t.drain(..n);
    b.event_type.drain(..n);
    b.label.drain(..n);
    b.anomaly_id.drain(..n);
    for col in b.attrs.iter_mut() {
        col.drain(..n);
    }
}

/// Append a single row `i` of `src` onto `dst`.
#[inline]
fn append_row(dst: &mut EventBatch, src: &EventBatch, i: usize) {
    dst.src.push(src.src[i]);
    dst.dst.push(src.dst[i]);
    dst.t.push(src.t[i]);
    dst.event_type.push(src.event_type[i]);
    dst.label.push(src.label[i]);
    dst.anomaly_id.push(src.anomaly_id[i]);
    for (c, col) in src.attrs.iter().enumerate() {
        dst.attrs[c].push(col[i]);
    }
}

// === Phase 2: sharded-equivalence emulation (consistency model §4) ==========
//
// Single-process emulation of the N-shard protocol. It proves that the sharded
// run's edges AND per-node final state equal the single-machine whole-graph
// run's, and measures the per-block cross-shard dst-effect exchange volume,
// WITHOUT a real multi-process deployment (a documented follow-on, §7).
//
// Shard k owns the nodes with `id % N == k` and generates exactly the edges
// sourced at those nodes. Because Phase A partitions strictly by src, the
// whole-graph candidate array split by `src % N` IS each shard's candidate set
// (the invariant the edge-union test already proves), so the emulation draws
// candidates ONCE and routes each event to its shard in Phase B: every guard
// reads its shard's own state slice, and at each block boundary the dst-effects
// whose dst is owned by another shard are shipped to the owner, which merges
// src-effects + received dst-effects in canonical order to form its slice of
// S_b (§4). N == 1 reduces to the whole-graph run.

/// Wire size of one shipped dst-effect delta: target node id (u64) + state-var
/// index (u8) + effect kind (u8) + value (f64). The entity type is derivable
/// from the node id, so it is not shipped. A conservative fixed-width encoding;
/// a real deployment could pack tighter (per-block var-int deltas), so this is
/// an upper bound on the exchange bytes.
pub const SHARD_DELTA_BYTES: usize = 8 + 1 + 1 + 8;

/// Measured cost of the per-block exchange (§4): number and bytes of the
/// cross-shard dst-effect deltas the protocol must ship, per block and total.
#[derive(Clone, Debug)]
pub struct ShardCommStats {
    pub shard_count: u64,
    /// Cross-shard dst-effect deltas shipped at each block boundary.
    pub cross_shard_deltas_per_block: Vec<u64>,
    pub total_cross_shard_deltas: u64,
    pub total_bytes: u64,
    /// Total effect deltas applied (local + shipped), for a shipped-fraction.
    pub total_effect_deltas: u64,
}

/// Canonical edge identity used for multiset comparison.
pub type EdgeKey = (u64, u64, i64, u16, u16, i64);

/// Result of the Phase-2 sharded-equivalence emulation for a given N.
pub struct ShardEquivalence {
    pub shard_count: u64,
    pub entity_names: Vec<String>,
    pub state_names: Vec<Vec<String>>,
    /// Final per-node state, whole-graph reference (the N == 1 sweep), indexed
    /// [entity][var][local].
    pub whole_state: Vec<Vec<Vec<f64>>>,
    /// Final per-node state from the N-shard sweep WITH the per-block exchange.
    pub shard_state: Vec<Vec<Vec<f64>>>,
    /// Final per-node state from the OLD sharded path (NO exchange) — the bug
    /// the exchange fixes: each shard sees only the dst-effects of the edges it
    /// generated, so cross-shard in-edges are missing.
    pub no_exchange_state: Vec<Vec<Vec<f64>>>,
    /// Edge multisets (sorted): whole-graph, sharded-with-exchange, old path.
    pub whole_edges: Vec<EdgeKey>,
    pub shard_edges: Vec<EdgeKey>,
    pub no_exchange_edges: Vec<EdgeKey>,
    pub comm: ShardCommStats,
}

impl ShardEquivalence {
    /// Do the whole-graph and sharded-with-exchange runs agree on all edges?
    pub fn edges_match(&self) -> bool {
        self.whole_edges == self.shard_edges
    }
    /// Do they agree on every per-node state value (compared by f64 bits)?
    pub fn state_matches(&self) -> bool {
        states_bit_equal(&self.whole_state, &self.shard_state)
    }
    /// Number of per-node state cells where the OLD (no-exchange) sharded slice
    /// differs from the whole-graph reference — the size of the bug the
    /// exchange fixes.
    pub fn no_exchange_state_mismatches(&self) -> usize {
        count_state_mismatches(&self.whole_state, &self.no_exchange_state)
    }
}

/// Logical (bit-level) equality of two state arrays; f64 compared by bits.
pub fn states_bit_equal(a: &[Vec<Vec<f64>>], b: &[Vec<Vec<f64>>]) -> bool {
    count_state_mismatches(a, b) == 0
}

fn count_state_mismatches(a: &[Vec<Vec<f64>>], b: &[Vec<Vec<f64>>]) -> usize {
    let mut n = 0;
    for (ea, eb) in a.iter().zip(b.iter()) {
        for (va, vb) in ea.iter().zip(eb.iter()) {
            for (x, y) in va.iter().zip(vb.iter()) {
                if x.to_bits() != y.to_bits() {
                    n += 1;
                }
            }
        }
    }
    n
}

/// One state-effect delta in the emulated exchange.
struct ShardDelta {
    entity: usize,
    var: usize,
    local: usize,
    kind: EffectKind,
    /// Resolved amount: the attr value for Add/Sub, 1.0 for Increment, the
    /// constant for Set. May be NaN for a missing attr (then Add/Sub is a
    /// no-op, exactly as `apply_event`).
    amount: f64,
}

#[inline]
fn apply_shard_delta(state: &mut [Vec<Vec<f64>>], d: &ShardDelta) {
    let slot = &mut state[d.entity][d.var][d.local];
    match d.kind {
        EffectKind::Add => {
            if d.amount.is_finite() {
                *slot += d.amount;
            }
        }
        EffectKind::Sub => {
            if d.amount.is_finite() {
                *slot -= d.amount;
            }
        }
        EffectKind::Increment => *slot += 1.0,
        EffectKind::Set => *slot = d.amount,
    }
}

/// Emulate an N-shard Phase-B sweep over the shared candidate batch `cand`.
///
/// Each shard k holds its own copy of the whole state array but only reads and
/// writes the cells of the nodes it owns (`id % N == k`). Events route to their
/// owning shard by `src % N`; guards read that shard's own slice (own-state
/// read, §4). At each block boundary:
///   * `exchange = true` — every effect delta is delivered to the OWNER of the
///     touched node (src-effects local, dst-effects possibly cross-shard), so
///     each owner's slice is complete before the next block. This is the §4
///     protocol; cross-shard dst-effects are the measured exchange volume.
///   * `exchange = false` — the OLD behavior: the generating shard keeps ALL
///     its events' effects on its OWN slice and ships nothing, so a node's owner
///     never sees in-edges from other shards. Reproduces the latent bug.
///
/// Returns the kept edges (canonical order), the assembled whole-graph state
/// (each node's value taken from its owner's slice), and the comm stats.
fn emulate_shards(
    params: &GenParams,
    world: &World,
    cand: &EventBatch,
    shard_count: u64,
    exchange: bool,
) -> (Vec<EdgeKey>, Vec<Vec<Vec<f64>>>, ShardCommStats) {
    let n_shards = shard_count.max(1) as usize;
    let l_s = (params.block_len_days * 86_400.0).max(1.0);
    let epoch = params.epoch_unix;
    let block_of = |t: i64| ((t - epoch) as f64 / l_s).floor() as i64;

    // Per-shard state slices (whole-shaped; only owned cells are authoritative).
    let mut state: Vec<Vec<Vec<Vec<f64>>>> =
        (0..n_shards).map(|_| world.state.clone()).collect();

    let mut edges: Vec<EdgeKey> = Vec::new();
    let mut per_block: Vec<u64> = Vec::new();
    let mut total_cross = 0u64;
    let mut total_deltas = 0u64;

    let n = cand.len();
    let mut i = 0usize;
    while i < n {
        let blk = block_of(cand.t[i]);
        let mut j = i;
        while j < n && block_of(cand.t[j]) == blk {
            j += 1;
        }
        // Deltas to merge at the boundary, bucketed per destination shard,
        // each tagged with the canonical delta key (t, src, dst, event_type,
        // effect_index, amount bits). Folding a bucket sorted by that key
        // reproduces the whole-graph canonical fold restricted to the shard's
        // nodes: the leading fields are the block's event order, the trailing
        // content fields canonicalize full-key ties exactly as
        // `fold_canonical` (and the Phase-4 exchange merge) do.
        type DeltaKey = (i64, u64, u64, u16, u8, u64);
        let mut deliver: Vec<Vec<(DeltaKey, ShardDelta)>> =
            (0..n_shards).map(|_| Vec::new()).collect();
        let mut block_cross = 0u64;

        for k in i..j {
            let src = cand.src[k];
            let owner_src = (src % shard_count) as usize;
            let ev = &world.events[cand.event_type[k] as usize];
            // Guard reads the SRC's OWN shard slice (own-state read).
            if params.guards_enabled {
                if let Some(g) = &ev.guard {
                    let start = world.entities.starts[g.entity];
                    let bal = state[owner_src][g.entity][g.var][(src - start) as usize];
                    let amt = cand.attrs[g.from_col][k];
                    if amt.is_finite() && bal < amt {
                        continue; // suppressed: never emitted, no effects
                    }
                }
            }
            edges.push((
                src, cand.dst[k], cand.t[k], cand.event_type[k], cand.label[k], cand.anomaly_id[k],
            ));
            for (ei, eff) in ev.effects.iter().enumerate() {
                let node = if eff.on_src { src } else { cand.dst[k] };
                let owner = (node % shard_count) as usize;
                let amount = match eff.kind {
                    EffectKind::Add | EffectKind::Sub => cand.attrs[eff.from_col][k],
                    EffectKind::Increment => 1.0,
                    EffectKind::Set => eff.value,
                };
                let key: DeltaKey = (
                    cand.t[k], src, cand.dst[k], cand.event_type[k], ei as u8, amount.to_bits(),
                );
                let d = ShardDelta {
                    entity: eff.entity,
                    var: eff.var,
                    local: (node - world.entities.starts[eff.entity]) as usize,
                    kind: eff.kind,
                    amount,
                };
                total_deltas += 1;
                if exchange {
                    // Cross-shard dst-effect => shipped over the exchange. A
                    // no-op (NaN Add/Sub) carries nothing, so only count deltas
                    // that actually move state.
                    let noop = matches!(eff.kind, EffectKind::Add | EffectKind::Sub)
                        && !amount.is_finite();
                    if !eff.on_src && owner != owner_src && !noop {
                        block_cross += 1;
                    }
                    deliver[owner].push((key, d));
                } else {
                    // OLD path: the generating shard keeps every effect locally.
                    deliver[owner_src].push((key, d));
                }
            }
        }
        // Boundary merge: each shard folds its delivered deltas in the
        // canonical delta order.
        for (s, ds) in deliver.iter_mut().enumerate() {
            ds.sort_unstable_by_key(|&(key, _)| key);
            for (_, d) in ds.iter() {
                apply_shard_delta(&mut state[s], d);
            }
        }
        per_block.push(block_cross);
        total_cross += block_cross;
        i = j;
    }

    // Assemble the whole-graph state: each node's value from its owner's slice.
    let mut merged = world.state.clone();
    for ety in 0..merged.len() {
        let start = world.entities.starts[ety];
        for vi in 0..merged[ety].len() {
            for local in 0..merged[ety][vi].len() {
                let owner = ((start + local as u64) % shard_count) as usize;
                merged[ety][vi][local] = state[owner][ety][vi][local];
            }
        }
    }

    let comm = ShardCommStats {
        shard_count,
        cross_shard_deltas_per_block: per_block,
        total_cross_shard_deltas: total_cross,
        total_bytes: total_cross * SHARD_DELTA_BYTES as u64,
        total_effect_deltas: total_deltas,
    };
    (edges, merged, comm)
}

/// Run the Phase-2 sharded-equivalence emulation for `shard_count` virtual
/// shards: the single-machine whole-graph reference (N == 1), the N-shard sweep
/// WITH the per-block dst-effect exchange, and the OLD no-exchange path, all
/// over ONE shared candidate draw. This is the measurement hook for §4: it
/// proves edges + per-node state equivalence and reports the exchange volume,
/// with a real multi-process deployment left as a documented follow-on (§7).
pub fn shard_equivalence(
    params: &GenParams,
    shard_count: u64,
) -> anyhow::Result<ShardEquivalence> {
    anyhow::ensure!(shard_count >= 1, "shard_count must be >= 1");
    anyhow::ensure!(
        params.block_len_days.is_finite() && params.block_len_days > 0.0,
        "block_len_days must be a finite value > 0, got {}",
        params.block_len_days
    );
    params.rulebase.validate()?;
    let (world, plan, marked_by_type, marked_set) = prepare(params)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(params.threads.max(1))
        .build()
        .map_err(|e| anyhow::anyhow!("cannot build thread pool: {e}"))?;
    let cand = phase_a(params, &world, &plan, &pool, &marked_by_type, &marked_set);

    let (mut whole_edges, whole_state, _c1) = emulate_shards(params, &world, &cand, 1, true);
    let (mut shard_edges, shard_state, comm) =
        emulate_shards(params, &world, &cand, shard_count, true);
    let (mut noex_edges, no_exchange_state, _c2) =
        emulate_shards(params, &world, &cand, shard_count, false);
    whole_edges.sort_unstable();
    shard_edges.sort_unstable();
    noex_edges.sort_unstable();

    Ok(ShardEquivalence {
        shard_count,
        entity_names: world.entities.names.clone(),
        state_names: world.state_names.clone(),
        whole_state,
        shard_state,
        no_exchange_state,
        whole_edges,
        shard_edges,
        no_exchange_edges: noex_edges,
        comm,
    })
}

// --- small deterministic samplers -------------------------------------------

#[inline]
fn sample_poisson(rng: &mut Rng64, lam: f64) -> u64 {
    if lam < 1e-9 {
        return 0;
    }
    rand_distr::Poisson::new(lam).map(|p| p.sample(rng) as u64).unwrap_or(0)
}

/// Geometric with the given mean (number of burst follow-ups).
#[inline]
fn sample_geometric(rng: &mut Rng64, mean: f64) -> u64 {
    if mean <= 0.0 {
        return 0;
    }
    let p = 1.0 / (1.0 + mean);
    let u: f64 = rng.gen();
    (u.ln() / (1.0 - p).ln()).floor() as u64
}

#[inline]
fn sample_exp(rng: &mut Rng64, mean: f64) -> f64 {
    let u: f64 = rng.gen::<f64>().max(1e-12);
    -mean * u.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_sample::{stream, StreamPurpose};

    /// Replicates the burst emitter EXACTLY (same geometric, same Exp gaps,
    /// same `>= W` break) and counts surviving follow-ups, so the closed form
    /// in `burst_survival` is checked against the thing it claims to describe
    /// rather than against a restatement of its own algebra.
    fn mc_survival(m: f64, gap: f64, w: f64, iters: u32) -> f64 {
        let mut rng = stream(0xB0_1E, StreamPurpose::Campaign, 7, 0);
        let mut total = 0u64;
        for _ in 0..iters {
            let t_s = rng.gen::<f64>() * w;
            let extra = sample_geometric(&mut rng, m);
            let mut bt = t_s;
            for _ in 0..extra {
                bt += sample_exp(&mut rng, gap);
                if bt >= w {
                    break;
                }
                total += 1;
            }
        }
        total as f64 / iters as f64
    }

    #[test]
    fn burst_survival_matches_monte_carlo() {
        // (mean chain length, gap seconds, window seconds)
        for &(m, gap, w) in &[
            (2.0, 300.0, 86_400.0),   // the shipped regime: gaps << window
            (5.0, 300.0, 86_400.0),
            (0.5, 300.0, 86_400.0),
            (3.0, 20_000.0, 86_400.0), // gaps comparable to the window
            (8.0, 40_000.0, 86_400.0), // truncation dominates
        ] {
            let exact = burst_survival(m, gap, w);
            let mc = mc_survival(m, gap, w, 400_000);
            assert!(
                (exact - mc).abs() < 0.02 * mc.max(0.05),
                "m={m} gap={gap} w={w}: closed form {exact:.4} vs monte carlo {mc:.4}"
            );
        }
    }

    /// The old Jensen form was a strict LOWER bound; the fix must never sit
    /// below it, and must be strictly above it wherever truncation bites.
    #[test]
    fn burst_survival_dominates_the_old_jensen_bound() {
        for &(m, gap, w) in &[(2.0f64, 300.0f64, 86_400.0f64), (3.0, 20_000.0, 86_400.0)] {
            let jensen = (m - (gap / w) * m * (1.0 + m)).max(0.0);
            let exact = burst_survival(m, gap, w);
            assert!(exact >= jensen - 1e-9, "m={m} gap={gap}: {exact} < bound {jensen}");
        }
        // Where gaps are a large fraction of the window the bound is not merely
        // loose, it collapses to the 0 clamp while real bursts still land
        // (Monte Carlo puts the truth near 0.89 follow-ups per burst here).
        let (m, gap, w) = (8.0f64, 40_000.0f64, 86_400.0f64);
        assert_eq!((m - (gap / w) * m * (1.0 + m)).max(0.0), 0.0);
        assert!(burst_survival(m, gap, w) > 0.5);
    }

    /// Degenerate inputs must not produce NaN/Inf on the cap path.
    #[test]
    fn burst_survival_handles_degenerate_inputs() {
        for &(m, gap, w) in &[(0.0, 300.0, 86_400.0), (2.0, 0.0, 86_400.0), (2.0, 300.0, 0.0)] {
            let v = burst_survival(m, gap, w);
            assert!(v.is_finite() && v >= 0.0, "m={m} gap={gap} w={w} -> {v}");
        }
    }
}
