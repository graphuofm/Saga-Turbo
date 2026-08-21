//! ORACLE cross-validation of Phase B (the block-bounded-staleness sweep).
//!
//! An independent sequential reference simulator (tests/common/mod.rs,
//! `oracle_replay`) replays the engine's own candidate stream one event at a
//! time in stream order, maintains per-node state directly, applies the
//! overdraft guard per event, and applies effects immediately. It shares NO
//! code with the engine's Phase-B sweep (BlockSweep / apply_effects /
//! apply_event are private to sim.rs, so reuse is impossible by construction);
//! only Phase A (candidate generation, obtained through the public API with
//! guards OFF) and the compiled World (initial state S_0, effect/guard index
//! tables) are shared, because candidates and S_0 are sampled from seeded RNG
//! streams an oracle cannot independently reproduce.
//!
//! Same-second-tie semantics (the ONLY documented divergence from
//! sequential-exact): the engine's smallest block is 1 second, so events in
//! the same second share a snapshot. The oracle handles this explicitly both
//! ways: `OracleMode::Block { l_s: 1.0 }` snapshots per second and must match
//! the engine EXACTLY; `OracleMode::PerEvent` is the true L -> 0 fold and its
//! divergence from the block oracle is asserted to be exactly same-second
//! ties (first divergence provably caused by an earlier same-second event on
//! the same payer's balance) and is counted.

mod common;

use common::*;
use saga_core::world::World;

/// Candidate stream (guards OFF) for the given base params: by the section-8
/// decoupling, guards and L touch nothing in Phase A, so this is exactly the
/// stream Phase B sweeps, in the order it sweeps it.
fn candidates(base: &saga_core::GenParams) -> (common::CaptureSink, saga_core::GenSummary) {
    let mut p = base.clone();
    p.guards_enabled = false;
    run_capture(&p)
}

/// Guards OFF, pure effect folding, any L: the engine's exported final node
/// state must equal the oracle's per-event fold of ALL candidates, bitwise.
#[test]
fn guards_off_final_state_matches_pure_fold() {
    for &seed in &[7u64, 42] {
        let params = finance_params(5_000, 100_000, 7.0, seed, 8, 1.0, false, false);
        let (cap, summary) = run_capture(&params);
        let world = World::build(&params).unwrap();

        let oracle = oracle_replay(&world, &cap.rows, params.epoch_unix, false, OracleMode::PerEvent);
        assert!(oracle.kept.iter().all(|&k| k), "guards off must keep every candidate");
        assert_eq!(summary.overdraft_guarded, 0);
        assert_eq!(summary.overdraft_violations, 0);

        let engine_state = exported_state(&world, &cap.nodes);
        let bad = state_mismatches(&engine_state, &oracle.state);
        assert_eq!(bad, 0, "seed {seed}: {bad} state cells differ from the pure fold");

        // Any L: guards off is L-independent, so a second L gives the same state.
        let mut p2 = params.clone();
        p2.block_len_days = params.span_days; // single whole-run block
        let (cap2, _) = run_capture(&p2);
        assert_eq!(
            state_mismatches(&exported_state(&world, &cap2.nodes), &oracle.state),
            0,
            "seed {seed}: guards-off state depends on L"
        );
        println!("seed {seed}: guards-off fold OK over {} events", cap.rows.len());
    }
}

/// Guards ON at L = granularity (1 s): kept-edge multiset AND final per-node
/// state must match the per-second-snapshot oracle exactly. Events in the same
/// second share a snapshot in BOTH (the documented spec semantics), so the
/// match is exact with no tie exemption needed.
#[test]
fn guards_on_matches_block_oracle_at_one_second() {
    for &seed in &[7u64, 42, 1234] {
        let base = finance_params(5_000, 100_000, 7.0, seed, 8, ONE_SECOND_DAYS, true, false);
        let (cand, _) = candidates(&base);
        let (engine, summary) = run_capture(&base);
        let world = World::build(&base).unwrap();

        let oracle =
            oracle_replay(&world, &cand.rows, base.epoch_unix, true, OracleMode::Block { l_s: 1.0 });

        // Kept-edge multiset: exact.
        let engine_rows = row_multiset(&engine.rows, None);
        let oracle_rows = row_multiset(&cand.rows, Some(&oracle.kept));
        assert_eq!(
            engine_rows.len(),
            oracle_rows.len(),
            "seed {seed}: engine kept {} rows, oracle {}",
            engine_rows.len(),
            oracle_rows.len()
        );
        assert_eq!(engine_rows, oracle_rows, "seed {seed}: kept-edge multiset differs");

        // Final per-node state: exact, bitwise.
        let engine_state = exported_state(&world, &engine.nodes);
        assert_eq!(
            state_mismatches(&engine_state, &oracle.state),
            0,
            "seed {seed}: final node state differs from the oracle"
        );

        // Telemetry agrees too.
        assert_eq!(summary.overdraft_guarded, oracle.guarded_kept, "seed {seed}: guarded count");

        // Sub effects DO drive balances negative (unguarded purchases and
        // withdrawals, plus same-second stale reads); engine and oracle must
        // agree cell for cell, which the bitwise check above already proves.
        let acct = world.entities.index_of("account");
        let bal = world.state_names[acct].iter().position(|n| n == "balance").unwrap();
        let negatives = engine_state[acct][bal].iter().filter(|&&v| v < 0.0).count();
        println!(
            "seed {seed}: L=1s kept {}/{} candidates (suppressed {}), guarded {}, \
             violations {}, negative balances {}",
            engine_rows.len(),
            cand.rows.len(),
            oracle.suppressed,
            summary.overdraft_guarded,
            summary.overdraft_violations,
            negatives
        );
        assert!(negatives > 0, "expected some negative balances from unguarded sub effects");
    }
}

/// The same oracle generalizes to ANY L: cross-validate the engine at sub-day,
/// exact-span, and beyond-span block lengths (L > span must equal L = span,
/// both a single whole-run block).
#[test]
fn guards_on_matches_block_oracle_at_coarser_l() {
    let seed = 42u64;
    let span = 7.0;
    let base = finance_params(5_000, 100_000, span, seed, 8, 1.0, true, false);
    let (cand, _) = candidates(&base);
    let world = World::build(&base).unwrap();

    let mut span_rows = None;
    for &l_days in &[0.25, 1.0, span, 4.0 * span] {
        let mut p = base.clone();
        p.block_len_days = l_days;
        let (engine, _) = run_capture(&p);
        let oracle = oracle_replay(
            &world,
            &cand.rows,
            base.epoch_unix,
            true,
            OracleMode::Block { l_s: (l_days * 86_400.0).max(1.0) },
        );
        let engine_rows = row_multiset(&engine.rows, None);
        let oracle_rows = row_multiset(&cand.rows, Some(&oracle.kept));
        assert_eq!(engine_rows, oracle_rows, "L={l_days}: kept-edge multiset differs");
        assert_eq!(
            state_mismatches(&exported_state(&world, &engine.nodes), &oracle.state),
            0,
            "L={l_days}: final state differs"
        );
        println!("L={l_days:<5} kept {}/{}", engine_rows.len(), cand.rows.len());
        if l_days == span {
            span_rows = Some(engine_rows);
        } else if l_days > span {
            assert_eq!(
                span_rows.as_ref().unwrap(),
                &engine_rows,
                "L > span must be identical to L = span (single whole-run block)"
            );
        }
    }
}

/// Sequential-exact (per-event) oracle vs the per-second (block) oracle over
/// the SAME candidates: their kept sets may diverge ONLY through same-second
/// ties. Verified explicitly: the first diverging decision must have an
/// earlier same-second event that touched the payer's guarded balance; every
/// divergence is classified as such a direct tie or as a downstream cascade of
/// earlier divergences, and both are counted.
///
/// Granularity 60 s (with L = granularity = 60 s) is used so that ties are
/// frequent enough to exercise the accounting; the 1 s case is exercised too.
#[test]
fn per_event_divergence_is_exactly_same_second_ties() {
    for &(granularity_s, label) in &[(60u64, "L=granularity=60s"), (1u64, "L=granularity=1s")] {
        let mut base = finance_params(5_000, 150_000, 7.0, 42, 8, 1.0, true, false);
        base.granularity_s = granularity_s;
        base.block_len_days = granularity_s as f64 / 86_400.0;
        let (cand, _) = candidates(&base);
        let world = World::build(&base).unwrap();
        let l_s = granularity_s as f64;

        let blk = oracle_replay(&world, &cand.rows, base.epoch_unix, true, OracleMode::Block { l_s });
        let seq = oracle_replay(&world, &cand.rows, base.epoch_unix, true, OracleMode::PerEvent);

        // The engine at L = granularity equals the block oracle (re-checked
        // here at this granularity so the tie accounting is anchored).
        let (engine, _) = run_capture(&base);
        assert_eq!(
            row_multiset(&engine.rows, None),
            row_multiset(&cand.rows, Some(&blk.kept)),
            "{label}: engine != block oracle"
        );

        // Does candidate j touch the guarded balance of `node`?
        let touches = |j: usize, entity: usize, var: usize, node: u64| -> bool {
            let ev = &world.events[cand.rows.event_type[j] as usize];
            ev.effects.iter().any(|e| {
                e.entity == entity
                    && e.var == var
                    && (if e.on_src { cand.rows.src[j] } else { cand.rows.dst[j] }) == node
            })
        };
        let block_of = |t: i64| ((t - base.epoch_unix) as f64 / l_s).floor() as i64;

        let mut divergent = 0u64;
        let mut direct_ties = 0u64;
        let mut cascade = 0u64;
        let mut first_checked = false;
        for i in 0..cand.rows.len() {
            if blk.kept[i] == seq.kept[i] {
                continue;
            }
            divergent += 1;
            let g = world.events[cand.rows.event_type[i] as usize]
                .guard
                .as_ref()
                .expect("only guarded events can diverge");
            // Direct same-second (same-block) tie: an earlier event in the
            // same block, kept by at least one trajectory, that moved this
            // payer's guarded balance.
            let mut j = i;
            let mut direct = false;
            while j > 0 && block_of(cand.rows.t[j - 1]) == block_of(cand.rows.t[i]) {
                j -= 1;
                if (blk.kept[j] || seq.kept[j]) && touches(j, g.entity, g.var, cand.rows.src[i]) {
                    direct = true;
                    break;
                }
            }
            if direct {
                direct_ties += 1;
            } else {
                cascade += 1;
            }
            if !first_checked {
                assert!(
                    direct,
                    "{label}: the FIRST divergence (row {i}) has no same-second cause; \
                     that would be a real Phase-B bug, not tie semantics"
                );
                first_checked = true;
            }
        }
        let kept_blk = blk.kept.iter().filter(|&&k| k).count();
        let kept_seq = seq.kept.iter().filter(|&&k| k).count();
        println!(
            "{label}: candidates {}, kept block {} vs per-event {}, divergent decisions {} \
             (direct same-second ties {}, downstream cascade {})",
            cand.rows.len(),
            kept_blk,
            kept_seq,
            divergent,
            direct_ties,
            cascade
        );
        assert!(
            divergent == 0 || direct_ties > 0,
            "{label}: divergence without a single direct same-second tie"
        );
    }
}

/// SECOND GUARDED DOMAIN (crypto): the balance guard on `tx` (value) and
/// `swap` (amount_in) — a wallet cannot spend native value it does not hold —
/// goes through the SAME domain-generic oracle. Engine == oracle bitwise
/// (kept-edge multiset AND final node state) at L = 1 s, 1 day, and span,
/// proving the guard mechanism is generic, not finance-specific. Also pins
/// threads 1 vs 8 determinism for the guarded crypto run and checks that
/// BOTH guarded event types are actually exercised (some suppressed each).
#[test]
fn crypto_guards_match_block_oracle() {
    let span = 7.0;
    let base = domain_params("crypto", 5_000, 100_000, span, 42, 8, 1.0, true, false);
    let (cand, _) = candidates(&base);
    let world = World::build(&base).unwrap();

    for &l_days in &[ONE_SECOND_DAYS, 1.0, span] {
        let mut p = base.clone();
        p.block_len_days = l_days;
        let (engine, summary) = run_capture(&p);
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
            "crypto L={l_days}: kept-edge multiset differs from oracle"
        );
        assert_eq!(
            state_mismatches(&exported_state(&world, &engine.nodes), &oracle.state),
            0,
            "crypto L={l_days}: final node state differs from oracle"
        );
        assert_eq!(summary.overdraft_guarded, oracle.guarded_kept, "crypto L={l_days}: guarded count");

        // Both guarded event types fire and both see suppressions.
        let tx_ty = world.events.iter().position(|e| e.name == "tx").unwrap() as u16;
        let swap_ty = world.events.iter().position(|e| e.name == "swap").unwrap() as u16;
        let suppressed_of = |ty: u16| {
            (0..cand.rows.len())
                .filter(|&i| cand.rows.event_type[i] == ty && !oracle.kept[i])
                .count()
        };
        assert!(suppressed_of(tx_ty) > 0, "crypto L={l_days}: tx guard never fired");
        assert!(suppressed_of(swap_ty) > 0, "crypto L={l_days}: swap guard never fired");

        // Determinism: threads 1 vs 8 bitwise identical (edges and state).
        let mut p1 = p.clone();
        p1.threads = 1;
        let (e1, s1) = run_capture(&p1);
        assert_eq!(
            row_multiset(&e1.rows, None),
            row_multiset(&engine.rows, None),
            "crypto L={l_days}: edges differ across threads"
        );
        assert_eq!(
            state_mismatches(
                &exported_state(&world, &e1.nodes),
                &exported_state(&world, &engine.nodes)
            ),
            0,
            "crypto L={l_days}: node state differs across threads"
        );
        assert_eq!(s1.overdraft_guarded, summary.overdraft_guarded);
        assert_eq!(s1.overdraft_violations, summary.overdraft_violations);
        println!(
            "crypto L={l_days:<9} kept {}/{} (tx suppressed {}, swap suppressed {}), \
             guarded {}, violations {}",
            engine.rows.len(),
            cand.rows.len(),
            suppressed_of(tx_ty),
            suppressed_of(swap_ty),
            summary.overdraft_guarded,
            summary.overdraft_violations
        );
    }
}

/// ALL SIX DOMAINS GUARDED (consistency model section 13): cyber (host
/// egress allowance on `flow`), ecommerce (buyer available funds on
/// `PURCHASED`), healthcare (pharmacy stock on `DISPENSED`) and transport
/// (vehicle remaining range on `trip`) go through the SAME domain-generic
/// oracle with zero oracle changes. For each domain and L in {1 s, 1 day,
/// span}: engine == oracle bitwise (kept-edge multiset AND final node
/// state), the guarded-count telemetry matches, every declared guard fires
/// (some candidates suppressed), and threads 1 vs 8 are bit-identical.
#[test]
fn remaining_domains_guards_match_block_oracle() {
    let span = 7.0;
    for domain in ["cyber", "ecommerce", "healthcare", "transport"] {
        let base = domain_params(domain, 5_000, 100_000, span, 42, 8, 1.0, true, false);
        let (cand, _) = candidates(&base);
        let world = World::build(&base).unwrap();
        let guarded_types: Vec<(u16, &str)> = world
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.guard.is_some())
            .map(|(i, e)| (i as u16, e.name.as_str()))
            .collect();
        assert!(!guarded_types.is_empty(), "{domain}: no guarded event type");

        for &l_days in &[ONE_SECOND_DAYS, 1.0, span] {
            let mut p = base.clone();
            p.block_len_days = l_days;
            let (engine, summary) = run_capture(&p);
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
                "{domain} L={l_days}: kept-edge multiset differs from oracle"
            );
            assert_eq!(
                state_mismatches(&exported_state(&world, &engine.nodes), &oracle.state),
                0,
                "{domain} L={l_days}: final node state differs from oracle"
            );
            assert_eq!(
                summary.overdraft_guarded, oracle.guarded_kept,
                "{domain} L={l_days}: guarded count"
            );
            let mut suppressed = Vec::new();
            for &(ty, name) in &guarded_types {
                let n = (0..cand.rows.len())
                    .filter(|&i| cand.rows.event_type[i] == ty && !oracle.kept[i])
                    .count();
                assert!(n > 0, "{domain} L={l_days}: `{name}` guard never fired");
                suppressed.push((name, n));
            }

            // Determinism: threads 1 vs 8 bitwise identical (edges and state).
            let mut p1 = p.clone();
            p1.threads = 1;
            let (e1, s1) = run_capture(&p1);
            assert_eq!(
                row_multiset(&e1.rows, None),
                row_multiset(&engine.rows, None),
                "{domain} L={l_days}: edges differ across threads"
            );
            assert_eq!(
                state_mismatches(
                    &exported_state(&world, &e1.nodes),
                    &exported_state(&world, &engine.nodes)
                ),
                0,
                "{domain} L={l_days}: node state differs across threads"
            );
            assert_eq!(s1.overdraft_guarded, summary.overdraft_guarded);
            assert_eq!(s1.overdraft_violations, summary.overdraft_violations);
            println!(
                "{domain:<10} L={l_days:<9} kept {}/{} suppressed {:?}, guarded {}, violations {}",
                engine.rows.len(),
                cand.rows.len(),
                suppressed,
                summary.overdraft_guarded,
                summary.overdraft_violations
            );
        }
    }
}

/// Guards x anomalies (edge-case audit 2a, oracle side): with anomalies ON and
/// guards ON, campaign/adversary events flow through the SAME Phase-B guard as
/// normal events. The oracle applies the guard uniformly to every candidate,
/// so engine == oracle here proves the engine does too, at both L = 1 s and
/// L = 1 day.
#[test]
fn anomalies_on_guards_on_matches_oracle() {
    for &l_days in &[ONE_SECOND_DAYS, 1.0] {
        let base = finance_params(10_000, 200_000, 7.0, 42, 8, l_days, true, true);
        let (cand, _) = candidates(&base);
        let (engine, summary) = run_capture(&base);
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
            "L={l_days}: anomalies+guards kept-edge multiset differs from oracle"
        );
        assert_eq!(
            state_mismatches(&exported_state(&world, &engine.nodes), &oracle.state),
            0,
            "L={l_days}: anomalies+guards final state differs from oracle"
        );

        // Campaign events are demonstrably subject to the guard: some
        // anomalous candidates are suppressed.
        let cand_anom = cand.rows.label.iter().filter(|&&l| l != 0).count();
        let kept_anom = engine.rows.label.iter().filter(|&&l| l != 0).count();
        println!(
            "L={l_days}: anomalous candidates {cand_anom} -> kept {kept_anom} \
             (suppressed {}), total kept {}/{}, guarded {}, violations {}",
            cand_anom - kept_anom,
            engine.rows.len(),
            cand.rows.len(),
            summary.overdraft_guarded,
            summary.overdraft_violations
        );
        assert!(kept_anom > 0, "anomaly machinery must still emit labeled edges");
        assert!(
            kept_anom < cand_anom,
            "expected the guard to suppress some campaign transfers"
        );
    }
}
