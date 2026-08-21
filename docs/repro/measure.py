#!/usr/bin/env python3
"""Anomaly-realism metrics under the anom.py protocol (docs/remeasure_fidelity.md).

homophily   = (both/out_edges) / (|mset|/n)
              out_edges = #edges whose SRC in mset; both = those with DST also in mset
deg ratio   = mean_totdeg(marked) / mean_totdeg(non-marked)   (nodes in edge list)
in/out avg  = (mean_indeg(marked) + mean_outdeg(marked)) / 2
"""
import sys, glob
import numpy as np
import pandas as pd


def metrics(src, dst, mset):
    src = np.asarray(src, dtype=np.int64)
    dst = np.asarray(dst, dtype=np.int64)
    nodes = np.unique(np.concatenate([src, dst]))
    n = len(nodes)
    mset = np.intersect1d(np.asarray(sorted(mset), dtype=np.int64), nodes)
    M = len(mset)
    marked = np.zeros(nodes.max() + 1, dtype=bool)
    marked[mset] = True

    src_marked = marked[src]
    dst_marked = marked[dst]
    out_edges = int(src_marked.sum())
    both = int((src_marked & dst_marked).sum())
    base = M / n
    homophily = (both / out_edges) / base if out_edges and base else float("nan")

    outdeg = pd.Series(src).value_counts()
    indeg = pd.Series(dst).value_counts()
    tot = outdeg.add(indeg, fill_value=0).reindex(nodes, fill_value=0).values
    outd = outdeg.reindex(nodes, fill_value=0).values
    ind = indeg.reindex(nodes, fill_value=0).values
    is_m = np.isin(nodes, mset)

    deg_ratio = tot[is_m].mean() / tot[~is_m].mean()
    in_out_avg = (ind[is_m].mean() + outd[is_m].mean()) / 2.0
    return dict(homophily=homophily, deg_ratio=deg_ratio, in_out_avg=in_out_avg,
                out_edges=out_edges, both=both, base=base, M=M, n=n,
                mean_in=ind[is_m].mean(), mean_out=outd[is_m].mean())


def load_agora(d):
    files = sorted(glob.glob(f"{d}/edges_*.csv")) or sorted(glob.glob(f"{d}/edges*.csv"))
    e = pd.concat([pd.read_csv(f, usecols=["src", "dst"]) for f in files], ignore_index=True)
    lab = pd.read_csv(f"{d}/labels_nodes.csv")
    mset = set(lab["node_id"].astype(np.int64).tolist())
    return e["src"].values, e["dst"].values, mset


def load_elliptic(root):
    e = pd.read_csv(f"{root}/elliptic_txs_edgelist.csv")
    c = pd.read_csv(f"{root}/elliptic_txs_classes.csv")
    mset = set(c.loc[c["class"] == "1", "txId"].astype(np.int64).tolist())
    return e.iloc[:, 0].values, e.iloc[:, 1].values, mset


if __name__ == "__main__":
    if sys.argv[1] == "elliptic":
        src, dst, mset = load_elliptic(sys.argv[2])
    else:
        src, dst, mset = load_agora(sys.argv[1])
    m = metrics(src, dst, mset)
    print(f"homophily={m['homophily']:.3f} deg_ratio={m['deg_ratio']:.3f} "
          f"in_out_avg={m['in_out_avg']:.3f} | out_edges={m['out_edges']} both={m['both']} "
          f"base={m['base']:.4f} M={m['M']} n={m['n']} "
          f"mean_in={m['mean_in']:.3f} mean_out={m['mean_out']:.3f}")
