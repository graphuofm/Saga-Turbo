#!/usr/bin/env python3
"""JOB 1b (CLEAN re-measurement): tab:downmulti on the FIXED engine, quiet host.

Same protocol as after_downmulti.py; the only changes vs that file are:
  - BIN -> ./target/release/saga (the fixed-engine binary named in the task).
  - per-domain SIGALRM sanity cap (CAP_S) so a stuck domain is reported+skipped
    rather than silently running ~11h (which is what contention did last time).
  - per-step wall-clock logging (gen / load / tgn_saga / eb / ba_gen / tgn_ba / eb_ba).
2M-edge --no-anomalies seed-42 runs, time-prefixed to first 200k events;
TGN 6 epochs CPU; EdgeBank; BA size-matched to SAGA's (n, m, span).
Writes incrementally so partial results are still real measurements.
"""
import json
import os
import shutil
import signal
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
CAP_S = 1800  # per-domain sanity cap; calibrated quiet-host cost is ~5 min/domain
os.makedirs(WORK, exist_ok=True)


class DomainTimeout(Exception):
    pass


def _alarm(signum, frame):
    raise DomainTimeout(f"exceeded per-domain cap of {CAP_S}s")


signal.signal(signal.SIGALRM, _alarm)


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
    steps = {}
    signal.alarm(CAP_S)
    try:
        shutil.rmtree(dd, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", dom, "--edges", "2000000",
               "--no-anomalies", "--seed", "42", "--out", dd, "--quiet"]
        row["cmd"] = " ".join(cmd)
        ts = time.time(); subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=1800); steps["gen"] = round(time.time() - ts, 1)
        ts = time.time(); s, d, t = load_saga(dd); steps["load"] = round(time.time() - ts, 1)
        shutil.rmtree(dd, ignore_errors=True)
        s, d, t = prefix(s, d, t, K)
        n = int(np.unique(np.concatenate([s, d])).size)
        m = int(s.size)
        span = float(t.max() - t.min())
        ts = time.time(); tg = run_tgn(s, d, t, epochs=6, seed=0); steps["tgn_saga"] = round(time.time() - ts, 1)
        ts = time.time(); eb = run_edgebank(s, d, t, seed=0); steps["eb_saga"] = round(time.time() - ts, 1)
        ts = time.time(); bs, bd, bt = gen_ba(n, m, span, 42); steps["ba_gen"] = round(time.time() - ts, 1)
        ts = time.time(); tg_ba = run_tgn(bs, bd, bt, epochs=6, seed=0); steps["tgn_ba"] = round(time.time() - ts, 1)
        ts = time.time(); eb_ba = run_edgebank(bs, bd, bt, seed=0); steps["eb_ba"] = round(time.time() - ts, 1)
        row.update(n_nodes=n, n_edges=m,
                   tgn_saga=round(tg["test_ap"], 3), tgn_ba=round(tg_ba["test_ap"], 3),
                   eb_saga=round(eb["inf_test_ap"], 3), eb_ba=round(eb_ba["inf_test_ap"], 3),
                   ebtw_saga=round(eb["tw_test_ap"], 3),
                   status="OK")
    except DomainTimeout as e:  # noqa: BLE001
        row["status"] = f"TIMEOUT: {e}"
    except Exception as e:  # noqa: BLE001
        row["status"] = f"FAILED: {type(e).__name__}: {e}"
    finally:
        signal.alarm(0)
        shutil.rmtree(dd, ignore_errors=True)
    row["steps_s"] = steps
    row["wall_s"] = round(time.time() - t0, 1)
    results.append(row)
    print(f"[{dom}] {row['status']} TGN saga={row.get('tgn_saga')} ba={row.get('tgn_ba')} | "
          f"EB saga={row.get('eb_saga')} ba={row.get('eb_ba')}  wall={row['wall_s']}s steps={steps}", flush=True)
    json.dump(results, open(os.path.join(SP, "downmulti_clean.json"), "w"), indent=1, default=float)

print("DONE job1b clean")
