#!/usr/bin/env python3
"""JOB 2 re-measure: 10 generators x 6 domains fidelity matrix on the FIXED engine.
Byte-for-byte the logic of scratchpad/matrix_driver.py — only BIN and the output
path change, so the comparison is like-for-like."""
import sys, os, json, time, subprocess, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
from saga_eval import baselines as bl

HOME = os.path.expanduser("~")
SP = "./scratch/mat_after"
BIN = "./target/release/saga"
CAP = 1_500_000
os.makedirs(SP, exist_ok=True)

PAIRS = [
    ("finance",   "email-Eu",    f"./realdata/snap-email-eu/email-Eu-core-temporal.txt", {}, False),
    ("crypto",    "wiki-talk",   f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}, False),
    ("cyber",     "sx-superuser",f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}, False),
    ("ecommerce", "tgbl-review", f"./realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
                    dict(src_col="source", dst_col="target", t_col="ts"), False),
    ("transport", "sx-askubuntu",f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}, True),
    ("healthcare","sx-mathoverflow",f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}, True),
]
GEN_PRIOR = [("ER", bl.gen_er), ("BA", bl.gen_ba), ("R-MAT", bl.gen_rmat),
             ("WS", bl.gen_ws), ("Kronecker", bl.gen_kronecker)]
GEN_FIT = [("config", bl.gen_config), ("Chung-Lu", bl.gen_chunglu),
           ("DC-SBM", bl.gen_dcsbm), ("RDPG", bl.gen_rdpg)]

def cap_t(s, d, t, cap):
    if s.size <= cap:
        return s, d, t
    order = np.sort(np.argsort(t, kind="stable")[:cap])
    return s[order], d[order], t[order]

def fid(rstat, arr):
    s, d, t = arr
    if s.size == 0:
        return float("nan")
    return round(compare(rstat, compute_stats(s, d, t))["fidelity_score"], 3)

matrix = {}
for domain, rname, path, kw, proxy in PAIRS:
    t0 = time.time()
    rs, rd, rt = cap_t(*load_real(path, **kw), CAP)
    real = (rs, rd, rt)
    n = int(np.unique(np.concatenate([rs, rd])).size)
    m = int(rs.size)
    span = float(rt.max() - rt.min())
    rstat = compute_stats(rs, rd, rt)
    row = {"_real": rname, "_proxy": proxy, "_edges": m, "_nodes": n}
    outdir = os.path.join(SP, f"{domain}")
    shutil.rmtree(outdir, ignore_errors=True)
    subprocess.run([BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
                    "--time-span", str(round(span/86400.0, 2)), "--seed", "42",
                    "--anomaly-rate", "0.02", "--out", outdir, "--quiet"],
                   check=True, stderr=subprocess.DEVNULL)
    row["SAGA"] = fid(rstat, cap_t(*load_saga(outdir), CAP))
    shutil.rmtree(outdir, ignore_errors=True)
    for name, fn in GEN_PRIOR:
        try: row[name] = fid(rstat, fn(n, m, span, 42))
        except Exception as e: row[name] = float("nan"); print(f"  {name} err {e}")
    for name, fn in GEN_FIT:
        try: row[name] = fid(rstat, fn(real, span, 42))
        except Exception as e: row[name] = float("nan"); print(f"  {name} err {e}")
    matrix[domain] = row
    json.dump(matrix, open(os.path.join(SP, "..", "matrix_after.json"), "w"), indent=2)
    gens = " ".join(f"{k}={row[k]}" for k in ["SAGA","config","RDPG","DC-SBM","Chung-Lu","Kronecker","R-MAT","BA","WS","ER"])
    print(f"[{domain}/{rname}{' PROXY' if proxy else ''}] {gens}  ({time.time()-t0:.0f}s)", flush=True)

ORDER = ["SAGA","config","RDPG","DC-SBM","Chung-Lu","Kronecker","R-MAT","BA","WS","ER"]
print("\n=== FIDELITY MATRIX (generator x domain) — AFTER FIX ===")
print(f"{'generator':12s}" + "".join(f"{d[:7]:>9s}" for d,*_ in PAIRS))
for g in ORDER:
    print(f"{g:12s}" + "".join(f"{matrix[d].get(g,float('nan')):>9.3f}" for d,*_ in PAIRS))
shutil.rmtree(SP, ignore_errors=True)
print("\nsaved matrix_after.json")
