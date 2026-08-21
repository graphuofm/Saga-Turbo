#!/usr/bin/env python3
"""BEFORE-baseline item 1+2: g(pi) sweep + anomaly subpopulation stats.
6 domains x pi in {0.005,0.01,0.02,0.05,0.10}, --edges 1000000 --seed 42.
Every number from a real run of the PINNED baseline binary (/tmp/saga_base)."""
import collections
import csv
import glob
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np
import pyarrow.parquet as papq

SP = "./scratch"
sys.path.insert(0, os.path.join(SP, "pin"))
from saga_eval import stats as st  # noqa: E402

BIN = "/tmp/saga_base/target/release/saga"
WORK = os.path.join(SP, "work_gpi")
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
PIS = [0.005, 0.01, 0.02, 0.05, 0.10]
EDGES = 1_000_000
SEED = 42

os.makedirs(WORK, exist_ok=True)
rng = np.random.default_rng(0)
results = []

for dom in DOMAINS:
    for pi in PIS:
        t0 = time.time()
        out = os.path.join(WORK, f"{dom}_{pi}")
        shutil.rmtree(out, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", dom, "--edges", str(EDGES),
               "--seed", str(SEED), "--anomaly-rate", str(pi),
               "--out", out, "--quiet"]
        row = dict(domain=dom, pi=pi, edges_requested=EDGES, seed=SEED, cmd=" ".join(cmd))
        try:
            subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=900)
            srcs, dsts, ts, labs = [], [], [], []
            for f in sorted(glob.glob(os.path.join(out, "edges_*.parquet"))):
                tbl = papq.read_table(f, columns=["src", "dst", "t", "label"])
                srcs.append(tbl["src"].to_numpy())
                dsts.append(tbl["dst"].to_numpy())
                ts.append(tbl["t"].to_numpy())
                labs += tbl["label"].to_pylist()
            src = np.concatenate(srcs).astype(np.int64)
            dst = np.concatenate(dsts).astype(np.int64)
            t = np.concatenate(ts).astype(np.float64)
            elab = np.array([x != "normal" for x in labs])
            n = int(max(src.max(), dst.max())) + 1

            # --- ITEM 1: realized anomalous-edge fraction / count / total
            m = int(src.size)
            n_anom_e = int(elab.sum())
            row.update(total_edges=m, anom_edges=n_anom_e,
                       anom_edge_fraction=n_anom_e / m if m else float("nan"))

            # --- node ground truth
            lab = np.zeros(n, dtype=np.int8)
            lp = os.path.join(out, "labels_nodes.csv")
            if os.path.exists(lp):
                with open(lp) as fh:
                    for r in csv.DictReader(fh):
                        nid = int(r["node_id"])
                        if nid < n:
                            lab[nid] = 1
            row["n_nodes_total"] = int(np.unique(np.concatenate([src, dst])).size)
            row["n_anom_nodes"] = int((lab == 1).sum())
            row["n_norm_nodes"] = int((lab == 0).sum())
            row["realized_node_rate"] = row["n_anom_nodes"] / row["n_nodes_total"]

            # --- ITEM 2: degrees
            outd = np.bincount(src, minlength=n)
            ind = np.bincount(dst, minlength=n)
            tot = outd + ind
            A = lab == 1
            # normal = non-anomalous nodes that actually appear in the graph
            present = np.zeros(n, dtype=bool)
            present[np.unique(np.concatenate([src, dst]))] = True
            Nn = (lab == 0) & present
            f = lambda a: float(np.mean(a)) if a.size else float("nan")
            ad, nd = tot[A], tot[Nn]
            row["mean_deg_anom"] = f(ad)
            row["mean_deg_norm"] = f(nd)
            row["mean_deg_ratio"] = (f(ad) / f(nd)) if f(nd) else float("nan")
            row["mean_in_anom"], row["mean_out_anom"] = f(ind[A]), f(outd[A])
            row["mean_in_norm"], row["mean_out_norm"] = f(ind[Nn]), f(outd[Nn])
            row["in_out_ratio_anom"] = (f(ind[A]) / f(outd[A])) if f(outd[A]) else float("nan")
            row["in_out_ratio_norm"] = (f(ind[Nn]) / f(outd[Nn])) if f(outd[Nn]) else float("nan")

            # --- anomaly edge share: edges INCIDENT to an anomalous node
            mset = A
            m_src = mset[src]
            m_dst = mset[dst]
            inc = m_src | m_dst
            row["anom_incident_edges"] = int(inc.sum())
            row["anom_incident_edge_share"] = float(inc.mean())

            # --- clustering of the anomalous subgraph vs normal
            und = collections.defaultdict(set)
            for a, b in zip(src.tolist(), dst.tolist()):
                if a != b:
                    und[a].add(b)
                    und[b].add(a)

            def clus(targets):
                cand = [x for x in targets if len(und.get(int(x), ())) >= 2]
                if not cand:
                    return float("nan"), 0
                if len(cand) > 3000:
                    cand = rng.choice(np.array(cand), 3000, replace=False).tolist()
                s = 0.0
                for v in cand:
                    nb = list(und[int(v)])
                    k = len(nb)
                    links = 0
                    for i in range(k):
                        ain = und.get(nb[i], ())
                        for j in range(i + 1, k):
                            if nb[j] in ain:
                                links += 1
                    s += 2.0 * links / (k * (k - 1))
                return s / len(cand), len(cand)

            ca, na_ = clus(np.nonzero(A)[0].tolist())
            cn, nn_ = clus(np.nonzero(Nn)[0].tolist())
            row["clustering_anom"], row["clustering_anom_n"] = ca, na_
            row["clustering_norm"], row["clustering_norm_n"] = cn, nn_
            row["clustering_ratio"] = (ca / cn) if cn else float("nan")
            row["status"] = "OK"
        except Exception as e:  # noqa: BLE001
            row["status"] = f"FAILED: {type(e).__name__}: {e}"
        finally:
            shutil.rmtree(out, ignore_errors=True)  # disk is testing-only
        row["wall_s"] = round(time.time() - t0, 1)
        results.append(row)
        print(f"[{dom} pi={pi}] {row['status']} edges={row.get('total_edges')} "
              f"anom_e={row.get('anom_edges')} frac={row.get('anom_edge_fraction')} "
              f"degratio={row.get('mean_deg_ratio')} {row['wall_s']}s", flush=True)
        json.dump(results, open(os.path.join(SP, "before_gpi.json"), "w"), indent=1, default=float)

print("DONE item1+2")
