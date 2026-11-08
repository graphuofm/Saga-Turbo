#!/usr/bin/env python3
"""Multi-domain fidelity vs real temporal graphs + SAGA-vs-classical-generators.
For each (domain, real dataset) pair: match SAGA to the real graph's (n, m, span),
compute the full statistic battery on both, report fidelity; on a few pairs also
run BA/ER/R-MAT/config/WS matched to the same real graph."""
import sys, os, json, time, subprocess, glob, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
from saga_eval.baselines import compare_baselines

HOME = os.path.expanduser("~")
SP = "$WORK/fid"
BIN = "./target/release/saga"
CAP = 2_500_000
os.makedirs(SP, exist_ok=True)

# (domain, real_name, path, kwargs for load_real, run_baselines?)
PAIRS = [
    ("finance",   "email-Eu",       f"{HOME}/saga-sigmod/realdata/snap-email-eu/email-Eu-core-temporal.txt", {}, True),
    ("finance",   "CollegeMsg",      f"{HOME}/saga-sigmod/realdata/snap-collegemsg/CollegeMsg.txt", {}, False),
    ("crypto",    "wiki-talk",       f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}, True),
    ("crypto",    "sx-mathoverflow", f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}, False),
    ("cyber",     "sx-superuser",    f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}, False),
    ("cyber",     "sx-askubuntu",    f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}, False),
    ("ecommerce", "tgbl-review",     f"{HOME}/saga-sigmod/realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
                    dict(src_col="source", dst_col="target", t_col="ts"), True),
]

def cap_by_time(s, d, t, cap):
    if s.size <= cap:
        return s, d, t
    order = np.argsort(t, kind="stable")[:cap]  # contiguous earliest window
    order = np.sort(order)
    return s[order], d[order], t[order]

results = []
for domain, rname, path, kw, do_base in PAIRS:
    t0 = time.time()
    rs, rd, rt = load_real(path, **kw)
    rs, rd, rt = cap_by_time(rs, rd, rt, CAP)
    n = int(np.unique(np.concatenate([rs, rd])).size)
    m = int(rs.size)
    span_days = max(1.0, (rt.max() - rt.min()) / 86400.0)
    # generate SAGA matched to (n, m, span)
    outdir = os.path.join(SP, f"{domain}_{rname}")
    shutil.rmtree(outdir, ignore_errors=True)
    cmd = [BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
           "--time-span", str(round(span_days, 2)), "--seed", "42",
           "--anomaly-rate", "0.02", "--out", outdir, "--quiet"]
    subprocess.run(cmd, check=True, stderr=subprocess.DEVNULL)
    ss, sd, st_ = load_saga(outdir)
    ss, sd, st_ = cap_by_time(ss, sd, st_, CAP)
    real_stats = compute_stats(rs, rd, rt, seed=0)
    saga_stats = compute_stats(ss, sd, st_, seed=0)
    cmp = compare(real_stats, saga_stats)
    fid = cmp["fidelity_score"]
    row = dict(domain=domain, real=rname, real_edges=m, real_nodes=n,
               span_days=round(span_days, 1), fidelity=round(fid, 3),
               ks=cmp["ks_distances"],
               real_B=real_stats.burstiness_b, saga_B=saga_stats.burstiness_b,
               real_recip=real_stats.reciprocity, saga_recip=saga_stats.reciprocity,
               real_alpha=real_stats.powerlaw_alpha, saga_alpha=saga_stats.powerlaw_alpha,
               real_rep=real_stats.repeat_edge_ratio, saga_rep=saga_stats.repeat_edge_ratio)
    # baselines on selected pairs
    if do_base:
        try:
            brows = compare_baselines((rs, rd, rt), (ss, sd, st_), seed=42)
            row["baselines"] = {r["name"]: round(r.get("fidelity", float("nan")), 3) for r in brows}
        except Exception as e:
            row["baselines_error"] = f"{type(e).__name__}: {e}"
    results.append(row)
    shutil.rmtree(outdir, ignore_errors=True)
    print(f"[{domain}/{rname}] fid={fid:.3f}  real(B={real_stats.burstiness_b:.2f},"
          f"rep={real_stats.repeat_edge_ratio:.2f}) saga(B={saga_stats.burstiness_b:.2f},"
          f"rep={saga_stats.repeat_edge_ratio:.2f})  {time.time()-t0:.0f}s"
          + (f"  base={row.get('baselines')}" if do_base else ""), flush=True)

json.dump(results, open(os.path.join(SP, "..", "fidelity_multi.json"), "w"), indent=2, default=float)
print("\n=== FIDELITY TABLE ===")
print(f"{'domain':10s} {'real':16s} {'edges':>9s} {'fid':>6s} {'realB/sagaB':>14s} {'realRep/sagaRep':>16s}")
for r in results:
    print(f"{r['domain']:10s} {r['real']:16s} {r['real_edges']:>9d} {r['fidelity']:>6.3f} "
          f"{r['real_B']:.2f}/{r['saga_B']:.2f}      {r['real_rep']:.2f}/{r['saga_rep']:.2f}")
print("\nsaved fidelity_multi.json")
