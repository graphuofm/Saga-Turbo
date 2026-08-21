//! `saga validate` must evaluate every declared Phi predicate post hoc,
//! including `state_non_negative`, which is declared in every domain's Phi
//! list but was previously never checked. Drives the REAL built binary and
//! cross-checks its counts against an independent re-read of the exported
//! node tables (the CSV path is parsed by hand here, so the cross-check does
//! not share code with the validator's own reader).

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_saga");

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "agora_validate_{}_{}_{}",
            std::process::id(),
            tag,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn generate(args: &[&str], out: &Path) -> serde_json::Value {
    let mut c = Command::new(BIN);
    c.arg("generate").args(args).args(["--json", "--quiet", "--out"]).arg(out);
    let o = c.output().expect("spawn saga");
    assert!(o.status.success(), "saga generate {args:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).expect("generate --json summary")
}

fn validate_json(out: &Path) -> serde_json::Value {
    let o = Command::new(BIN).args(["--json", "validate"]).arg(out).output().expect("spawn saga");
    assert!(o.status.success(), "saga validate failed:\n{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).expect("validate --json")
}

fn validate_human(out: &Path) -> String {
    let o = Command::new(BIN).arg("validate").arg(out).output().expect("spawn saga");
    assert!(o.status.success(), "saga validate failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// Independent recount from nodes.csv: (accounts, accounts with balance < 0,
/// min balance). Parses the header of the first block (accounts are the
/// first finance entity type, so the header names its columns).
fn recount_account_balances_csv(dir: &Path) -> (u64, u64, f64) {
    let text = std::fs::read_to_string(dir.join("nodes.csv")).expect("nodes.csv");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    let bi = header.iter().position(|c| *c == "balance").expect("balance column");
    let (mut n, mut neg, mut min) = (0u64, 0u64, f64::INFINITY);
    for line in lines {
        let f: Vec<&str> = line.split(',').collect();
        if f.get(1) != Some(&"account") {
            continue;
        }
        let v: f64 = f[bi].parse().unwrap();
        n += 1;
        if v < 0.0 {
            neg += 1;
        }
        min = min.min(v);
    }
    (n, neg, min)
}

fn balance_check(v: &serde_json::Value) -> &serde_json::Value {
    v["phi_state_non_negative"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "balance_non_negative")
        .expect("balance_non_negative reported")
}

const BASE: [&str; 10] =
    ["--domain", "finance", "--nodes", "20000", "--edges", "200000", "--time-span", "14", "--seed", "42"];

/// (a) guards off: final balances go negative (unguarded dynamics) and the
/// validator says VIOLATED with counts equal to an independent recount.
#[test]
fn guards_off_finance_reports_negative_balances() {
    let root = TempDir::new("goff");
    let out = root.path().join("run");
    let mut args = BASE.to_vec();
    args.extend(["--threads", "4", "--format", "csv"]);
    generate(&args, &out);

    let (n, neg, min) = recount_account_balances_csv(&out);
    assert!(neg > 0, "test premise: guards-off finance must end with some negative balances");

    let v = validate_json(&out);
    let bc = balance_check(&v);
    assert_eq!(bc["verdict"], "VIOLATED");
    assert_eq!(bc["nodes"].as_u64().unwrap(), n);
    assert_eq!(bc["violating_nodes"].as_u64().unwrap(), neg);
    assert!((bc["fraction"].as_f64().unwrap() - neg as f64 / n as f64).abs() < 1e-12);
    assert!((bc["min_value"].as_f64().unwrap() - min).abs() < 1e-3, "min {} vs {min}", bc["min_value"]);
    assert_eq!(v["guards"]["enabled"], false);
    assert_eq!(v["ok"], true, "hard checks must still pass; Phi is informational");

    let h = validate_human(&out);
    assert!(h.contains("VIOLATED"), "human report:\n{h}");
    assert!(h.contains(&format!("{neg} of {n} nodes < 0")), "human report:\n{h}");
}

/// Same run in parquet: the parquet node-table path must agree with the CSV
/// path (guards off, same seed => identical final state).
#[test]
fn parquet_and_csv_node_tables_agree() {
    let root = TempDir::new("fmt");
    let csv = root.path().join("csv");
    let pq = root.path().join("pq");
    let mut a = BASE.to_vec();
    a.extend(["--threads", "4", "--format", "csv"]);
    generate(&a, &csv);
    let mut b = BASE.to_vec();
    b.extend(["--threads", "4"]);
    generate(&b, &pq);
    let (_, neg, _) = recount_account_balances_csv(&csv);
    let vc = balance_check(&validate_json(&csv)).clone();
    let vp = balance_check(&validate_json(&pq)).clone();
    assert_eq!(vc["violating_nodes"].as_u64().unwrap(), neg);
    assert_eq!(vp["violating_nodes"], vc["violating_nodes"]);
    assert_eq!(vp["nodes"], vc["nodes"]);
    assert!((vp["min_value"].as_f64().unwrap() - vc["min_value"].as_f64().unwrap()).abs() < 1e-3);
}

/// (b) guards on at L = granularity (1 s = 1/86400 day): the overdraft guard
/// is essentially exact, so the guarded transfers contribute (near) zero
/// leakage; unguarded purchases/withdrawals may still drive balances below
/// zero. Assert the validator's count equals an independent recount, that it
/// is far below the guards-off count, and that the guard cross-reference
/// surfaces the engine's staleness counters.
#[test]
fn guards_on_exact_l_finance_matches_recount() {
    let root = TempDir::new("gon");
    let off = root.path().join("off");
    let on = root.path().join("on");
    let mut a = BASE.to_vec();
    a.extend(["--threads", "4", "--format", "csv"]);
    generate(&a, &off);
    let mut b = BASE.to_vec();
    b.extend(["--threads", "4", "--format", "csv", "--guards", "--block-len-days", "0.0000115740740741"]);
    let summary = generate(&b, &on);

    let (n, neg_on, min_on) = recount_account_balances_csv(&on);
    let (_, neg_off, _) = recount_account_balances_csv(&off);

    let v = validate_json(&on);
    let bc = balance_check(&v);
    assert_eq!(bc["nodes"].as_u64().unwrap(), n);
    assert_eq!(bc["violating_nodes"].as_u64().unwrap(), neg_on, "validator count must equal recount");
    if neg_on == 0 {
        assert_eq!(bc["verdict"], "PASS");
    } else {
        assert_eq!(bc["verdict"], "VIOLATED");
        assert!((bc["min_value"].as_f64().unwrap() - min_on).abs() < 1e-3);
    }
    assert!(
        neg_on * 2 < neg_off,
        "guards at L = 1 s must leave far fewer negative balances: on {neg_on} vs off {neg_off}"
    );

    let guarded = summary["overdraft_guarded"].as_u64().unwrap();
    let leaked = summary["overdraft_violations"].as_u64().unwrap();
    assert!(guarded > 0);
    assert_eq!(v["guards"]["enabled"], true);
    assert_eq!(v["guards"]["guarded_events"].as_u64().unwrap(), guarded);
    assert_eq!(v["guards"]["staleness_violations"].as_u64().unwrap(), leaked);
    let h = validate_human(&on);
    assert!(h.contains("guards were ON"), "human report:\n{h}");
    assert!(h.contains("block-start snapshot"), "human report:\n{h}");
}

/// (c) Documented constraints appear in the report, so the report is
/// complete over the declared Phi list; every state_non_negative predicate
/// of the domain is evaluated (crypto declares two, on different entity
/// types, which exercises the positional CSV column lookup).
#[test]
fn documented_and_all_state_predicates_listed() {
    let root = TempDir::new("doc");
    let fin = root.path().join("fin");
    let mut a = BASE.to_vec();
    a.extend(["--threads", "4"]);
    generate(&a, &fin);
    let v = validate_json(&fin);
    let docs = v["phi_documented"].as_array().unwrap();
    assert!(docs.iter().any(|d| d["name"] == "mule_dwell"), "{docs:?}");
    assert!(docs.iter().all(|d| d["status"] == "documented (not machine-checked)"));
    let h = validate_human(&fin);
    assert!(h.contains("mule_dwell"), "human report:\n{h}");
    assert!(h.contains("documented (not machine-checked)"), "human report:\n{h}");

    let rb = saga_rules::load_builtin_rulebase("crypto").unwrap();
    let declared: Vec<(String, String)> = rb
        .constraints
        .iter()
        .filter_map(|c| match &c.check {
            saga_rules::ConstraintCheck::StateNonNegative { entity, var } => Some((entity.clone(), var.clone())),
            _ => None,
        })
        .collect();
    assert!(declared.len() >= 2, "crypto declares several state_non_negative predicates");
    for fmt in [None, Some("csv")] {
        let out = root.path().join(format!("crypto_{}", fmt.unwrap_or("parquet")));
        let mut c = vec!["--domain", "crypto", "--nodes", "20000", "--edges", "100000", "--time-span", "14",
            "--seed", "42", "--threads", "4"];
        if let Some(f) = fmt {
            c.extend(["--format", f]);
        }
        generate(&c, &out);
        let v = validate_json(&out);
        let checks = v["phi_state_non_negative"].as_array().unwrap();
        for (entity, var) in &declared {
            let c = checks
                .iter()
                .find(|c| c["entity"] == entity.as_str() && c["var"] == var.as_str())
                .unwrap_or_else(|| panic!("{entity}.{var} missing from report"));
            assert_eq!(c["table_found"], true, "{entity}.{var} table not found ({fmt:?})");
            assert!(c["nodes"].as_u64().unwrap() > 0);
            assert!(c["verdict"] == "PASS" || c["verdict"] == "VIOLATED");
        }
    }
}
