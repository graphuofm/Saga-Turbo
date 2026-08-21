"""P1-a homophily measurement. Same definitions as docs/remeasure_fidelity.md.

homophily (anom.py / doc §3 definition): (both.sum()/out_edges) / (M/N)
   out_edges = # edges whose SOURCE is marked ; both = # edges w/ BOTH endpoints marked
degree_ratio (doc §2): mean_totdeg(marked) / mean_totdeg(all non-marked graph nodes)
in/out (doc §4): mean in-deg and out-deg over marked ; report 0.5*(in+out) as "in/out avg"
"""
import sys, glob
import numpy as np
import pandas as pd


def metrics(src, dst, marked_ids):
    src = np.asarray(src); dst = np.asarray(dst)
    both_ep = np.concatenate([src, dst])
    nodes, inv = np.unique(both_ep, return_inverse=True)
    E = src.size; N = nodes.size
    s = inv[:E]; d = inv[E:]
    outdeg = np.bincount(s, minlength=N).astype(float)
    indeg = np.bincount(d, minlength=N).astype(float)
    totdeg = outdeg + indeg

    marked_ids = set(int(x) for x in marked_ids)
    marked_node = np.array([int(nid) in marked_ids for nid in nodes])
    M = int(marked_node.sum())
    base = M / N

    mean_tot_anom = float(totdeg[marked_node].mean()) if M > 0 else float("nan")
    mean_tot_norm = float(totdeg[~marked_node].mean()) if (N - M) > 0 else float("nan")
    degree_ratio = mean_tot_anom / mean_tot_norm if mean_tot_norm > 0 else float("nan")

    m_s = marked_node[s]; m_d = marked_node[d]
    both = m_s & m_d
    out_edges = int(m_s.sum())
    homophily_out = (float(both.sum()) / out_edges) / base if out_edges and base > 0 else float("nan")
    homophily_E = (float(both.sum()) / E) / base if base > 0 else float("nan")

    mean_in = float(indeg[marked_node].mean()) if M > 0 else float("nan")
    mean_out = float(outdeg[marked_node].mean()) if M > 0 else float("nan")

    return dict(E=E, N=N, M=M, base=base,
                out_edges_marked=out_edges, both_marked=int(both.sum()),
                homophily=homophily_out, homophily_Edenom=homophily_E,
                mean_totdeg_anom=mean_tot_anom, mean_totdeg_norm=mean_tot_norm,
                degree_ratio=degree_ratio,
                mean_in_anom=mean_in, mean_out_anom=mean_out,
                inout_avg=0.5 * (mean_in + mean_out))


def load_agora(d):
    csvs = sorted(glob.glob(d + "/edges_*.csv"))
    if csvs:
        df = pd.concat([pd.read_csv(f) for f in csvs], ignore_index=True)
    else:
        import pyarrow.parquet as papq
        parts = [papq.read_table(f, columns=["src", "dst"]).to_pandas()
                 for f in sorted(glob.glob(d + "/edges_*.parquet"))]
        df = pd.concat(parts, ignore_index=True)
    lab = pd.read_csv(d + "/labels_nodes.csv")
    return df["src"].to_numpy(), df["dst"].to_numpy(), lab["node_id"].to_numpy()


def load_elliptic(d="./realdata/elliptic-bitcoin"):
    edg = pd.read_csv(d + "/elliptic_txs_edgelist.csv")
    cls = pd.read_csv(d + "/elliptic_txs_classes.csv")
    src = edg["txId1"].to_numpy(); dst = edg["txId2"].to_numpy()
    illicit = cls.loc[cls["class"].astype(str) == "1", "txId"].to_numpy()
    return src, dst, illicit


def show(tag, m):
    print(f"[{tag}] E={m['E']} N={m['N']} M={m['M']} base={m['base']:.4f} "
          f"out_edges={m['out_edges_marked']} both={m['both_marked']}")
    print(f"   homophily(out-denom)={m['homophily']:.3f}  (E-denom={m['homophily_Edenom']:.3f})  "
          f"degree_ratio={m['degree_ratio']:.3f}  "
          f"in={m['mean_in_anom']:.3f} out={m['mean_out_anom']:.3f} in/out_avg={m['inout_avg']:.3f}")


if __name__ == "__main__":
    if sys.argv[1] == "elliptic":
        show("Elliptic", metrics(*load_elliptic()))
    else:
        show(sys.argv[1], metrics(*load_agora(sys.argv[2])))
