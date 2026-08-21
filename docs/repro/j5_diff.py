#!/usr/bin/env python3
"""BEFORE-baseline item 5: tab:diff — detector ROC-AUC by feature view vs delta.
Config per docs/EXPERIMENTS.md section 6 (the run the PAPER's tab:diff reports):
finance, 200k edges, --anomaly-rate 0.05, seed 11, delta in {0,.25,.5,.75,1}.
detector.py needs edges*.csv -> --format csv."""
import json
import os
import shutil
import subprocess
import sys
import time

SP = "./scratch"
sys.path.insert(0, os.path.join(SP, "pin"))
from saga_eval.detector import detect_auc  # noqa: E402

BIN = "/tmp/saga_base/target/release/saga"
WORK = os.path.join(SP, "work_diff")
DELTAS = [0.0, 0.25, 0.5, 0.75, 1.0]
os.makedirs(WORK, exist_ok=True)

rows = []
for delta in DELTAS:
    t0 = time.time()
    out = os.path.join(WORK, f"d{delta}")
    shutil.rmtree(out, ignore_errors=True)
    cmd = [BIN, "generate", "--domain", "finance", "--edges", "200000",
           "--anomaly-rate", "0.05", "--difficulty", str(delta), "--seed", "11",
           "--format", "csv", "--out", out, "--quiet"]
    row = dict(delta=delta, cmd=" ".join(cmd))
    try:
        subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=1800)
        for view in ("all", "attr", "struct"):
            r = detect_auc(out, seed=0, feature_set=view)
            if r is None:
                row[view] = "FAILED: sklearn unavailable"
            else:
                row[f"auc_{view}"] = round(r["roc_auc"], 4)
                row[f"ap_{view}"] = round(r["ap"], 4)
                row["n_anom"] = r["n_anom"]
                row["n_total"] = r["n_total"]
        row["status"] = "OK"
    except Exception as e:  # noqa: BLE001
        row["status"] = f"FAILED: {type(e).__name__}: {e}"
    finally:
        shutil.rmtree(out, ignore_errors=True)
    row["wall_s"] = round(time.time() - t0, 1)
    rows.append(row)
    print(f"[delta={delta}] {row['status']} all={row.get('auc_all')} "
          f"attr={row.get('auc_attr')} struct={row.get('auc_struct')} "
          f"n_anom={row.get('n_anom')}/{row.get('n_total')} {row['wall_s']}s", flush=True)
    json.dump(rows, open(os.path.join(SP, "before_diff.json"), "w"), indent=1, default=float)

print("DONE item5")
