//! `saga validate` — consistency + constraint (Φ) checks on an output (§12).
//!
//! Two classes of result:
//!   HARD checks (fail = broken output): shards readable, time-sorted,
//!     endpoint ids in range, labels/event types in the declared sets.
//!   Φ candidates (informational): counts of constraint violations from the
//!     domain rule base. These are a detector's-eye view — violations may be
//!     legitimate tails OR injected anomalies; ground truth stays the label.
//!
//! Every constraint declared in the domain's Φ list is accounted for:
//!   attr_range / rate_limit / sub_threshold_count are evaluated over the edge
//!   stream; state_non_negative is evaluated post hoc over the exported node
//!   tables (final state per node: count, fraction and minimum of nodes whose
//!   variable ended below zero); documented constraints are listed as such
//!   (not machine-checked) so the report is complete over Φ rather than
//!   silent. Generation-time guards (--guards) enforce a SUBSET of the
//!   state_non_negative predicates against the block-start snapshot; the
//!   post-hoc check is the ground truth for what actually leaked.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use owo_colors::OwoColorize;
use saga_rules::{load_builtin_rulebase, ConstraintCheck, RunMeta};

use super::Ctx;

pub fn run(ctx: &Ctx, output: &Path) -> anyhow::Result<()> {
    let meta_path = output.join("saga_meta.json");
    if !meta_path.exists() {
        anyhow::bail!(
            "`{}` is not a SAGA output directory (no saga_meta.json found)",
            output.display()
        );
    }
    let meta: RunMeta = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
    let rb = load_builtin_rulebase(&meta.config.domain)?;
    let nodes = meta.config.scale.nodes;

    let mut event_types: Vec<String> = rb.event_types.iter().map(|e| e.name.clone()).collect();
    event_types.sort();
    let mut labels: Vec<String> = vec!["normal".into()];
    labels.extend(rb.adversaries.iter().map(|a| a.intent.clone()));
    labels.extend(rb.failures.iter().map(|f| f.intent.clone()));

    // Hard-check state.
    let mut total = 0u64;
    let mut last_t = i64::MIN;
    let mut unsorted = 0u64;
    let mut bad_ids = 0u64;
    let mut bad_enum = 0u64;

    // Φ state.
    let mut range_violations: HashMap<String, u64> = HashMap::new();
    struct WindowCheck {
        name: String,
        event: String,
        attr: Option<String>,
        floor: f64,
        threshold: f64,
        k: u32,
        window_s: u64,
        per_src: HashMap<u64, VecDeque<i64>>,
        violations: u64,
    }
    let mut window_checks: Vec<WindowCheck> = Vec::new();
    for c in &rb.constraints {
        match &c.check {
            ConstraintCheck::SubThresholdCount { event, attr, threshold, floor, k, window_s } => {
                window_checks.push(WindowCheck {
                    name: c.name.clone(),
                    event: event.clone(),
                    attr: Some(attr.clone()),
                    floor: *floor,
                    threshold: *threshold,
                    k: *k,
                    window_s: *window_s,
                    per_src: HashMap::new(),
                    violations: 0,
                });
            }
            ConstraintCheck::RateLimit { event, k, window_s } => {
                window_checks.push(WindowCheck {
                    name: c.name.clone(),
                    event: event.clone(),
                    attr: None,
                    floor: f64::NEG_INFINITY,
                    threshold: f64::INFINITY,
                    k: *k,
                    window_s: *window_s,
                    per_src: HashMap::new(),
                    violations: 0,
                });
            }
            _ => {}
        }
    }

    let shards = saga_io::read::edge_shards(output)?;
    for shard in &shards {
        saga_io::read::read_shard(shard, &mut |b| {
            for i in 0..b.src.len() {
                total += 1;
                let t = b.t[i];
                if t < last_t {
                    unsorted += 1;
                }
                last_t = t;
                if b.src[i] >= nodes || b.dst[i] >= nodes {
                    bad_ids += 1;
                }
                if event_types.binary_search(&b.event_type[i]).is_err()
                    || !labels.contains(&b.label[i])
                {
                    bad_enum += 1;
                }
                // AttrRange checks.
                for c in &rb.constraints {
                    if let ConstraintCheck::AttrRange { event, attr, min, max } = &c.check {
                        if &b.event_type[i] == event {
                            if let Some((_, vals)) =
                                b.numeric_attrs.iter().find(|(n, _)| n == attr)
                            {
                                let v = vals[i];
                                if !v.is_nan() && (v < *min || v > *max) {
                                    *range_violations.entry(c.name.clone()).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }
                // Sliding-window checks.
                for wc in window_checks.iter_mut() {
                    if b.event_type[i] != wc.event {
                        continue;
                    }
                    if let Some(attr) = &wc.attr {
                        let v = b
                            .numeric_attrs
                            .iter()
                            .find(|(n, _)| n == attr)
                            .map(|(_, vals)| vals[i])
                            .unwrap_or(f64::NAN);
                        if !(v >= wc.floor && v < wc.threshold) {
                            continue; // only near-threshold events count
                        }
                    }
                    let q = wc.per_src.entry(b.src[i]).or_default();
                    let cutoff = t - wc.window_s as i64;
                    while q.front().is_some_and(|&ft| ft < cutoff) {
                        q.pop_front();
                    }
                    q.push_back(t);
                    if q.len() as u32 >= wc.k {
                        wc.violations += 1;
                        q.clear(); // count each run once
                    }
                }
            }
            Ok(())
        })?;
    }

    // State non-negativity, evaluated post hoc over the exported node tables
    // (final state). Guards, when on, enforce a subset of these against the
    // block-start snapshot; unguarded subtracting events and staleness can
    // still drive the final value negative, and that is what is reported.
    struct StateCheck {
        name: String,
        entity: String,
        var: String,
        nodes: u64,
        violating: u64,
        min_value: f64,
        table_found: bool,
    }
    let mut state_checks: Vec<StateCheck> = Vec::new();
    let mut documented: Vec<(String, String)> = Vec::new();
    for c in &rb.constraints {
        match &c.check {
            ConstraintCheck::StateNonNegative { entity, var } => {
                let position = rb.entity_types.iter().find(|e| &e.name == entity).and_then(|e| {
                    e.state
                        .iter()
                        .position(|s| &s.name == var)
                        .map(|si| e.attributes.len() + si)
                });
                let col = saga_io::read::node_column(output, entity, var, position)?;
                let mut sc = StateCheck {
                    name: c.name.clone(),
                    entity: entity.clone(),
                    var: var.clone(),
                    nodes: 0,
                    violating: 0,
                    min_value: f64::NAN,
                    table_found: col.is_some(),
                };
                if let Some(vals) = col {
                    sc.nodes = vals.len() as u64;
                    for &v in &vals {
                        if v.is_nan() {
                            continue;
                        }
                        if v < 0.0 {
                            sc.violating += 1;
                        }
                        if sc.min_value.is_nan() || v < sc.min_value {
                            sc.min_value = v;
                        }
                    }
                }
                state_checks.push(sc);
            }
            ConstraintCheck::Documented { rule } => documented.push((c.name.clone(), rule.clone())),
            _ => {}
        }
    }

    // Guard cross-reference from the run summary embedded in saga_meta.json.
    // saga_meta does not record the --guards flag itself; the engine reports
    // `overdraft_guarded > 0` only when guards were enabled.
    let result = meta.result.as_ref();
    let guarded = result.and_then(|r| r.get("overdraft_guarded")).and_then(|v| v.as_u64()).unwrap_or(0);
    let guard_violations =
        result.and_then(|r| r.get("overdraft_violations")).and_then(|v| v.as_u64()).unwrap_or(0);
    let guards_on = guarded > 0;
    let guard_note = if guards_on {
        format!(
            "guards were ON for this run: guarded predicates were enforced at generation time \
             against the block-start snapshot (stale); the engine counted {guard_violations} of \
             {guarded} guarded events that exceeded the true up-to-date state (staleness \
             leakage, the quantity the paper measures). Final-state violations above, if any, \
             are that leakage plus unguarded subtracting events."
        )
    } else {
        "guards were OFF (or the run was not whole-graph): no predicate was enforced at \
         generation time; final-state violations above are the unguarded dynamics."
            .to_string()
    };

    let hard_ok = unsorted == 0 && bad_ids == 0 && bad_enum == 0;
    if ctx.json {
        let state_json: Vec<serde_json::Value> = state_checks
            .iter()
            .map(|sc| {
                serde_json::json!({
                    "name": sc.name,
                    "entity": sc.entity,
                    "var": sc.var,
                    "table_found": sc.table_found,
                    "nodes": sc.nodes,
                    "violating_nodes": sc.violating,
                    "fraction": if sc.nodes > 0 { sc.violating as f64 / sc.nodes as f64 } else { 0.0 },
                    "min_value": if sc.min_value.is_nan() { serde_json::Value::Null } else { serde_json::json!(sc.min_value) },
                    "verdict": if !sc.table_found { "NO_TABLE" } else if sc.violating == 0 { "PASS" } else { "VIOLATED" },
                })
            })
            .collect();
        let documented_json: Vec<serde_json::Value> = documented
            .iter()
            .map(|(n, r)| serde_json::json!({ "name": n, "rule": r, "status": "documented (not machine-checked)" }))
            .collect();
        let phi: HashMap<String, u64> = range_violations
            .into_iter()
            .chain(window_checks.iter().map(|w| (w.name.clone(), w.violations)))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "edges": total,
                "shards": shards.len(),
                "hard_checks": { "time_sorted": unsorted == 0, "ids_in_range": bad_ids == 0, "enums_valid": bad_enum == 0 },
                "phi_candidates": phi,
                "phi_state_non_negative": state_json,
                "phi_documented": documented_json,
                "guards": {
                    "enabled": guards_on,
                    "guarded_events": guarded,
                    "staleness_violations": guard_violations,
                    "note": guard_note,
                },
                "ok": hard_ok,
            }))?
        );
    } else {
        println!("{}", format!("saga validate — {}", output.display()).bold());
        println!("  edges       {total} across {} shard(s)", shards.len());
        let mark = |ok: bool| if ok { "PASS".green().to_string() } else { "FAIL".red().to_string() };
        println!("  [{}] time-sorted stream ({unsorted} out-of-order)", mark(unsorted == 0));
        println!("  [{}] endpoint ids in range ({bad_ids} bad)", mark(bad_ids == 0));
        println!("  [{}] event types & labels in declared sets ({bad_enum} bad)", mark(bad_enum == 0));
        println!("  Φ constraint candidates (informational, label is ground truth):");
        for (name, v) in &range_violations {
            println!("    {name:<28} {v}");
        }
        for wc in &window_checks {
            println!("    {:<28} {}", wc.name, wc.violations);
        }
        println!("  Φ state non-negativity (post hoc, final state from node tables):");
        for sc in &state_checks {
            if !sc.table_found {
                println!(
                    "    [{}] {:<28} {}.{}: no node table in output (edge-only run?)",
                    "SKIP".yellow(),
                    sc.name,
                    sc.entity,
                    sc.var
                );
                continue;
            }
            let frac = if sc.nodes > 0 { sc.violating as f64 / sc.nodes as f64 } else { 0.0 };
            println!(
                "    [{}] {:<28} {}.{}: {} of {} nodes < 0 ({:.4}%), min {:.4}",
                if sc.violating == 0 { "PASS".green().to_string() } else { "VIOLATED".red().to_string() },
                sc.name,
                sc.entity,
                sc.var,
                sc.violating,
                sc.nodes,
                frac * 100.0,
                sc.min_value
            );
        }
        println!("  guards      {guard_note}");
        println!("  Φ documented (not machine-checked):");
        for (n, r) in &documented {
            println!("    {n:<28} {r}");
        }
        println!(
            "  verdict     {}",
            if hard_ok { "consistent".green().bold().to_string() } else { "BROKEN".red().bold().to_string() }
        );
    }
    anyhow::ensure!(hard_ok, "hard consistency checks failed");
    Ok(())
}
