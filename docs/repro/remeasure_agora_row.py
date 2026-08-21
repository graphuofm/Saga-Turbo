#!/usr/bin/env python3
"""Re-measure ONLY the AGORA row of the 10x6 fidelity matrix on the FIXED engine.
Byte-for-byte the SAGA-row logic of scratchpad/matrix_driver.py / job2_matrix_after.py:
same CAP, same real refs, same cap_t, same generate command, same fidelity(compare).
Only BIN (current fixed-engine agora) and the output path change. Baselines are NOT
re-run: they are networkx/numpy/scipy graph models that never touch the engine."""
import sys, os, json, time, subprocess, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare

HOME = os.path.expanduser("~")
SP = "./scratch/mat_fixed"
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

FROZEN = {"finance":.801,"crypto":.843,"cyber":.807,"ecommerce":.861,"transport":.696,"healthcare":.751}
matrix = {}
for domain, rname, path, kw, proxy in PAIRS:
    t0 = time.time()
    rs, rd, rt = cap_t(*load_real(path, **kw), CAP)
    n = int(np.unique(np.concatenate([rs, rd])).size)
    m = int(rs.size)
    span = float(rt.max() - rt.min())
    rstat = compute_stats(rs, rd, rt)
    outdir = os.path.join(SP, f"{domain}")
    shutil.rmtree(outdir, ignore_errors=True)
    subprocess.run([BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
                    "--time-span", str(round(span/86400.0, 2)), "--seed", "42",
                    "--anomaly-rate", "0.02", "--out", outdir, "--quiet"],
                   check=True, stderr=subprocess.DEVNULL)
    saga = fid(rstat, cap_t(*load_saga(outdir), CAP))
    shutil.rmtree(outdir, ignore_errors=True)
    matrix[domain] = {"_real": rname, "_proxy": proxy, "_edges": m, "_nodes": n, "SAGA": saga}
    json.dump(matrix, open(os.path.join(SP, "..", "matrix_fixed_agora.json"), "w"), indent=2)
    fz = FROZEN[domain]
    print(f"[{domain}/{rname}{' PROXY' if proxy else ''}] n={n} m={m} span_d={round(span/86400.0,2)} "
          f"SAGA_fixed={saga} frozen={fz} delta={round(saga-fz,3):+} ({time.time()-t0:.0f}s)", flush=True)

shutil.rmtree(SP, ignore_errors=True)
print("\n=== AGORA ROW: fixed vs frozen ===")
print(f"{'domain':11s}{'frozen':>8s}{'fixed':>8s}{'delta':>8s}")
for d,*_ in PAIRS:
    print(f"{d:11s}{FROZEN[d]:>8.3f}{matrix[d]['SAGA']:>8.3f}{matrix[d]['SAGA']-FROZEN[d]:>+8.3f}")
print("\nsaved matrix_fixed_agora.json; generated dirs deleted")
