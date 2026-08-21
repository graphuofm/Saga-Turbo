#!/usr/bin/env python3
"""Full per-statistic fidelity breakdown: transport (agora) vs sx-askubuntu."""
import sys, os, json
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare, format_scorecard

HOME = os.path.expanduser("~")
CAP = 1_500_000
def cap_t(s, d, t, cap=CAP):
    if s.size <= cap: return s, d, t
    order = np.sort(np.argsort(t, kind="stable")[:cap])
    return s[order], d[order], t[order]

synth_dir = sys.argv[1]
rs, rd, rt = cap_t(*load_real(f"{HOME}/.cache/zord/sx-askubuntu.txt.gz"))
ss, sd, st = cap_t(*load_saga(synth_dir))
real = compute_stats(rs, rd, rt)
synth = compute_stats(ss, sd, st)
res = compare(real, synth)
print(format_scorecard(res, "sx-askubuntu", synth_dir))
print("\n--- raw component distances (1-dist contributes to fidelity) ---")
comps = []
for k, v in res["ks_distances"].items():
    if v == v: comps.append((k, v));
for k, t in res["scalar_metrics"].items():
    if t["distance"] == t["distance"]: comps.append((k, t["distance"]))
for k, v in sorted(comps, key=lambda x: -x[1]):
    print(f"  {k:<20} dist={v:.4f}   contrib_to_(1-fid)={v:.4f}")
print(f"\n  mean_dist={np.mean([c[1] for c in comps]):.4f}  fidelity={1-np.mean([c[1] for c in comps]):.4f}")
print(f"  n_components={len(comps)}")
