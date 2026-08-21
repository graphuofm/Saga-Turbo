#!/usr/bin/env python3
"""tab:fidelity re-measurement on the FINAL fixed engine (target/release/saga).
Exact reproduction of the submission driver (scratchpad/fidelity_driver.py ->
fidelity_multi.json): CAP=2.5M, cap real & AGORA by earliest-time window, match
AGORA to real (n,m,span), seed 42, compare(). The three paper columns are:
  fid   = fidelity_score
  B r/s = burstiness_b   (real/synth)
  recip = repeat_edge_ratio (real/synth)   [paper label 'recip.' = recurrence]
"""
import sys, os, json, time, subprocess, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare

HOME = os.path.expanduser("~")
SP = "./scratch/fidwork"
BIN = "./target/release/saga"   # FINAL fixed engine
CAP = 2_500_000
os.makedirs(SP, exist_ok=True)

PAIRS = [
    ("finance",   "email-Eu",        f"./realdata/snap-email-eu/email-Eu-core-temporal.txt", {}),
    ("finance",   "CollegeMsg",       f"./realdata/snap-collegemsg/CollegeMsg.txt", {}),
    ("crypto",    "wiki-talk",        f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}),
    ("crypto",    "sx-mathoverflow",  f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}),
    ("cyber",     "sx-superuser",     f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}),
    ("cyber",     "sx-askubuntu",     f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}),
    ("ecommerce", "tgbl-review",      f"./realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
                    dict(src_col="source", dst_col="target", t_col="ts")),
]

def cap_by_time(s, d, t, cap):
    if s.size <= cap:
        return s, d, t
    order = np.argsort(t, kind="stable")[:cap]
    order = np.sort(order)
    return s[order], d[order], t[order]

results = []
for domain, rname, path, kw in PAIRS:
    t0 = time.time()
    rs, rd, rt = load_real(path, **kw)
    rs, rd, rt = cap_by_time(rs, rd, rt, CAP)
    n = int(np.unique(np.concatenate([rs, rd])).size)
    m = int(rs.size)
    span_days = max(1.0, (rt.max() - rt.min()) / 86400.0)
    outdir = os.path.join(SP, f"{domain}_{rname}")
    shutil.rmtree(outdir, ignore_errors=True)
    cmd = [BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
           "--time-span", str(round(span_days, 2)), "--seed", "42",
           "--anomaly-rate", "0.02", "--out", outdir, "--quiet"]
    subprocess.run(cmd, check=True, stderr=subprocess.DEVNULL)
    ss, sd, st_ = load_saga(outdir)
    ss, sd, st_ = cap_by_time(ss, sd, st_, CAP)
    rst = compute_stats(rs, rd, rt, seed=0)
    sst = compute_stats(ss, sd, st_, seed=0)
    c = compare(rst, sst)
    row = dict(domain=domain, real=rname, real_edges=m, real_nodes=n,
               span_days=round(span_days, 2), synth_edges=int(ss.size),
               fidelity=round(c["fidelity_score"], 3),
               real_B=round(rst.burstiness_b, 3), saga_B=round(sst.burstiness_b, 3),
               real_repeat=round(rst.repeat_edge_ratio, 3), saga_repeat=round(sst.repeat_edge_ratio, 3),
               real_recip=round(rst.reciprocity, 3), saga_recip=round(sst.reciprocity, 3))
    results.append(row)
    shutil.rmtree(outdir, ignore_errors=True)
    print(f"[{domain}/{rname}] fid={row['fidelity']:.3f}  B {row['real_B']:.2f}/{row['saga_B']:.2f}  "
          f"repeat {row['real_repeat']:.2f}/{row['saga_repeat']:.2f}  ({time.time()-t0:.0f}s)", flush=True)

json.dump(results, open(os.path.join(SP, "fid_final.json"), "w"), indent=2, default=float)
shutil.rmtree(SP, ignore_errors=True) if False else None
print("\n=== FINAL FIDELITY TABLE (target/release/saga, seed 42) ===")
print(f"{'domain':10s} {'real':16s} {'edges':>9s} {'fid':>5s}  {'B r/s':>10s}  {'repeat r/s':>11s}")
for r in results:
    print(f"{r['domain']:10s} {r['real']:16s} {r['real_edges']:>9d} {r['fidelity']:>5.3f}  "
          f"{r['real_B']:.2f}/{r['saga_B']:.2f}  {r['real_repeat']:.2f}/{r['saga_repeat']:.2f}")
