"""aggregate.py — collect per-cell JSONs into the two paper tables (mean +/- std)."""
from __future__ import annotations
import glob, json, os, sys
import numpy as np

RES = sys.argv[1] if len(sys.argv) > 1 else "results"
OUT = sys.argv[2] if len(sys.argv) > 2 else "downstream_gpu"

DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
PRE_DM = {  # pre-fix published (single-seed), for direction comparison only
    "finance": (0.956, 0.725, 0.825, 0.500), "crypto": (0.985, 0.659, 0.925, 0.500),
    "cyber": (0.941, 0.702, 0.793, 0.500), "transport": (0.986, 0.727, 0.931, 0.500),
    "ecommerce": (0.943, 0.703, 0.697, 0.500), "healthcare": (0.972, 0.664, 0.883, 0.500),
}


def load(pat):
    out = []
    for f in sorted(glob.glob(pat)):
        with open(f) as fh:
            out.append(json.load(fh))
    return out


def ms(vals):
    v = [x for x in vals if x is not None and np.isfinite(x)]
    if not v:
        return None, None, 0
    return float(np.mean(v)), float(np.std(v, ddof=1)) if len(v) > 1 else 0.0, len(v)


def get(recs, key, sub):
    vals = []
    for r in recs:
        d = r.get(key, {})
        if sub in d:
            vals.append(d[sub])
    return vals


def fmt(m, s, n):
    if m is None:
        return "FAILED"
    return f"{m:.3f}+/-{s:.3f}"


res = {"config": {
    "tab:downmulti": "saga generate --domain <D> --edges 200000 --seed <S> "
                     "(default anomaly rate); BA size-matched to that SAGA graph "
                     "(n, m, span) via saga_eval.baselines.gen_ba, uniform-random timestamps",
    "tab:downstream": "saga generate --domain finance --nodes 1899 --edges 59835 "
                      "--time-span 193.7057986 --seed <S>; real = SNAP CollegeMsg; "
                      "BA matched to real (n=1899, m=59835, span)",
    "tgn": "saga_eval.tgn_eval.run_tgn UNMODIFIED (6 epochs, mem_dim=64, batch=200). "
           "NOTE: run_tgn hardcodes device=cpu, so the GPU was allocated but unused; "
           "cells were parallelised across CPU cores instead.",
    "edgebank": "saga_eval.downstream_extra.run_edgebank UNMODIFIED; EdgeBank_inf reported",
    "seeds": "5 seeds (0-4); seed drives BOTH graph generation and model/negative sampling",
    "metric": "test AP, mean +/- sample std (ddof=1) over seeds",
}, "downmulti": {}, "downstream": {}}

lines = []
lines.append("=" * 78)
lines.append("tab:downmulti — downstream learnability per domain (test AP, mean +/- std, 5 seeds)")
lines.append("2e5 events per graph. TGN = saga_eval.tgn_eval.run_tgn; EdgeBank = EdgeBank_inf.")
lines.append("=" * 78)
lines.append(f"{'domain':<12}{'TGN SAGA':>16}{'TGN BA':>16}{'EB SAGA':>16}{'EB BA':>16}")
agg = {k: [] for k in ["ts", "tb", "es", "eb"]}
for d in DOMAINS:
    sg = load(f"{RES}/downmulti/{d}_saga_s*.json")
    ba = load(f"{RES}/downmulti/{d}_ba_s*.json")
    row = {}
    cells = {}
    for tag, recs, key, sub in (("ts", sg, "tgn", "test_ap"), ("tb", ba, "tgn", "test_ap"),
                                ("es", sg, "edgebank", "inf_test_ap"),
                                ("eb", ba, "edgebank", "inf_test_ap")):
        m, s, n = ms(get(recs, key, sub))
        cells[tag] = (m, s, n)
        row[tag] = {"mean": m, "std": s, "n_seeds": n}
        if m is not None:
            agg[tag].append(m)
    row["repeat_edge_fraction_saga"] = ms([r["repeat_edge_fraction"] for r in sg])[0]
    row["repeat_edge_fraction_ba"] = ms([r["repeat_edge_fraction"] for r in ba])[0]
    row["pre_fix_published"] = dict(zip(["tgn_saga", "tgn_ba", "eb_saga", "eb_ba"], PRE_DM[d]))
    res["downmulti"][d] = row
    lines.append(f"{d:<12}" + "".join(f"{fmt(*cells[t]):>16}" for t in ["ts", "tb", "es", "eb"]))
means = {t: (float(np.mean(v)) if v else None) for t, v in agg.items()}
res["downmulti"]["_mean_over_domains"] = means
lines.append("-" * 78)
lines.append(f"{'mean':<12}" + "".join(
    f"{(f'{means[t]:.3f}' if means[t] is not None else 'n/a'):>16}" for t in ["ts", "tb", "es", "eb"]))
lines.append("")
lines.append("pre-fix published means (single-seed): TGN SAGA 0.964  BA 0.697 | "
             "EB SAGA 0.842  BA 0.500")

# ---- tab:downstream ----
lines.append("")
lines.append("=" * 78)
lines.append("tab:downstream — SAGA vs REAL CollegeMsg vs BA (test AP, mean +/- std, 5 seeds)")
lines.append("=" * 78)
cols = ["real", "saga", "ba"]
data = {c: load(f"{RES}/downstream/{c}_s*.json") for c in cols}
lines.append(f"{'model':<26}" + "".join(f"{c:>18}" for c in cols))
for label, key, sub in (("TGN (learned)", "tgn", "test_ap"),
                        ("EdgeBank_inf", "edgebank", "inf_test_ap"),
                        ("EdgeBank_tw", "edgebank", "tw_test_ap")):
    cs = {}
    for c in cols:
        m, s, n = ms(get(data[c], key, sub))
        cs[c] = (m, s, n)
        res["downstream"].setdefault(c, {})[label] = {"mean": m, "std": s, "n_seeds": n}
    lines.append(f"{label:<26}" + "".join(f"{fmt(*cs[c]):>18}" for c in cols))
rf = {}
for c in cols:
    m, s, n = ms([r["repeat_edge_fraction"] for r in data[c]])
    rf[c] = (m, s, n)
    res["downstream"].setdefault(c, {})["repeat_edge_fraction"] = {"mean": m, "std": s, "n_seeds": n}
lines.append(f"{'repeated-edge fraction':<26}" + "".join(f"{fmt(*rf[c]):>18}" for c in cols))
lines.append("")
lines.append("pre-fix published (single-seed): TGN 0.943/0.843/0.611 (saga/real/ba); "
             "EdgeBank 0.942/0.764/0.500; repeat 0.94/0.83/~0")

# ---- THE DIRECTION VERDICT ----
lines.append("")
lines.append("=" * 78)
lines.append("SAGA-vs-REAL GAP (positive = SAGA MORE predictable than real = EASIER than reality)")
lines.append("=" * 78)
verdict = {}
for label, key, sub, pre in (("TGN", "tgn", "test_ap", 0.943 - 0.843),
                             ("EdgeBank_inf", "edgebank", "inf_test_ap", 0.942 - 0.764)):
    sm, ss, sn = ms(get(data["saga"], key, sub))
    rm, rs, rn = ms(get(data["real"], key, sub))
    if sm is None or rm is None:
        lines.append(f"{label:<14} FAILED (missing cells)")
        continue
    gap = sm - rm
    d = "NARROWED" if abs(gap) < abs(pre) else ("WIDENED" if abs(gap) > abs(pre) else "UNCHANGED")
    sign = "SAGA easier than real" if gap > 0 else "SAGA harder than real"
    verdict[label] = {"saga_mean": sm, "real_mean": rm, "gap_post_fix": gap,
                      "gap_pre_fix": pre, "direction": d, "interpretation": sign}
    lines.append(f"{label:<14} SAGA {sm:.3f}  real {rm:.3f}  gap {gap:+.3f}  "
                 f"(pre-fix {pre:+.3f})  -> {d}  [{sign}]")
res["verdict"] = verdict

txt = "\n".join(lines)
print(txt)
with open(f"{OUT}.txt", "w") as fh:
    fh.write(txt + "\n")
with open(f"{OUT}.json", "w") as fh:
    json.dump(res, fh, indent=2)
print(f"\nwrote {OUT}.json / {OUT}.txt")
