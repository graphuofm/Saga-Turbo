#!/usr/bin/env python3
"""BEFORE-baseline item 4: tab:fidelity, SAGA vs 7 real temporal graphs.
Exact replica of scratchpad/fidelity_driver.py (the driver behind
docs/EXPERIMENTS.md section 9 / paper tab:fidelity) but pinned to the
baseline binary at /tmp/saga_base.

Records reciprocity and repeat-rate SEPARATELY and under correct names:
the paper's tab:fidelity column labelled "recip. r/s" actually carries the
REPEAT RATE (verified against scratchpad/fidelity_multi.json)."""
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np

SP = "./scratch"
sys.path.insert(0, os.path.join(SP, "pin"))
from saga_eval.compare import compare  # noqa: E402
from saga_eval.load import load_real, load_saga  # noqa: E402
from saga_eval.stats import compute_stats  # noqa: E402

HOME = os.path.expanduser("~")
BIN = "/tmp/saga_base/target/release/saga"
WORK = os.path.join(SP, "work_fid")
CAP = 2_500_000
os.makedirs(WORK, exist_ok=True)

PAIRS = [
    ("finance", "email-Eu", f"./realdata/snap-email-eu/email-Eu-core-temporal.txt", {}),
    ("finance", "CollegeMsg", f"./realdata/snap-collegemsg/CollegeMsg.txt", {}),
    ("crypto", "wiki-talk", f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}),
    ("crypto", "sx-mathoverflow", f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}),
    ("cyber", "sx-superuser", f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}),
    ("cyber", "sx-askubuntu", f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}),
    ("ecommerce", "tgbl-review", f"./realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
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
    row = dict(domain=domain, real=rname, real_path=path)
    outdir = os.path.join(WORK, f"{domain}_{rname}")
    try:
        if not os.path.exists(path):
            raise FileNotFoundError(path)
        rs, rd, rt = load_real(path, **kw)
        rs, rd, rt = cap_by_time(rs, rd, rt, CAP)
        n = int(np.unique(np.concatenate([rs, rd])).size)
        m = int(rs.size)
        span_days = max(1.0, (rt.max() - rt.min()) / 86400.0)
        shutil.rmtree(outdir, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
               "--time-span", str(round(span_days, 2)), "--seed", "42",
               "--anomaly-rate", "0.02", "--out", outdir, "--quiet"]
        row["cmd"] = " ".join(cmd)
        subprocess.run(cmd, check=True, stderr=subprocess.DEVNULL, timeout=3600)
        ss, sd, st_ = load_saga(outdir)
        ss, sd, st_ = cap_by_time(ss, sd, st_, CAP)
        real_stats = compute_stats(rs, rd, rt, seed=0)
        saga_stats = compute_stats(ss, sd, st_, seed=0)
        cmp = compare(real_stats, saga_stats)
        row.update(
            real_edges=m, real_nodes=n, span_days=round(span_days, 1),
            fidelity=round(cmp["fidelity_score"], 3),
            ks=cmp["ks_distances"],
            real_B=real_stats.burstiness_b, saga_B=saga_stats.burstiness_b,
            # NOTE: reciprocity and repeat rate kept explicitly separate.
            real_reciprocity=real_stats.reciprocity, saga_reciprocity=saga_stats.reciprocity,
            real_repeat_rate=real_stats.repeat_edge_ratio, saga_repeat_rate=saga_stats.repeat_edge_ratio,
            real_alpha=real_stats.powerlaw_alpha, saga_alpha=saga_stats.powerlaw_alpha,
            real_clustering=real_stats.clustering, saga_clustering=saga_stats.clustering,
            real_mean_deg=real_stats.mean_degree, saga_mean_deg=saga_stats.mean_degree,
            status="OK",
        )
    except Exception as e:  # noqa: BLE001
        row["status"] = f"FAILED: {type(e).__name__}: {e}"
    finally:
        shutil.rmtree(outdir, ignore_errors=True)
    row["wall_s"] = round(time.time() - t0, 1)
    results.append(row)
    print(f"[{domain}/{rname}] {row['status']} fid={row.get('fidelity')} "
          f"B={row.get('real_B')}/{row.get('saga_B')} "
          f"recip={row.get('real_reciprocity')}/{row.get('saga_reciprocity')} "
          f"repeat={row.get('real_repeat_rate')}/{row.get('saga_repeat_rate')} {row['wall_s']}s", flush=True)
    json.dump(results, open(os.path.join(SP, "before_fidelity.json"), "w"), indent=1, default=float)

print("DONE item4")
