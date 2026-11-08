#!/usr/bin/env python3
"""JOB 1a: tab:downstream re-measured on the FIXED engine.

real CollegeMsg vs SAGA vs BA(random timestamps); TGN + EdgeBank future link
prediction test AP, plus the recurring-edge fraction.

The exact generate command behind the published /tmp/sagatgn_A is not recorded
anywhere in the repo, so we CANNOT replay it byte-for-byte. Instead we run the
documented domain-matched convention (identical to fidelity_driver.py /
j4_fid.py: finance matched to CollegeMsg's n/m/span, seed 42, pi=0.02) through
BOTH binaries -- the pre-fix baseline at /tmp/saga_base and the fixed worktree
binary -- so the before/after delta is a controlled comparison under one config.
Real + BA are re-measured too (they are binary-independent; they should
reproduce the published 0.843 / 0.611).
"""
import collections
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np

SP = "$WORK"
sys.path.insert(0, os.path.join(SP, "pin"))  # pinned harness == live harness (diff -rq clean)
from saga_eval.baselines import gen_ba  # noqa: E402
from saga_eval.downstream_extra import run_edgebank  # noqa: E402
from saga_eval.load import load_real, load_saga  # noqa: E402
from saga_eval.tgn_eval import run_tgn  # noqa: E402

BIN_FIXED = "./target/release/saga"
BIN_BASE = "/tmp/saga_base/target/release/saga"
REAL = "./realdata/snap-collegemsg/CollegeMsg.txt"
WORK = os.path.join(SP, "work_college")
os.makedirs(WORK, exist_ok=True)


def recurring_frac(s, d):
    c = collections.Counter(zip(s.tolist(), d.tolist()))
    return float(sum(v for v in c.values() if v >= 2) / s.size)


def measure(name, s, d, t, cmd=None):
    t0 = time.time()
    row = dict(graph=name, n_nodes=int(np.unique(np.concatenate([s, d])).size),
               n_edges=int(s.size), recurring_edge_frac=round(recurring_frac(s, d), 4))
    if cmd:
        row["cmd"] = cmd
    try:
        tg = run_tgn(s, d, t, epochs=6, seed=0)
        eb = run_edgebank(s, d, t, seed=0)
        row.update(tgn_ap=round(tg["test_ap"], 3), tgn_auc=round(tg["test_auc"], 3),
                   eb_inf_ap=round(eb["inf_test_ap"], 3), eb_inf_auc=round(eb["inf_test_auc"], 3),
                   eb_tw_ap=round(eb["tw_test_ap"], 3), status="OK")
    except Exception as e:  # noqa: BLE001
        row["status"] = f"FAILED: {type(e).__name__}: {e}"
    row["wall_s"] = round(time.time() - t0, 1)
    print(f"[{name}] {row['status']} TGN_AP={row.get('tgn_ap')} EB_AP={row.get('eb_inf_ap')} "
          f"rep={row['recurring_edge_frac']} ({row['wall_s']}s)", flush=True)
    return row


rows = []
rs, rd, rt = load_real(REAL)
n = int(np.unique(np.concatenate([rs, rd])).size)
m = int(rs.size)
span_days = max(1.0, (rt.max() - rt.min()) / 86400.0)
print(f"CollegeMsg: n={n} m={m} span_days={span_days:.2f}", flush=True)

rows.append(measure("real_CollegeMsg", rs, rd, rt))
json.dump(rows, open(os.path.join(SP, "downstream_after.json"), "w"), indent=1, default=float)

for tag, binary in (("SAGA_prefix", BIN_BASE), ("SAGA_fixed", BIN_FIXED)):
    out = os.path.join(WORK, tag)
    shutil.rmtree(out, ignore_errors=True)
    cmd = [binary, "generate", "--domain", "finance", "--nodes", str(n), "--edges", str(m),
           "--time-span", str(round(span_days, 2)), "--seed", "42",
           "--anomaly-rate", "0.02", "--out", out, "--quiet"]
    try:
        subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=1800)
        ss, sd, st = load_saga(out)
        rows.append(measure(tag, ss, sd, st, cmd=" ".join(cmd)))
    except Exception as e:  # noqa: BLE001
        rows.append(dict(graph=tag, cmd=" ".join(cmd),
                         status=f"FAILED: {type(e).__name__}: {e}"))
        print(f"[{tag}] {rows[-1]['status']}", flush=True)
    finally:
        shutil.rmtree(out, ignore_errors=True)
    json.dump(rows, open(os.path.join(SP, "downstream_after.json"), "w"), indent=1, default=float)

bs, bd, bt = gen_ba(n, m, float(rt.max() - rt.min()), 42)
rows.append(measure("BA_random_ts", bs, bd, bt, cmd="gen_ba(n,m,span,seed=42)"))
json.dump(rows, open(os.path.join(SP, "downstream_after.json"), "w"), indent=1, default=float)
print("DONE job1a")
