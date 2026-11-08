import subprocess, glob, os, shutil, collections, sys
import numpy as np
sys.path.insert(0, "./python")
from saga_eval import load
from saga_eval.stats import compute_stats

BIN = "./target/release/saga"
SCRATCH = "$WORK"
STRUCT_COLS = {"src", "dst", "t", "event_type", "label", "anomaly_id"}
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]

rows = []
for dom in DOMAINS:
    out = os.path.join(SCRATCH, f"gen_{dom}")
    shutil.rmtree(out, ignore_errors=True)
    r = subprocess.run([BIN, "generate", "--domain", dom, "--edges", "2000000",
                        "--no-anomalies", "--seed", "42", "--out", out, "--quiet"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print(f"FAIL {dom}: {r.stderr[-500:]}"); sys.exit(1)

    # schema metadata from the actual output
    import pyarrow.parquet as pq
    fs = sorted(glob.glob(os.path.join(out, "edges_*.parquet")))
    cols = pq.read_table(fs[0], columns=None).schema.names
    edge_attr_cols = [c for c in cols if c not in STRUCT_COLS]
    n_edge_attr = len(edge_attr_cols)
    n_node_types = len(glob.glob(os.path.join(out, "nodes_*.parquet")))
    ec = collections.Counter()
    for f in fs:
        tb = pq.read_table(f, columns=["event_type"])
        for x in tb["event_type"].to_pylist():
            ec[x] += 1
    n_edge_types = len(ec)

    # structural stats (same compute_stats as the paper harness, default args)
    s, d, t = load.load_saga(out)
    gs = compute_stats(s, d, t)
    rows.append(dict(
        domain=dom, n_nodes=gs.n_nodes, mean_degree=gs.mean_degree,
        clustering=gs.clustering, reciprocity=gs.reciprocity,
        burstiness=gs.burstiness_b, alpha=gs.powerlaw_alpha,
        memory=gs.memory_m, repeat=gs.repeat_edge_ratio,
        nt=n_node_types, et=n_edge_types, attr=n_edge_attr,
        ev=dict(ec),
    ))
    print(f"[done] {dom}: n={gs.n_nodes} mdeg={gs.mean_degree:.1f} clust={gs.clustering:.4f} "
          f"recip={gs.reciprocity:.4f} B={gs.burstiness_b:.4f} a={gs.powerlaw_alpha:.3f} "
          f"rep={gs.repeat_edge_ratio:.4f} typ={n_node_types}/{n_edge_types},{n_edge_attr} ev={dict(ec)}",
          flush=True)
    shutil.rmtree(out, ignore_errors=True)

print("\n=== FULL TABLE ===")
print(f"{'domain':<11}{'nodes':>8}{'m.deg':>8}{'clust':>8}{'recip':>8}{'B':>7}{'alpha':>7}{'memM':>7}{'rep':>7}  typ/attr")
for r in rows:
    print(f"{r['domain']:<11}{r['n_nodes']:>8}{r['mean_degree']:>8.1f}{r['clustering']:>8.4f}"
          f"{r['reciprocity']:>8.4f}{r['burstiness']:>7.3f}{r['alpha']:>7.2f}{r['memory']:>7.3f}"
          f"{r['repeat']:>7.3f}  {r['nt']}/{r['et']},{r['attr']}")

import json
with open(os.path.join(SCRATCH, "results.json"), "w") as fh:
    json.dump(rows, fh, indent=1)
