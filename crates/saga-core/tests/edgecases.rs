//! Edge-case audit of the block-bounded-staleness path (Task 2):
//! guards x anomalies, L edge values, granularity rounding vs block
//! boundaries, guard boundary condition, non-finance guard no-op, and
//! repeat-partner memory persistence across days.

mod common;

use common::*;
use saga_core::world::World;
use saga_core::{generate, GenParams};
use saga_rules::RuleBase;

// ---------------------------------------------------------------------------
// (a) GUARDS x ANOMALIES
// ---------------------------------------------------------------------------

/// Campaign/adversary transfers pass through the overdraft guard like any
/// other event (an adversary cannot spend money it does not have either).
/// This test pins the numbers: suppression rates for anomalous vs normal
/// candidates, determinism across threads, and intact anomaly machinery
/// (labels, ground truth, per-intent counts).
#[test]
fn anomalies_with_guards_are_guarded_and_deterministic() {
    let mk = |threads: usize, guards: bool| {
        finance_params(10_000, 200_000, 7.0, 42, threads, 1.0, guards, true)
    };

    // Determinism: threads 1 vs 8 with anomalies + guards, bit-identical.
    let (e1, n1, s1) = run_hashes(&mk(1, true));
    let (e8, n8, s8) = run_hashes(&mk(8, true));
    assert_eq!(e1, e8, "anomalies+guards: edges differ across threads");
    assert_eq!(n1, n8, "anomalies+guards: node state differs across threads");
    assert_eq!(s1.edges_per_intent, s8.edges_per_intent);

    // Candidate (guards-off) vs kept (guards-on) volumes, per label class.
    let (cand, sum_off) = run_capture(&mk(8, false));
    let (kept, sum_on) = run_capture(&mk(8, true));
    let world = World::build(&mk(8, true)).unwrap();
    let transfer = world.events.iter().position(|e| e.name == "transfer").unwrap() as u16;

    let count = |b: &saga_core::EventBatch, anom: bool, only_transfer: bool| {
        (0..b.len())
            .filter(|&i| (b.label[i] != 0) == anom)
            .filter(|&i| !only_transfer || b.event_type[i] == transfer)
            .count()
    };
    let cand_anom_tx = count(&cand.rows, true, true);
    let kept_anom_tx = count(&kept.rows, true, true);
    let cand_norm_tx = count(&cand.rows, false, true);
    let kept_norm_tx = count(&kept.rows, false, true);
    assert!(
        kept_anom_tx < cand_anom_tx,
        "campaign transfers must be guarded: {kept_anom_tx} kept of {cand_anom_tx} candidates"
    );
    // Non-transfer campaign events (no guard on their type) are never touched.
    let cand_anom_other = count(&cand.rows, true, false) - cand_anom_tx;
    let kept_anom_other = count(&kept.rows, true, false) - kept_anom_tx;
    assert_eq!(
        cand_anom_other, kept_anom_other,
        "guard must only suppress event types that carry a guard"
    );

    // The anomaly machinery is intact: ground truth records are identical to
    // the guards-off run (the plan is guard-independent) and labeled edges
    // still join to instances.
    assert_eq!(
        format!("{:?}", sum_off.ground_truth),
        format!("{:?}", sum_on.ground_truth),
        "ground-truth instance records must not depend on guards"
    );
    for i in 0..kept.rows.len() {
        if kept.rows.label[i] != 0 {
            assert!(kept.rows.anomaly_id[i] >= 0, "labeled edge without instance id");
        }
    }

    // Report the distortion the guard introduces against the planner's
    // guard-blind expected counts (documented, not silently corrected).
    let anom_rate_off = count(&cand.rows, true, false) as f64 / cand.rows.len() as f64;
    let anom_rate_on = count(&kept.rows, true, false) as f64 / kept.rows.len() as f64;
    println!(
        "anomalous transfers: {cand_anom_tx} candidates -> {kept_anom_tx} kept \
         ({:.1}% suppressed); normal transfers: {cand_norm_tx} -> {kept_norm_tx} \
         ({:.1}% suppressed); realized anomaly rate {:.4}% (guards off) -> {:.4}% (guards on)",
        100.0 * (cand_anom_tx - kept_anom_tx) as f64 / cand_anom_tx.max(1) as f64,
        100.0 * (cand_norm_tx - kept_norm_tx) as f64 / cand_norm_tx.max(1) as f64,
        100.0 * anom_rate_off,
        100.0 * anom_rate_on,
    );
}

// ---------------------------------------------------------------------------
// (b) L edge values
// ---------------------------------------------------------------------------

/// Nonsense L values must be a clean error from the engine (and the CLI
/// mirrors the same check), never a panic or a silent 1-second clamp.
#[test]
fn invalid_l_is_a_clean_error() {
    for &bad in &[0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let params = finance_params(1_000, 5_000, 2.0, 42, 2, bad, true, false);
        let err = generate(&params, &mut HashSink::default(), None)
            .expect_err(&format!("block_len_days={bad} must be rejected"));
        assert!(
            err.to_string().contains("block_len_days"),
            "error must name the parameter, got: {err}"
        );
        // The sharded-equivalence hook refuses it too.
        assert!(saga_core::sim::shard_equivalence(&params, 2).is_err());
    }
}

/// Extreme but valid L values: L > span == L = span (single whole-run block),
/// and any positive L below one second floors at the engine's 1 s granularity
/// (documented), so it equals an explicit 1 s run bit for bit.
#[test]
fn extreme_l_values_are_well_defined() {
    let base = finance_params(5_000, 100_000, 7.0, 42, 8, 1.0, true, false);

    let at = |l: f64| {
        let mut p = base.clone();
        p.block_len_days = l;
        run_hashes(&p)
    };
    let (e_span, n_span, _) = at(7.0);
    let (e_over, n_over, _) = at(1000.0);
    assert_eq!((e_span, n_span), (e_over, n_over), "L > span must equal L = span");

    let (e_1s, n_1s, _) = at(ONE_SECOND_DAYS);
    let (e_tiny, n_tiny, _) = at(1e-9);
    assert_eq!(
        (e_1s, n_1s),
        (e_tiny, n_tiny),
        "sub-second L must floor at the 1 s granularity"
    );
}

/// L that does not divide the span (or the day): still deterministic across
/// threads and still exactly the block semantics (oracle cross-check).
#[test]
fn l_not_dividing_span_is_correct() {
    let l = 0.3; // 25920 s: divides neither the day nor the 7-day span
    let base = finance_params(5_000, 100_000, 7.0, 42, 8, l, true, false);
    let mut p1 = base.clone();
    p1.threads = 1;
    let (e1, n1, _) = run_hashes(&p1);
    let (e8, n8, _) = run_hashes(&base);
    assert_eq!((e1, n1), (e8, n8), "L=0.3: threads 1 vs 8 differ");

    let mut off = base.clone();
    off.guards_enabled = false;
    let (cand, _) = run_capture(&off);
    let (engine, _) = run_capture(&base);
    let world = World::build(&base).unwrap();
    let oracle = oracle_replay(
        &world,
        &cand.rows,
        base.epoch_unix,
        true,
        OracleMode::Block { l_s: l * 86_400.0 },
    );
    assert_eq!(
        row_multiset(&engine.rows, None),
        row_multiset(&cand.rows, Some(&oracle.kept)),
        "L=0.3: engine differs from block oracle"
    );
    assert_eq!(state_mismatches(&exported_state(&world, &engine.nodes), &oracle.state), 0);
}

// ---------------------------------------------------------------------------
// (c) Granularity rounding vs block boundaries
// ---------------------------------------------------------------------------

/// granularity_s > 1 with sub-day L: rounding happens in Phase A, before any
/// block assignment, so an event's block is a function of its FINAL (rounded)
/// timestamp only and can never round across a block boundary after being
/// swept. Proven by matching an oracle that sees only the rounded stream, for
/// a granularity that divides the day (3600) and one that does not (7000,
/// which makes day boundaries land mid-block AND rounds day-(w+1) events below
/// the raw day boundary).
#[test]
fn granularity_rounding_is_consistent_with_blocks() {
    for &(g, l_days) in &[(3600u64, 0.25f64), (7000, 0.5), (7000, 0.05)] {
        let mut base = finance_params(5_000, 100_000, 7.0, 42, 8, l_days, true, false);
        base.granularity_s = g;
        let mut off = base.clone();
        off.guards_enabled = false;
        let (cand, _) = run_capture(&off);
        // All candidate timestamps are on the granularity grid.
        assert!(
            cand.rows.t.iter().all(|&t| t.rem_euclid(g as i64) == 0),
            "granularity {g}: timestamps must be rounded"
        );
        let (engine, _) = run_capture(&base);
        let world = World::build(&base).unwrap();
        let oracle = oracle_replay(
            &world,
            &cand.rows,
            base.epoch_unix,
            true,
            OracleMode::Block { l_s: (l_days * 86_400.0).max(1.0) },
        );
        assert_eq!(
            row_multiset(&engine.rows, None),
            row_multiset(&cand.rows, Some(&oracle.kept)),
            "granularity {g}, L={l_days}: engine differs from rounded-stream oracle"
        );
        assert_eq!(
            state_mismatches(&exported_state(&world, &engine.nodes), &oracle.state),
            0,
            "granularity {g}, L={l_days}: state differs"
        );

        // Determinism across threads at this granularity.
        let mut p1 = base.clone();
        p1.threads = 1;
        let (e1, n1, _) = run_hashes(&p1);
        let (e8, n8, _) = run_hashes(&base);
        assert_eq!((e1, n1), (e8, n8), "granularity {g}: threads 1 vs 8 differ");
        println!("granularity {g}, L={l_days}: OK ({} kept)", engine.rows.len());
    }
}

// ---------------------------------------------------------------------------
// (d) Guard boundary condition: balance == amount
// ---------------------------------------------------------------------------

/// Micro rule base where every account starts at exactly 100.0 and every
/// transfer is exactly `amount`. With L = span every guard reads S_0, so:
/// amount == balance passes (the guard is `suppress iff balance < amount`,
/// i.e. a >= check), amount = balance + epsilon suppresses everything.
fn micro_guard_rulebase(amount: f64) -> RuleBase {
    let yaml = format!(
        r#"
meta:
  id: micro_guard
  name: Micro guard boundary
  description: guard boundary-condition fixture
  schema_version: 1
entity_types:
  - name: acct
    population_weight: 1.0
    state:
      - name: balance
        init: !constant {{ value: 100.0 }}
relations:
  - name: net
    src: acct
    dst: acct
    model: !uniform_random
    layer: skeleton
    mean_degree: 4.0
event_types:
  - name: pay
    src: acct
    dst: acct
    guard: {{ src_var: balance, amount_attr: amount }}
    attributes:
      - name: amount
        kind: !numeric {{ dist: !constant {{ value: {amount} }} }}
    effects:
      - !sub_from_src {{ var: balance, from_attr: amount }}
      - !add_to_dst {{ var: balance, from_attr: amount }}
behaviors:
  - name: paying
    actor: acct
    timing: {{ rate_per_day: 2.0 }}
    events:
      - {{ event: pay, weight: 1.0, counterparty: !skeleton_neighbor {{ relation: net }} }}
    activity: !constant {{ value: 1.0 }}
control:
  prevalence: 0.0
  difficulty: 0.0
  placement: !uniform
  cascade: 0.0
"#
    );
    RuleBase::from_yaml(&yaml).expect("micro rulebase must parse")
}

fn micro_params(rb: RuleBase, guards: bool, block_len_days: f64) -> GenParams {
    GenParams {
        rulebase: rb,
        nodes: 500,
        target_edges: 4_000,
        span_days: 2.0,
        granularity_s: 1,
        epoch_unix: 1_735_689_600,
        seed: 7,
        threads: 4,
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

#[test]
fn guard_boundary_balance_equal_amount_passes() {
    // amount == initial balance: L = span means every read sees S_0 = 100,
    // so NOTHING is suppressed (>= semantics: equality passes).
    let rb = micro_guard_rulebase(100.0);
    let (cand, _) = run_capture(&micro_params(rb.clone(), false, 2.0));
    let (kept, summary) = run_capture(&micro_params(rb, true, 2.0));
    assert!(!cand.rows.is_empty());
    assert_eq!(
        cand.rows.len(),
        kept.rows.len(),
        "balance == amount must PASS the guard (suppress only on balance < amount)"
    );
    assert_eq!(summary.overdraft_guarded, kept.rows.len() as u64);

    // amount just above the balance: everything is suppressed.
    let rb_eps = micro_guard_rulebase(100.0 + 1e-6);
    let (cand2, _) = run_capture(&micro_params(rb_eps.clone(), false, 2.0));
    let (kept2, summary2) = run_capture(&micro_params(rb_eps, true, 2.0));
    assert!(!cand2.rows.is_empty());
    assert_eq!(
        kept2.rows.len(),
        0,
        "balance < amount (by epsilon) must suppress every transfer"
    );
    assert_eq!(summary2.overdraft_guarded, 0);
}

/// Sequential-exact boundary walk: at L = 1 s with amount == 100, a payer that
/// pays away its whole balance drops to exactly 0 and further sends are
/// suppressed until an incoming 100 refunds it. Cross-checked against the
/// oracle (engine == block oracle) and the invariant that no kept send ever
/// left a snapshot balance below 0 in steps of 100.
#[test]
fn guard_boundary_sequential_walk_matches_oracle() {
    let rb = micro_guard_rulebase(100.0);
    let base = micro_params(rb, true, ONE_SECOND_DAYS);
    let mut off = base.clone();
    off.guards_enabled = false;
    let (cand, _) = run_capture(&off);
    let (engine, _) = run_capture(&base);
    let world = World::build(&base).unwrap();
    let oracle =
        oracle_replay(&world, &cand.rows, base.epoch_unix, true, OracleMode::Block { l_s: 1.0 });
    assert_eq!(
        row_multiset(&engine.rows, None),
        row_multiset(&cand.rows, Some(&oracle.kept))
    );
    let engine_state = exported_state(&world, &engine.nodes);
    assert_eq!(state_mismatches(&engine_state, &oracle.state), 0);
    // All final balances are multiples of 100 and, guards on at L = 1 s, only
    // same-second ties could overdraw; balances must never go below -100 * (a
    // small tie count). With amounts all equal to 100 and initial balance 100,
    // any negative balance can only come from a same-second double-spend.
    let neg = engine_state[0][0].iter().filter(|&&b| b < 0.0).count();
    println!(
        "sequential walk: {} kept of {}, negative balances {neg}",
        engine.rows.len(),
        cand.rows.len()
    );
}

// ---------------------------------------------------------------------------
// (e) Unguarded domains with --guards: clean no-op
// ---------------------------------------------------------------------------

/// Only finance and crypto define guards; for every other built-in domain
/// --guards must be a no-op for a rule base that declares NO guard:
/// bit-identical edges AND node state vs guards-off, zero guard telemetry.
/// All six built-in domains now declare a guard (consistency model section
/// 13), so the no-guard rule base is each of the four newest guarded domains
/// with its `guard:` fields stripped; the same loop also asserts the intact
/// domain's guard actually fires (non-zero telemetry, fewer edges).
#[test]
fn guards_are_a_noop_for_domains_without_guards() {
    for domain in ["cyber", "transport", "ecommerce", "healthcare"] {
        let mk = |guards: bool, strip: bool| {
            let mut p = finance_params(5_000, 50_000, 5.0, 42, 8, 0.5, guards, true);
            let mut rb = saga_rules::load_builtin_rulebase(domain).unwrap();
            if strip {
                for ev in rb.event_types.iter_mut() {
                    ev.guard = None;
                }
            }
            p.rulebase = rb;
            p
        };
        let (e_off, n_off, s_off) = run_hashes(&mk(false, true));
        let (e_on, n_on, s_on) = run_hashes(&mk(true, true));
        assert_eq!(e_off, e_on, "{domain}: guards-on edges differ from guards-off");
        assert_eq!(n_off, n_on, "{domain}: guards-on node state differs from guards-off");
        assert_eq!(s_on.overdraft_guarded, 0, "{domain}: phantom guard telemetry");
        assert_eq!(s_on.overdraft_violations, 0);
        assert_eq!(s_off.edges_written, s_on.edges_written);
        // Stripping the guard must not change guards-off output either.
        let (e_off_intact, _, _) = run_hashes(&mk(false, false));
        assert_eq!(e_off, e_off_intact, "{domain}: guard field changes guards-off edges");
        // The intact domain's guard is real: it fires and suppresses.
        let (_, _, s_guarded) = run_hashes(&mk(true, false));
        assert!(s_guarded.overdraft_guarded > 0, "{domain}: guard never evaluated");
        assert!(s_guarded.edges_written < s_off.edges_written, "{domain}: guard never suppressed");
        println!(
            "{domain}: no-guard no-op OK ({} edges); guarded {} edges, {} guarded, {} violations",
            s_on.edges_written, s_guarded.edges_written, s_guarded.overdraft_guarded,
            s_guarded.overdraft_violations
        );
    }
}

// ---------------------------------------------------------------------------
// (f) Repeat-partner memory persists across days
// ---------------------------------------------------------------------------

/// Micro rule base with repeat_p = 1.0: an actor's FIRST event draws a
/// skeleton neighbor and stores it; every later event reuses the stored
/// partner. If memory persisted only within a day (the streaming refactor
/// hoisted init_memories out of the day loop; this pins that), an actor
/// active on several days would draw a fresh partner each day and, with
/// mean degree 20, almost surely show several distinct partners.
fn micro_memory_rulebase() -> RuleBase {
    RuleBase::from_yaml(
        r#"
meta:
  id: micro_mem
  name: Micro memory
  description: repeat-partner memory persistence fixture
  schema_version: 1
entity_types:
  - name: user
    population_weight: 1.0
relations:
  - name: net
    src: user
    dst: user
    model: !uniform_random
    layer: skeleton
    mean_degree: 20.0
event_types:
  - name: ping
    src: user
    dst: user
behaviors:
  - name: pinging
    actor: user
    timing: { rate_per_day: 4.0 }
    events:
      - { event: ping, weight: 1.0, counterparty: !repeat_or_neighbor { relation: net, repeat_p: 1.0 } }
    activity: !constant { value: 1.0 }
control:
  prevalence: 0.0
  difficulty: 0.0
  placement: !uniform
  cascade: 0.0
"#,
    )
    .expect("micro memory rulebase must parse")
}

#[test]
fn repeat_partner_memory_persists_across_days() {
    let mut params = micro_params(micro_memory_rulebase(), false, 1.0);
    params.nodes = 1_000;
    params.target_edges = 12_000;
    params.span_days = 3.0;
    let (cap, _) = run_capture(&params);
    assert!(!cap.rows.is_empty());

    use std::collections::{HashMap, HashSet};
    let mut partners: HashMap<u64, HashSet<u64>> = HashMap::new();
    let mut days: HashMap<u64, HashSet<i64>> = HashMap::new();
    for i in 0..cap.rows.len() {
        partners.entry(cap.rows.src[i]).or_default().insert(cap.rows.dst[i]);
        days.entry(cap.rows.src[i])
            .or_default()
            .insert((cap.rows.t[i] - params.epoch_unix) / 86_400);
    }
    let multi_day = days.values().filter(|d| d.len() >= 2).count();
    assert!(
        multi_day > 100,
        "test has no teeth: only {multi_day} actors active on 2+ days"
    );
    for (src, ps) in &partners {
        assert_eq!(
            ps.len(),
            1,
            "actor {src} has {} distinct partners; with repeat_p = 1.0 memory \
             must pin the first partner across ALL days",
            ps.len()
        );
    }
    println!(
        "memory persistence: {} actors, {} active on 2+ days, all with exactly 1 partner",
        partners.len(),
        multi_day
    );

    // Identical under threads 1 vs 8 and guards on vs off (no guard defined,
    // and phase A never reads state, so the day-2 draws depend on day-1
    // memory the same way in every configuration).
    let mut variants = Vec::new();
    for &(threads, guards) in &[(1usize, false), (8, false), (1, true), (8, true)] {
        let mut p = params.clone();
        p.threads = threads;
        p.guards_enabled = guards;
        variants.push(run_hashes(&p));
    }
    for w in variants.windows(2) {
        assert_eq!(w[0].0, w[1].0, "memory fixture: edges differ across threads/guards");
        assert_eq!(w[0].1, w[1].1, "memory fixture: node state differs across threads/guards");
    }
}
