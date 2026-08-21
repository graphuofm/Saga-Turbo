#!/usr/bin/env python3
"""Render anomaly_validation_after2.txt from anomaly_validation_after2.json.
Every column is read programmatically; nothing is transcribed by hand."""
import json

S = "./scratch"
d = json.load(open(f"{S}/anomaly_validation_after2.json"))


def row(x):
    if x is None:
        return None
    a, n, c = x["anomalous"], x["normal"], x["contrast"]
    return {
        "mean_degree_ratio": c["mean_degree_ratio"],
        "clustering_ratio": c["clustering_ratio"],
        "in_out_ratio_anom": a["in_out_ratio_mean"],
        "homophily_lift_anom": a["homophily_lift"],
        "anom_edge_share": a["n_incident_edges"] / x["n_edges_total"],
        "degree_KS": c["degree_KS_anom_vs_norm"],
        "anom_mean_deg": a["mean_total_degree"], "norm_mean_deg": n["mean_total_degree"],
        "anom_clustering": a["clustering"], "norm_clustering": n["clustering"],
        "anom_nodes": a["n_nodes"], "anom_inc_edges": a["n_incident_edges"],
        "anom_int_edges": a["n_internal_edges"],
        "powerlaw_alpha_anom": a["powerlaw_alpha"],
    }


# AFTER-1's raw per-population stats live under full_cfgA/full_cfgB in the
# post-multiplier JSON (its top-level after_cfg* keys are already row-form);
# read the RAW ones so every column goes through the identical row() reducer.
prev_raw = json.load(open(f"{S}/anomaly_validation_after.json"))

COLS = [("REAL (Elliptic)", row(d["real_elliptic"])),
        ("BEFORE (pre-mult)", row(d["before_prefix"])),
        ("AFTER-1 cfgA", row(prev_raw["full_cfgA"])),
        ("AFTER-1 cfgB", row(prev_raw["full_cfgB"])),
        ("AFTER-2 cfgA", row(d.get("after2_cfgA_matched"))),
        ("AFTER-2 cfgB", row(d.get("after2_cfgB_1e7")))]

KEYS = [("mean_degree_ratio", "mean total degree ratio (anom/norm)"),
        ("clustering_ratio", "local clustering ratio (anom/norm)"),
        ("in_out_ratio_anom", "in/out degree ratio (anomalous pop)"),
        ("homophily_lift_anom", "homophily lift (anomalous POPULATION)"),
        ("anom_edge_share", "anomaly share of all edges"),
        ("degree_KS", "degree KS (anom vs norm)")]
RAW = ["anom_mean_deg", "norm_mean_deg", "anom_clustering", "norm_clustering",
       "anom_nodes", "anom_inc_edges", "anom_int_edges", "powerlaw_alpha_anom"]


def f(x):
    if x is None:
        return "n/a"
    if x != x:
        return "nan"
    return f"{x:.3f}" if abs(x) < 1000 else f"{x:.1f}"


L = []
L.append("SAGA ANOMALY-STRUCTURE VALIDATION vs ELLIPTIC — REAL | BEFORE | AFTER-1 | AFTER-2")
L.append("=" * 118)
L.append("")
L.append("METHOD: scratchpad/anom.py reused UNMODIFIED (imported, not copied). BEFORE and AFTER-1 columns")
L.append("read programmatically from anomaly_validation.json / anomaly_validation_after.json.")
L.append(f"AFTER-2 binary: {d['binary_after2']}")
L.append("")
sc = d["elliptic_selfcheck"]
L.append("SELF-CHECK — Elliptic reference re-derived from scratch on this run:")
b = d["real_elliptic"]["basics"]
L.append(f"  n_nodes={b['n_nodes']} n_edges={b['n_edges']} n_illicit={b['n_illicit']} n_licit={b['n_licit']} "
         f"illicit_rate={b['illicit_rate_all_nodes']:.6f} T={b['n_time_steps']}")
L.append(f"  measured: {sc['measured']}")
L.append(f"  frozen  : {sc['expected']}")
L.append(f"  -> {'MATCHES exactly; pipeline confirmed identical.' if sc['ok'] else 'DRIFTED: ' + str(sc['drifted'])}")
L.append("")
L.append("CONFIGURATIONS (AFTER-2, fixed engine, all from real runs):")
for k, v in d["configs"].items():
    if v.get("status") == "OK":
        L.append(f"  {k}: {v['edges_actual']:,} edges, {v['anomalous_edges']:,} anomalous edges "
                 f"({v['anomalous_edge_rate']*100:.4f}%), {v['anomalous_nodes']:,} anomalous nodes")
        L.append(f"      cmd: {v['cmd']}")
    else:
        L.append(f"  {k}: {v['status']}")
L.append("  AFTER-1 configs, for reference (same generate config, pre-structural-fix binary):")
for k, v in d["after1_configs"].items():
    L.append(f"      {k}: {v['edges_actual']:,} edges, {v['anomalous_edges']:,} anomalous edges "
             f"({v['anomalous_edge_rate']*100:.4f}%), {v['anomalous_nodes']:,} anomalous nodes")
L.append("")
L.append("MAIN TABLE — anomalous vs normal contrasts")
L.append("-" * 118)
L.append(f"{'statistic':<40}" + "".join(f"|{c:>12}" for c, _ in COLS))
L.append("-" * 118)
for k, label in KEYS:
    L.append(f"{label:<40}" + "".join(f"|{f(r[k]) if r else 'n/a':>12}" for _, r in COLS))
L.append("-" * 118)
L.append("")
L.append("SUPPORTING RAW VALUES")
for k in RAW:
    L.append(f"  {k:<24}" + "".join(f"|{f(r[k]) if r else 'n/a':>12}" for _, r in COLS))
L.append("")
L.append("FIDELITY ON THE ANOMALY SUBGRAPHS (saga_eval.compare — same code path as the paper)")
fa2 = d.get("subgraph_fidelity_after2", {})
fa1 = d.get("after1_subgraph_fidelity", {})
fb = d.get("before_subgraph_fidelity", {})
wb = d.get("before_wholegraph_fidelity", {})
L.append(f"{'subgraph':<34}|{'BEFORE':>10}|{'AFTER-1 A':>10}|{'AFTER-1 B':>10}|{'AFTER-2 A':>10}|{'AFTER-2 B':>10}")


def g(x, k="fidelity_score"):
    if isinstance(x, (int, float)):  # BEFORE whole-graph is stored as a bare float
        return f"{x:.3f}"
    if not isinstance(x, dict):
        return "n/a"
    v = x.get(k)
    return f"{v:.3f}" if isinstance(v, (int, float)) else "n/a"


L.append(f"{'whole graph vs Elliptic':<34}|{g(wb):>10}|{g(fa1.get('cfgA_matched_wholegraph')):>10}|"
         f"{g(fa1.get('cfgB_1e7_wholegraph')):>10}|{g(fa2.get('A_matched_whole')):>10}|{g(fa2.get('B_1e7_whole')):>10}")
L.append(f"{'anomaly-incident subgraph':<34}|{g(fb.get('incident')) if isinstance(fb, dict) else 'n/a':>10}|"
         f"{g(fa1.get('cfgA_matched_incident')):>10}|{g(fa1.get('cfgB_1e7_incident')):>10}|"
         f"{g(fa2.get('A_matched_incident')):>10}|{g(fa2.get('B_1e7_incident')):>10}")
L.append(f"{'anomaly-internal subgraph':<34}|{g(fb.get('internal')) if isinstance(fb, dict) else 'n/a':>10}|"
         f"{g(fa1.get('cfgA_matched_internal')):>10}|{g(fa1.get('cfgB_1e7_internal')):>10}|"
         f"{g(fa2.get('A_matched_internal')):>10}|{g(fa2.get('B_1e7_internal')):>10}")
L.append("  AFTER-2 subgraph edge counts (real vs synth):")
for k, v in fa2.items():
    if isinstance(v, dict) and "synth_n_edges" in v:
        L.append(f"    {k:<24} real={v['real_n_edges']:>8}  synth={v['synth_n_edges']:>9}")
L.append("")
L.append("CAVEATS (carried forward, still apply):")
L.append("  - Elliptic nodes are TRANSACTIONS (a DAG); SAGA crypto nodes are ACCOUNTS. The reciprocity and")
L.append("    repeat-edge gaps are partly a UNITS MISMATCH, not purely a modelling error.")
L.append("  - The 0.650x real degree ratio is a tx-shape fact, NOT a valid account-world target; a defensible")
L.append("    account-world target is ~1.0-1.5. SAGA's post-fix ratio is therefore not automatically 'better'.")
L.append("  - Elliptic: 77.1% of nodes are unlabeled; contrasts use the 22.9% labeled only.")
L.append("  - --anomaly-rate was held at Elliptic's real 0.0223 in EVERY config; it was never raised to")
L.append("    manufacture positives. cfg B raises --edges only, which changes the anomalous-edge COUNT.")
open(f"{S}/anomaly_validation_after2.txt", "w").write("\n".join(L) + "\n")
print("\n".join(L))
