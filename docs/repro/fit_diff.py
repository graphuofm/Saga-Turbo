import json, numpy as np

S = "$WORK"
r42 = json.load(open(f"{S}/diff_raw.json"))
r43 = json.load(open(f"{S}/diff_raw_seed43.json"))
DELTAS = [0.0, 0.25, 0.5, 0.75, 1.0]
VIEWS = [("all", "full"), ("attr", "attribute-only"), ("struct", "structure-only")]

out = {
    "setup": {
        "domain": "finance", "edges_requested": 10_000_000, "anomaly_rate_pi": 0.05,
        "gen_seeds": [42, 43], "detector_seed": 0, "format": "csv",
        "shard_size_mb": 8192,
        "why_this_scale": (
            "JOB 1 measured finance frac = 0.05424*pi, so pi=0.05 gives ~0.26% of edges anomalous. "
            "At 1e7 edges that is ~26k anomalous edges: above the ~20k target for training while "
            "keeping the base rate at its honest intrinsic value (~0.26%, NOT the ~30% the "
            "elasticity bug used to produce). 1e8 edges would give ~260k positives but a 6GB+ CSV "
            "that detector.py must read into pandas; 1e7 is the largest scale that trains "
            "comfortably and it already clears the positives bar."),
        "shard_note": ("--shard-size 8192 forces a single edges_00000.csv because detector.py "
                       "reads only the first edges*.csv shard; no data is changed by this."),
        "edge_invariance_check": (
            "finance pi=0.05 delta=0.5 (the default) at 1e7 -> 25,780/9,983,506 = 0.2582%; "
            "JOB 1's same config at 1e8 -> 258,677/99,803,433 = 0.2592%. The anomaly fraction is "
            "edge-count-invariant to within 0.4% relative, confirming the elasticity fix."),
    },
    "runs": {"seed42": r42, "seed43": r43},
}

lines = []
lines.append("JOB 2 - difficulty vs detectability, at a scale with enough positives")
lines.append("finance, --edges 1e7, --anomaly-rate 0.05, delta in {0,0.25,0.5,0.75,1.0}")
lines.append("Two independent GENERATION seeds (42, 43); detector seed 0. All numbers real runs.")
lines.append("")
lines.append(out["setup"]["why_this_scale"])
lines.append("")
lines.append("EDGE-INVARIANCE CHECK: " + out["setup"]["edge_invariance_check"])
lines.append("")

for seed, runs in (("42", r42), ("43", r43)):
    lines.append(f"--- generation seed {seed} ---")
    hdr = (f"{'delta':>6}{'anom_edges':>12}{'base_rate_%':>13}"
           f"{'AUC_full':>10}{'AUC_attr':>10}{'AUC_struct':>12}"
           f"{'AP_full':>10}{'AP_attr':>10}{'AP_struct':>11}")
    lines.append(hdr); lines.append("-" * len(hdr))
    for c in runs:
        lines.append(f"{c['delta']:>6}{c['anomalous_edges']:>12,}{100*c['fraction']:>13.4f}"
                     f"{c['all']['roc_auc']:>10.4f}{c['attr']['roc_auc']:>10.4f}{c['struct']['roc_auc']:>12.4f}"
                     f"{c['all']['ap']:>10.4f}{c['attr']['ap']:>10.4f}{c['struct']['ap']:>11.4f}")
    lines.append("")

ana = {}
for key, label in VIEWS:
    ana[key] = {}
    for metric in ("roc_auc", "ap"):
        v42 = [c[key][metric] for c in r42]
        v43 = [c[key][metric] for c in r43]
        mono42 = all(v42[i] > v42[i + 1] for i in range(4))
        mono43 = all(v43[i] > v43[i + 1] for i in range(4))
        ana[key][metric] = {
            "seed42": v42, "seed43": v43,
            "monotone_decreasing_seed42": mono42,
            "monotone_decreasing_seed43": mono43,
            "monotone_in_both_seeds": mono42 and mono43,
            "range_seed42": float(max(v42) - min(v42)),
            "range_seed43": float(max(v43) - min(v43)),
            "delta0_to_delta1_seed42": [v42[0], v42[-1]],
            "delta0_to_delta1_seed43": [v43[0], v43[-1]],
            "ratio_delta0_over_delta1_seed42": float(v42[0] / v42[-1]),
            "ratio_delta0_over_delta1_seed43": float(v43[0] / v43[-1]),
            "max_abs_seed_disagreement_per_cell": float(max(abs(a - b) for a, b in zip(v42, v43))),
        }
out["analysis"] = ana

# anomaly volume vs delta
out["anomaly_volume_vs_delta"] = {
    "seed42": {str(c["delta"]): c["anomalous_edges"] for c in r42},
    "seed43": {str(c["delta"]): c["anomalous_edges"] for c in r43},
    "note": ("CONFOUND: raising delta also REDUCES the number of anomalous edges (34,652 -> 18,316 "
             "at seed 42; 37,540 -> 19,891 at seed 43, a ~1.9x drop). So the delta knob changes both "
             "camouflage and anomaly volume; the AP decline is partly a rarer-positives effect, not "
             "camouflage alone. This confound should be stated in the paper."),
}

lines.append("MONOTONICITY AND EFFECT SIZE (per view, per metric)")
for key, label in VIEWS:
    for metric in ("roc_auc", "ap"):
        a = ana[key][metric]
        lines.append(f"  {label:<16} {metric:<8} monotone-decreasing in BOTH seeds: {str(a['monotone_in_both_seeds']):<5} "
                     f"| delta0->delta1: {a['delta0_to_delta1_seed42'][0]:.3f}->{a['delta0_to_delta1_seed42'][1]:.3f} (s42), "
                     f"{a['delta0_to_delta1_seed43'][0]:.3f}->{a['delta0_to_delta1_seed43'][1]:.3f} (s43) "
                     f"| max seed disagreement per cell: {a['max_abs_seed_disagreement_per_cell']:.3f}")
lines.append("")
lines.append("ANOMALY VOLUME vs delta (the confound):")
lines.append("  " + out["anomaly_volume_vs_delta"]["note"])
lines.append("")
lines.append("VERDICT (from these measurements):")
lines.append("  * ROC-AUC is NOT monotone in delta at either seed, and the non-monotone cells")
lines.append("    disagree BETWEEN seeds (s42 bumps up at delta=0.25, s43 dips there), i.e. the")
lines.append("    wiggle is seed noise of ~+-0.04 AUC, not structure. The overall trend is down")
lines.append("    (0.862->0.817 s42; 0.923->0.811 s43): a ~0.05-0.11 AUC move. ROC-AUC is the wrong")
lines.append("    metric at a 0.26% base rate.")
lines.append("  * AVERAGE PRECISION is strictly monotone decreasing in delta at BOTH seeds, for the")
lines.append("    full view (0.547->0.132, 4.1x, s42; 0.640->0.147, 4.4x, s43) and the attribute-only")
lines.append("    view (0.411->0.030, 13.9x, s42; 0.470->0.033, 14.1x, s43). The knob genuinely works,")
lines.append("    and it works on ATTRIBUTES.")
lines.append("  * The structure-only view barely moves and if anything gets EASIER with delta")
lines.append("    (AUC 0.908->0.945 s42, 0.930->0.946 s43). SAGA's camouflage does not hide the")
lines.append("    anomalies' structural footprint. This is a real limitation and should be stated.")
open(f"{S}/diff_scale.txt", "w").write("\n".join(lines) + "\n")
json.dump(out, open(f"{S}/diff_scale.json", "w"), indent=2)
print("\n".join(lines))
