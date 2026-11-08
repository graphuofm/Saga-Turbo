#!/usr/bin/env python3
"""JOB 1 re-measure: tab:baselines vs CollegeMsg on the FIXED engine.
Reuses saga_eval.baselines.compare_baselines verbatim (same generators, same
matching, same seed) — only the SAGA binary changes."""
import sys, os, json, time, subprocess, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval import baselines as bl

SP = "$WORK"
BIN = "./target/release/saga"
REAL = "./realdata/snap-collegemsg/CollegeMsg.txt"

rs, rd, rt = load_real(REAL)
n = int(np.unique(np.concatenate([rs, rd])).size)
m = int(rs.size)
span_days = (rt.max() - rt.min()) / 86400.0
print(f"real CollegeMsg: n={n} m={m} span={span_days:.2f}d", flush=True)

out = {"_real": "CollegeMsg", "_real_nodes": n, "_real_edges": m,
       "_span_days": round(span_days, 2), "_bin": BIN}


def saga_run(tag, extra):
    d = os.path.join(SP, f"j1_{tag}")
    shutil.rmtree(d, ignore_errors=True)
    cmd = [BIN, "generate", "--domain", "finance", "--nodes", str(n), "--edges", str(m),
           "--time-span", str(round(span_days, 2)), "--seed", "42",
           "--out", d, "--quiet"] + extra
    t0 = time.time()
    subprocess.run(cmd, check=True, stderr=subprocess.DEVNULL)
    arr = load_saga(d)
    shutil.rmtree(d, ignore_errors=True)
    print(f"  saga[{tag}] {arr[0].size} edges in {time.time()-t0:.0f}s", flush=True)
    return arr


# Primary protocol, exactly as docs/BASELINES.md states: anomalies OFF, seed 42.
saga = saga_run("noanom", ["--no-anomalies"])
rows = bl.compare_baselines((rs, rd, rt), saga, seed=42)
print(bl.format_table(rows, "CollegeMsg"), flush=True)
out["rows"] = rows

# Sensitivity check: the same table's SAGA row with anomalies ON at 2% (the rate
# whose semantics the fix changed).
saga2 = saga_run("anom02", ["--anomaly-rate", "0.02"])
real_stats = bl.st.compute_stats(rs, rd, rt)
out["saga_anom02"] = bl._row("SAGA (anom 2%)", saga2, real_stats, True, (rs, rd, rt))
print("SAGA with --anomaly-rate 0.02:", json.dumps(
    {k: out["saga_anom02"][k] for k in
     ("edges", "clustering", "reciprocity", "burstiness", "alpha", "fidelity")}, default=float))

json.dump(out, open(os.path.join(SP, "baselines_after.json"), "w"), indent=2, default=float)
print("saved baselines_after.json")
