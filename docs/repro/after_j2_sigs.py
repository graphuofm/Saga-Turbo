#!/usr/bin/env python3
"""JOB 2 (AFTER-fix): tab:multidomain structural signatures.
Exact replica of before_snapshot/j3_sigs.py; ONLY the binary path changed."""
import glob
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np
import pyarrow.parquet as pq

SP = "$WORK"
sys.path.insert(0, "./python")
from saga_eval.load import load_saga  # noqa: E402
from saga_eval.stats import compute_stats  # noqa: E402

BIN = "./target/release/saga"
WORK = os.path.join(SP, "work_sig_after")
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
BASE_COLS = {"src", "dst", "t", "event_type", "label", "anomaly_id"}
os.makedirs(WORK, exist_ok=True)

rows = []
for d in DOMAINS:
    t0 = time.time()
    dd = os.path.join(WORK, d)
    shutil.rmtree(dd, ignore_errors=True)
    cmd = [BIN, "generate", "--domain", d, "--edges", "2000000",
           "--no-anomalies", "--seed", "42", "--out", dd, "--quiet"]
    row = dict(domain=d, cmd=" ".join(cmd))
    try:
        subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=1800)
        src, dst, t = load_saga(dd)
        st = compute_stats(src, dst, t, clustering_sample=3000, seed=0)
        ef = sorted(glob.glob(os.path.join(dd, "edges_*.parquet")))
        tbl = pq.read_table(ef[0])
        cols = set(tbl.column_names)
        n_evt = int(np.unique(tbl.column("event_type").to_numpy(zero_copy_only=False)).size)
        n_attr = len(cols - BASE_COLS)
        n_node_types = len(glob.glob(os.path.join(dd, "nodes_*.parquet")))
        row.update(
            n_nodes=st.n_nodes, n_edges=st.n_edges,
            mean_deg=round(st.mean_degree, 2),
            clustering=round(st.clustering, 4) if st.clustering is not None else None,
            reciprocity=round(st.reciprocity, 4),
            burstiness_b=round(st.burstiness_b, 3) if st.burstiness_b is not None else None,
            memory_m=round(st.memory_m, 3) if st.memory_m is not None else None,
            powerlaw_alpha=round(st.powerlaw_alpha, 2) if st.powerlaw_alpha else None,
            repeat_edge=round(st.repeat_edge_ratio, 3),
            span_days=round((st.t_max - st.t_min) / 86400.0, 1),
            n_node_types=n_node_types, n_edge_types=n_evt, n_attributes=n_attr,
            status="OK",
        )
    except Exception as e:  # noqa: BLE001
        row["status"] = f"FAILED: {type(e).__name__}: {e}"
    finally:
        shutil.rmtree(dd, ignore_errors=True)
    row["wall_s"] = round(time.time() - t0, 1)
    rows.append(row)
    print(f"[{d}] {row['status']} n={row.get('n_nodes')} deg={row.get('mean_deg')} "
          f"clus={row.get('clustering')} recip={row.get('reciprocity')} B={row.get('burstiness_b')} "
          f"a={row.get('powerlaw_alpha')} rep={row.get('repeat_edge')} {row['wall_s']}s", flush=True)
    json.dump(rows, open(os.path.join(SP, "signatures_after.json"), "w"), indent=1, default=float)

shutil.rmtree(WORK, ignore_errors=True)
print("DONE job2")
