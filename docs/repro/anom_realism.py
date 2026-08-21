"""Anomaly-realism metrics, identical functions applied to AGORA and Elliptic.

Definitions (stated exactly for the doc):
 - marked/anomalous node set = AGORA: node_ids in labels_nodes.csv (adversary nodes)
                               Elliptic: txIds with class==1 (illicit)
 - N = number of distinct nodes that appear in the edge list (graph nodes)
 - M = |marked ∩ graph nodes|
 - degree = in+out (directed edges, endpoints as given)
 - degree_ratio = mean_total_degree(marked) / mean_total_degree(non-marked)
 - homophily (anom.py definition): both.sum()/E  divided by  (M/N)
        both[e] = 1 iff BOTH endpoints of edge e are in the marked set
        E = total number of edges
        i.e. (fraction of edges whose both endpoints are marked) / (marked node fraction)
 - anom in/out degree magnitude = mean in-degree and mean out-degree over the marked set
"""
import sys, glob
import numpy as np
import pandas as pd


def metrics(src, dst, marked_ids):
    src = np.asarray(src); dst = np.asarray(dst)
    both_ep = np.concatenate([src, dst])
    nodes, inv = np.unique(both_ep, return_inverse=True)
    E = src.size
    N = nodes.size
    s = inv[:E]; d = inv[E:]
    outdeg = np.bincount(s, minlength=N).astype(float)
    indeg = np.bincount(d, minlength=N).astype(float)
    totdeg = outdeg + indeg

    marked_ids = set(int(x) for x in marked_ids)
    marked_node = np.array([int(nid) in marked_ids for nid in nodes])
    M = int(marked_node.sum())

    # degree ratio anom / normal (normal = all non-marked graph nodes)
    mean_tot_anom = float(totdeg[marked_node].mean()) if M > 0 else float("nan")
    mean_tot_norm = float(totdeg[~marked_node].mean()) if (N - M) > 0 else float("nan")
    degree_ratio = mean_tot_anom / mean_tot_norm if mean_tot_norm > 0 else float("nan")

    # homophily (anom.py definition)
    marked_edge_s = marked_node[s]
    marked_edge_d = marked_node[d]
    both = marked_edge_s & marked_edge_d
    both_frac = float(both.sum()) / E
    base = M / N
    homophily = both_frac / base if base > 0 else float("nan")

    # anom in/out degree magnitude (mean over marked nodes)
    mean_in_anom = float(indeg[marked_node].mean()) if M > 0 else float("nan")
    mean_out_anom = float(outdeg[marked_node].mean()) if M > 0 else float("nan")
    mean_inout_anom = 0.5 * (mean_in_anom + mean_out_anom)

    return {
        "E": E, "N": N, "M_marked_nodes": M, "marked_frac": base,
        "mean_totdeg_anom": mean_tot_anom, "mean_totdeg_norm": mean_tot_norm,
        "degree_ratio_anom_over_norm": degree_ratio,
        "both_marked_edge_count": int(both.sum()), "both_marked_edge_frac": both_frac,
        "homophily": homophily,
        "mean_indeg_anom": mean_in_anom, "mean_outdeg_anom": mean_out_anom,
        "mean_inout_anom": mean_inout_anom,
    }


def load_agora(d):
    df = pd.concat([pd.read_csv(f) for f in sorted(glob.glob(d + "/edges_*.csv"))],
                   ignore_index=True)
    lab = pd.read_csv(d + "/labels_nodes.csv")
    return df["src"].to_numpy(), df["dst"].to_numpy(), lab["node_id"].to_numpy()


def load_elliptic(d):
    edg = pd.read_csv(d + "/elliptic_txs_edgelist.csv")
    cls = pd.read_csv(d + "/elliptic_txs_classes.csv")
    src = edg["txId1"].to_numpy(); dst = edg["txId2"].to_numpy()
    illicit = cls.loc[cls["class"].astype(str) == "1", "txId"].to_numpy()
    return src, dst, illicit


if __name__ == "__main__":
    agora_dir = sys.argv[1]
    ell_dir = sys.argv[2]
    print("=== AGORA finance (fixed engine) ===")
    a = metrics(*load_agora(agora_dir))
    for k, v in a.items():
        print(f"  {k:<28} {v}")
    print("=== Elliptic Bitcoin (real, class==1 illicit) ===")
    e = metrics(*load_elliptic(ell_dir))
    for k, v in e.items():
        print(f"  {k:<28} {v}")
