//! Section 17: guard-aware anomaly calibration.
//!
//! With guards on, guarded event types are suppressed differentially
//! (anomalous structuring-sized transfers exceed a typical balance more often
//! than normal transfers), so the realized anomalous-edge fraction drifts
//! below the planned (guards-off) one. `calibrate_passes` measures that
//! differential and rescales per-event-type campaign rates so the realized
//! fraction lands back on the guards-off fraction. These tests pin: the gap
//! closes within tolerance, the correction is deterministic across thread
//! counts and reruns, the ground truth stays consistent with the labels, and
//! the opt-in is refused where it cannot be sound.

mod common;

use std::collections::HashSet;

use common::*;
use saga_core::generate;

fn labeled_fraction(rows: &saga_core::EventBatch) -> f64 {
    rows.label.iter().filter(|&&l| l != 0).count() as f64 / rows.len().max(1) as f64
}

#[test]
fn calibration_lands_realized_rate_within_tolerance() {
    let off = finance_params(10_000, 200_000, 7.0, 42, 8, 1.0, false, true);
    let (off_rows, _) = run_capture(&off);
    let target = labeled_fraction(&off_rows.rows);
    assert!(target > 0.0);

    for &l_days in &[0.001, 1.0] {
        let on = finance_params(10_000, 200_000, 7.0, 42, 8, l_days, true, true);
        let (on_rows, on_summary) = run_capture(&on);
        let uncorrected = labeled_fraction(&on_rows.rows);
        assert_eq!(on_summary.calibration_passes, 0);
        assert!(on_summary.planned_anomalous_fraction.is_finite());

        let mut cal = on.clone();
        cal.calibrate_passes = 1;
        let (cal_rows, cal_summary) = run_capture(&cal);
        let corrected = labeled_fraction(&cal_rows.rows);

        println!(
            "L={l_days}: target {:.4}% uncorrected {:.4}% corrected {:.4}% multipliers {:?}",
            100.0 * target,
            100.0 * uncorrected,
            100.0 * corrected,
            cal_summary.calibration_multipliers
        );
        assert_eq!(cal_summary.calibration_passes, 1);
        // The calibration's own telemetry agrees with what we measured.
        assert!((cal_summary.calibration_target_fraction - target).abs() < 1e-12);
        assert!((cal_summary.calibration_uncorrected_fraction - uncorrected).abs() < 1e-12);
        // Within 5% relative of the guards-off fraction (Poisson noise on
        // ~1.8k labeled edges is ~2.4%), and never worse than uncorrected.
        let gap = (corrected - target).abs() / target;
        assert!(gap < 0.05, "L={l_days}: corrected rate off by {:.2}% relative", 100.0 * gap);
        assert!(
            (corrected - target).abs() <= (uncorrected - target).abs(),
            "L={l_days}: calibration moved the realized rate away from the target"
        );
        // Every labeled kept edge joins a ground-truth instance; every normal
        // edge carries no instance id.
        let ids: HashSet<i64> = cal_summary.ground_truth.iter().map(|r| r.id).collect();
        for i in 0..cal_rows.rows.len() {
            if cal_rows.rows.label[i] != 0 {
                assert!(ids.contains(&cal_rows.rows.anomaly_id[i]), "labeled edge without instance");
            } else {
                assert_eq!(cal_rows.rows.anomaly_id[i], -1);
            }
        }
    }
}

#[test]
fn calibration_is_deterministic_across_threads_and_reruns() {
    let mut p1 = finance_params(10_000, 200_000, 7.0, 42, 1, 1.0, true, true);
    p1.calibrate_passes = 2;
    let mut p8 = p1.clone();
    p8.threads = 8;
    let (a, sa) = run_capture(&p1);
    let (b, sb) = run_capture(&p8);
    let (c, sc) = run_capture(&p8);
    assert!(sa.calibration_multipliers.iter().any(|(_, g)| *g != 1.0));
    assert_eq!(sa.calibration_multipliers, sb.calibration_multipliers);
    assert_eq!(sb.calibration_multipliers, sc.calibration_multipliers);
    assert_eq!(row_multiset(&a.rows, None), row_multiset(&b.rows, None));
    assert_eq!(row_multiset(&b.rows, None), row_multiset(&c.rows, None));
    assert_eq!(a.rows.t, b.rows.t, "stream order differs across thread counts");
    assert_eq!(b.rows.t, c.rows.t, "stream order differs across reruns");
    assert_eq!(sa.edges_per_intent, sb.edges_per_intent);
    assert_eq!(sb.edges_per_intent, sc.edges_per_intent);
    assert_eq!(a.nodes.len(), b.nodes.len());
    for (na, nb) in a.nodes.iter().zip(&b.nodes) {
        assert_eq!(na.ids, nb.ids);
        assert_eq!(format!("{:?}", na.attrs), format!("{:?}", nb.attrs));
    }
}

#[test]
fn calibration_is_refused_where_it_cannot_be_sound() {
    let mut p = finance_params(2_000, 20_000, 2.0, 1, 2, 1.0, false, true);
    p.calibrate_passes = 1;
    let err = generate(&p, &mut saga_core::CountingSink::default(), None).unwrap_err();
    assert!(err.to_string().contains("guards_enabled"), "{err}");

    let mut p = finance_params(2_000, 20_000, 2.0, 1, 2, 1.0, true, true);
    p.calibrate_passes = 1;
    p.shard_count = 2;
    p.exchange_dir = Some(std::env::temp_dir());
    let err = generate(&p, &mut saga_core::CountingSink::default(), None).unwrap_err();
    assert!(err.to_string().contains("whole-graph"), "{err}");
}
