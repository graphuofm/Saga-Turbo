#!/usr/bin/env python3
"""JOB 1b: tab:downmulti re-measured on the FIXED engine.

Byte-for-byte the protocol of scratchpad/j6_down.py (which produced the pre-fix
before_downstream.json), the ONLY change being BIN -> the fixed worktree binary.
2M-edge --no-anomalies seed-42 runs, time-prefixed to the first 200k events;
TGN 6 epochs CPU; EdgeBank; BA size-matched to SAGA's (n, m, span).
Writes incrementally so partial results are still real measurements.
"""
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np

SP = "./scratch"
sys.path.insert(0, os.path.join(SP, "pin"))
from saga_eval.baselines import gen_ba  # noqa: E402
from saga_eval.downstream_extra import run_edgebank  # noqa: E402
from saga_eval.load import load_saga  # noqa: E402
from saga_eval.tgn_eval import run_tgn  # noqa: E402

BIN = "./target/release/saga"
WORK = os.path.join(SP, "work_down_after")
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
K = 200_000
os.makedirs(WORK, exist_ok=True)


def prefix(s, d, t, k):
    if s.size <= k:
        return s, d, t
    order = np.argsort(t, kind="stable")[:k]
    order = np.sort(order)
    return s[order], d[order], t[order]


results = []
for dom in DOMAINS:
    t0 = time.time()
    dd = os.path.join(WORK, dom)
    row = dict(domain=dom)
    try:
        shutil.rmtree(dd, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", dom, "--edges", "2000000",
               "--no-anomalies", "--seed", "42", "--out", dd, "--quiet"]
        row["cmd"] = " ".join(cmd)
        subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=1800)
        s, d, t = load_saga(dd)
        shutil.rmtree(dd, ignore_errors=True)
        s, d, t = prefix(s, d, t, K)
        n = int(np.unique(np.concatenate([s, d])).size)
        m = int(s.size)
        span = float(t.max() - t.min())
        tg = run_tgn(s, d, t, epochs=6, seed=0)
        eb = run_edgebank(s, d, t, seed=0)
        bs, bd, bt = gen_ba(n, m, span, 42)
        tg_ba = run_tgn(bs, bd, bt, epochs=6, seed=0)
        eb_ba = run_edgebank(bs, bd, bt, seed=0)
        row.update(n_nodes=n, n_edges=m,
                   tgn_saga=round(tg["test_ap"], 3), tgn_ba=round(tg_ba["test_ap"], 3),
                   eb_saga=round(eb["inf_test_ap"], 3), eb_ba=round(eb_ba["inf_test_ap"], 3),
                   ebtw_saga=round(eb["tw_test_ap"], 3),
                   status="OK")
    except Exception as e:  # noqa: BLE001
        row["status"] = f"FAILED: {type(e).__name__}: {e}"
    finally:
        shutil.rmtree(dd, ignore_errors=True)
    row["wall_s"] = round(time.time() - t0, 1)
    results.append(row)
    print(f"[{dom}] {row['status']} TGN saga={row.get('tgn_saga')} ba={row.get('tgn_ba')} | "
          f"EB saga={row.get('eb_saga')} ba={row.get('eb_ba')}  {row['wall_s']}s", flush=True)
    json.dump(results, open(os.path.join(SP, "downmulti_after.json"), "w"), indent=1, default=float)

print("DONE job1b")
