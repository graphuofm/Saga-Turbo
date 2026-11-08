//! Node dynamics (lifecycles) and the node-state trajectory.
//!
//! Pins the four properties the paper states:
//!   1. With node dynamics, no edge touches a node outside its lifetime, in
//!      every domain, on the normal path and on the campaign path.
//!   2. Births follow the id (construction) order within a type, and the
//!      first-built nodes exist from t = 0.
//!   3. Lifecycles and the trajectory are schedule-independent: edges and
//!      the node_states table are identical for 1 and 4 threads.
//!   4. The trajectory is consistent with the merged state: the last value
//!      of every (node, var) equals the exported final state, and every row
//!      is stamped with a block end.
//! Plus: with both switches off the world has no lifecycle at all.

mod common;

use common::*;
use saga_core::world::World;
use saga_core::{generate, AuxTable, EventBatch, EventSink, NodeBatch, TableColumn};

const DOMAINS: [&str; 6] = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"];

#[derive(Default)]
struct Full {
    cap: CaptureSink,
    states: Vec<AuxTable>,
}

impl EventSink for Full {
    fn write_batch(&mut self, b: &EventBatch) -> anyhow::Result<()> {
        self.cap.write_batch(b)
    }
    fn write_nodes(&mut self, b: &NodeBatch) -> anyhow::Result<()> {
        self.cap.write_nodes(b)
    }
    fn write_table(&mut self, t: &AuxTable) -> anyhow::Result<()> {
        assert_eq!(t.name, "node_states");
        self.states.push(t.clone());
        Ok(())
    }
}

/// (node, t, var code, value bits) rows of all captured chunks, in order.
fn state_rows(tables: &[AuxTable]) -> Vec<(u64, i64, u16, u64)> {
    let mut out = Vec::new();
    for t in tables {
        let (TableColumn::U64(n), TableColumn::I64(ts), TableColumn::Category { codes, .. }, TableColumn::F64(v)) =
            (&t.columns[0].1, &t.columns[1].1, &t.columns[2].1, &t.columns[3].1)
        else {
            panic!("unexpected node_states schema")
        };
        for i in 0..n.len() {
            out.push((n[i], ts[i], codes[i], v[i].to_bits()));
        }
    }
    out
}

fn dyn_params(domain: &str, threads: usize, guards: bool) -> saga_core::GenParams {
    let mut p = domain_params(domain, 8_000, 60_000, 30.0, 7, threads, 1.0, guards, true);
    p.node_dynamics = true;
    p.anomaly_rate = Some(0.05);
    p
}

#[test]
fn off_by_default_no_lifecycle() {
    let p = domain_params("finance", 2_000, 10_000, 10.0, 1, 2, 1.0, false, true);
    let w = World::build(&p).unwrap();
    assert!(w.birth.is_empty() && w.death.is_empty());
}

#[test]
fn no_edge_outside_lifetime_all_domains() {
    for d in DOMAINS {
        for guards in [false, true] {
            let p = dyn_params(d, 4, guards);
            let w = World::build(&p).unwrap();
            assert!(!w.birth.is_empty(), "{d}: lifecycle expected");
            let (cap, s) = run_capture(&p);
            let r = &cap.rows;
            assert!(r.len() > 1_000, "{d}: too few edges ({})", r.len());
            let mut bad = 0usize;
            let mut bad_campaign = 0usize;
            for i in 0..r.len() {
                let ok = w.alive(r.src[i], r.t[i]) && w.alive(r.dst[i], r.t[i]);
                if !ok {
                    bad += 1;
                    if r.label[i] != 0 {
                        bad_campaign += 1;
                    }
                }
            }
            assert_eq!(bad, 0, "{d} guards={guards}: {bad} edges outside a lifetime ({bad_campaign} labeled)");
            // The budget compensation keeps the delivered count near the
            // fixed-population run (guards off: near the target).
            if !guards {
                let frac = s.edges_written as f64 / p.target_edges as f64;
                assert!((0.8..1.2).contains(&frac), "{d}: edges {} vs target {}", s.edges_written, p.target_edges);
            }
        }
    }
}

#[test]
fn births_follow_construction_order() {
    for d in DOMAINS {
        let p = dyn_params(d, 2, false);
        let w = World::build(&p).unwrap();
        for ti in 0..w.entities.names.len() {
            let s = w.entities.starts[ti] as usize;
            let n = w.entities.counts[ti] as usize;
            let b = &w.birth[s..s + n];
            assert!(b.windows(2).all(|x| x[0] <= x[1]), "{d}: births not monotone in id");
            assert_eq!(b[0], p.epoch_unix, "{d}: first-built node must exist at t = 0");
            assert!((0..n).all(|i| w.death[s + i] > w.birth[s + i]), "{d}: death before birth");
        }
    }
}

#[test]
fn dynamics_and_trajectory_schedule_independent() {
    for d in ["finance", "cyber"] {
        let mut runs = Vec::new();
        for threads in [1, 4] {
            let mut p = dyn_params(d, threads, true);
            p.state_trajectory = true;
            let mut sink = Full::default();
            generate(&p, &mut sink, None).unwrap();
            runs.push((edge_hash(&sink.cap.rows), state_rows(&sink.states)));
        }
        assert_eq!(runs[0].0, runs[1].0, "{d}: edges differ across thread counts");
        assert!(runs[0].1 == runs[1].1, "{d}: node_states differ across thread counts");
    }
}

#[test]
fn trajectory_matches_final_state() {
    for d in DOMAINS {
        let mut p = dyn_params(d, 4, true);
        p.state_trajectory = true;
        let w = World::build(&p).unwrap();
        let mut sink = Full::default();
        generate(&p, &mut sink, None).unwrap();
        let fin = exported_state(&w, &sink.cap.nodes);
        let names = match &sink.states[0].columns[2].1 {
            TableColumn::Category { names, .. } => names.clone(),
            _ => unreachable!(),
        };
        let rows = state_rows(&sink.states);
        let l_s = 86_400i64;
        let end = p.epoch_unix + (p.span_days * 86_400.0) as i64;
        // Last value per (node, var); rows are in time order.
        let mut last = std::collections::HashMap::new();
        let mut prev_t = i64::MIN;
        for &(node, t, var, bits) in &rows {
            assert!(t >= prev_t, "{d}: trajectory not in time order");
            prev_t = t;
            assert!(t == end || (t - p.epoch_unix) % l_s == 0, "{d}: row not on a block end");
            last.insert((node, var), bits);
        }
        let mut checked = 0;
        for ti in 0..w.entities.names.len() {
            for (vi, vname) in w.state_names[ti].iter().enumerate() {
                let code = names.iter().position(|n| n == vname).unwrap() as u16;
                for local in 0..w.entities.counts[ti] as usize {
                    let node = w.entities.starts[ti] + local as u64;
                    let got = f64::from_bits(*last.get(&(node, code)).expect("initial row missing"));
                    let want = fin[ti][vi][local];
                    assert!(
                        got.to_bits() == want.to_bits(),
                        "{d}: node {node} var {vname}: trajectory {got} vs final {want}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }
}

fn edge_hash(r: &EventBatch) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for i in 0..r.len() {
        (r.src[i], r.dst[i], r.t[i], r.event_type[i], r.label[i], r.anomaly_id[i]).hash(&mut h);
        for c in &r.attrs {
            c[i].to_bits().hash(&mut h);
        }
    }
    h.finish()
}
