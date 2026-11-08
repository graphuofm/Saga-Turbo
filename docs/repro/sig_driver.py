#!/usr/bin/env python3
"""Structural-signature battery across all six SAGA domains (one real run each)."""
import sys, os, glob, json, time
sys.path.insert(0, "./python")
import numpy as np
import pyarrow.parquet as pq
from saga_eval.load import load_saga
from saga_eval.stats import compute_stats

SP = "$WORK/ds"
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
BASE_COLS = {"src", "dst", "t", "event_type", "label", "anomaly_id"}

rows = []
for d in DOMAINS:
    dd = os.path.join(SP, d)
    t0 = time.time()
    src, dst, t = load_saga(dd)
    st = compute_stats(src, dst, t, clustering_sample=3000, seed=0)
    # anomaly edge rate + edge/attr typing from parquet
    ef = sorted(glob.glob(os.path.join(dd, "edges_*.parquet")))
    tbl = pq.read_table(ef[0])
    cols = set(tbl.column_names)
    labels = tbl.column("label").to_numpy(zero_copy_only=False)
    anom_rate = float(np.mean(labels != "normal"))
    n_evt = int(np.unique(tbl.column("event_type").to_numpy(zero_copy_only=False)).size)
    n_attr = len(cols - BASE_COLS)
    n_node_types = len(glob.glob(os.path.join(dd, "nodes_*.parquet")))
    span_days = (st.t_max - st.t_min) / 86400.0
    rows.append(dict(
        domain=d, n_nodes=st.n_nodes, n_edges=st.n_edges,
        mean_deg=round(st.mean_degree, 2),
        density=st.density,
        clustering=round(st.clustering, 4) if st.clustering is not None else None,
        reciprocity=round(st.reciprocity, 4),
        burstiness_b=round(st.burstiness_b, 3) if st.burstiness_b is not None else None,
        memory_m=round(st.memory_m, 3) if st.memory_m is not None else None,
        powerlaw_alpha=round(st.powerlaw_alpha, 2) if st.powerlaw_alpha else None,
        repeat_edge=round(st.repeat_edge_ratio, 3),
        span_days=round(span_days, 1),
        n_node_types=n_node_types, n_edge_types=n_evt, n_attributes=n_attr,
        anomaly_edge_rate=round(anom_rate, 4),
    ))
    print(f"[{d}] done in {time.time()-t0:.1f}s  n={st.n_nodes} e={st.n_edges} "
          f"deg={st.mean_degree:.1f} clus={st.clustering} B={st.burstiness_b} "
          f"a={st.powerlaw_alpha} rep={st.repeat_edge_ratio:.2f} anom={anom_rate:.3f}", flush=True)

out = os.path.join(SP, "..", "signatures.json")
json.dump(rows, open(out, "w"), indent=2)
print("\n=== MARKDOWN TABLE ===")
hdr = ["domain","n_nodes","n_edges","mean_deg","clustering","reciprocity","burstiness_b",
       "memory_m","powerlaw_alpha","repeat_edge","span_days","n_node_types","n_edge_types",
       "n_attributes","anomaly_edge_rate"]
print("| " + " | ".join(hdr) + " |")
print("|" + "|".join(["---"]*len(hdr)) + "|")
for r in rows:
    print("| " + " | ".join(str(r[h]) for h in hdr) + " |")
print("\nsaved:", os.path.abspath(out))
