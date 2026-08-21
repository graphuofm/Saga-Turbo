#!/usr/bin/env python3
"""Learned-generator contest: TIGGER-Lite (deep temporal SOTA), CTGAN, TVAE vs SAGA
and two references (BA, config), on each domain. Learned generators do NOT scale
(TIGGER is transductive), so each real graph is capped to ~2500 nodes — a controlled
size where every method can run. This IS the finding: SAGA + structural generators
run at real scale (the big matrix); learned generators only at toy scale, here."""
import sys, os, json, time, subprocess, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
from saga_eval import baselines as bl
from saga_eval.deep_gen import fit_generate
from saga_eval.tabular_gen import ctgan_generate, tvae_generate

HOME = os.path.expanduser("~")
SP = "./scratch/lrn"
BIN = "./target/release/saga"
NODE_CAP = 2500
os.makedirs(SP, exist_ok=True)

PAIRS = [
    ("finance",   "email-Eu",    f"./realdata/snap-email-eu/email-Eu-core-temporal.txt", {}),
    ("crypto",    "wiki-talk",   f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}),
    ("cyber",     "sx-superuser",f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}),
    ("ecommerce", "tgbl-review", f"./realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
                    dict(src_col="source", dst_col="target", t_col="ts")),
    ("transport", "sx-askubuntu",f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}),
    ("healthcare","sx-mathoverflow",f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}),
]

def cap_by_nodes(s, d, t, cap):
    order = np.argsort(t, kind="stable")
    s, d, t = s[order], d[order], t[order]
    both = np.empty(2 * s.size, dtype=np.int64)
    both[0::2] = s; both[1::2] = d
    _, first = np.unique(both, return_index=True)
    is_new = np.zeros(both.size, bool); is_new[first] = True
    edge_nodes = np.cumsum(is_new)[1::2]
    cut = min(int(np.searchsorted(edge_nodes, cap)) + 1, s.size)
    return s[:cut], d[:cut], t[:cut]

def fscore(rstat, arr):
    s, d, t = arr
    if s is None or len(s) == 0:
        return float("nan")
    try:
        return round(compare(rstat, compute_stats(s, d, t))["fidelity_score"], 3)
    except Exception as e:
        print("   fscore err", e); return float("nan")

matrix = {}
for domain, rname, path, kw in PAIRS:
    t0 = time.time()
    rs, rd, rt = cap_by_nodes(*load_real(path, **kw), NODE_CAP)
    real = (rs, rd, rt)
    n = int(np.unique(np.concatenate([rs, rd])).size)
    m = int(rs.size)
    span = float(rt.max() - rt.min())
    rstat = compute_stats(rs, rd, rt)
    row = {"_real": rname, "_nodes": n, "_edges": m}
    # SAGA matched
    outdir = os.path.join(SP, domain)
    shutil.rmtree(outdir, ignore_errors=True)
    subprocess.run([BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
                    "--time-span", str(round(span/86400.0, 3)), "--seed", "42",
                    "--anomaly-rate", "0.02", "--out", outdir, "--quiet"],
                   check=True, stderr=subprocess.DEVNULL)
    row["SAGA"] = fscore(rstat, cap_by_nodes(*load_saga(outdir), 10**9))
    shutil.rmtree(outdir, ignore_errors=True)
    # learned
    for name, fn in [("TIGGER", fit_generate), ("CTGAN", ctgan_generate), ("TVAE", tvae_generate)]:
        try:
            tt = time.time()
            arr = fn(rs, rd, rt, n, m, span, 42)
            row[name] = fscore(rstat, arr)
            row[name + "_s"] = round(time.time() - tt, 0)
        except Exception as e:
            print(f"   {name} err {type(e).__name__}: {e}"); row[name] = float("nan")
    # references
    row["BA"] = fscore(rstat, bl.gen_ba(n, m, span, 42))
    row["config"] = fscore(rstat, bl.gen_config(real, span, 42))
    matrix[domain] = row
    json.dump(matrix, open(os.path.join(SP, "..", "learned_matrix.json"), "w"), indent=2, default=float)
    print(f"[{domain}] n={n} m={m}  SAGA={row['SAGA']} TIGGER={row.get('TIGGER')} "
          f"CTGAN={row.get('CTGAN')} TVAE={row.get('TVAE')} BA={row['BA']} config={row['config']} "
          f"({time.time()-t0:.0f}s)", flush=True)

print("\n=== LEARNED-GENERATOR MATRIX (fid, ~2500-node cap) ===")
ORDER = ["SAGA", "TIGGER", "CTGAN", "TVAE", "config", "BA"]
print(f"{'method':10s}" + "".join(f"{d[:7]:>9s}" for d, *_ in PAIRS))
for g in ORDER:
    print(f"{g:10s}" + "".join(f"{matrix[d].get(g, float('nan')):>9.3f}" for d, *_ in PAIRS))
print("saved learned_matrix.json")
