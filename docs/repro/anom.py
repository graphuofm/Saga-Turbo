"""Anomaly-structure comparison: Elliptic illicit-vs-licit  vs  SAGA anomalous-vs-normal.

Everything measured, nothing estimated. Subpopulation stats are computed
identically on both sides from (src, dst, t, node_label).
"""
from __future__ import annotations
import csv, json, os, sys, glob
import collections
import numpy as np

sys.path.insert(0, "./python")
from saga_eval import stats as st

SCRATCH = "$WORK"
ELL = "./realdata/elliptic-bitcoin"


# ---------- loading ----------------------------------------------------------
def load_elliptic():
    cls = {}
    with open(os.path.join(ELL, "elliptic_txs_classes.csv")) as fh:
        r = csv.reader(fh); next(r)
        for row in r:
            cls[row[0]] = row[1]
    tstep = {}
    with open(os.path.join(ELL, "elliptic_txs_features.csv")) as fh:
        for line in fh:
            i = line.index(",")
            j = line.index(",", i + 1)
            tstep[line[:i]] = int(float(line[i + 1:j]))
    src_r, dst_r = [], []
    with open(os.path.join(ELL, "elliptic_txs_edgelist.csv")) as fh:
        r = csv.reader(fh); next(r)
        for row in r:
            src_r.append(row[0]); dst_r.append(row[1])
    ids = {v: i for i, v in enumerate(sorted(set(src_r) | set(dst_r) | set(cls)))}
    n = len(ids)
    src = np.array([ids[x] for x in src_r], dtype=np.int64)
    dst = np.array([ids[x] for x in dst_r], dtype=np.int64)
    # edge time = source tx's time step (endpoints share the step per SCHEMA)
    ts_node = np.zeros(n, dtype=np.float64)
    for k, v in tstep.items():
        ts_node[ids[k]] = v
    t = ts_node[src]
    # node labels: 1=illicit -> 1 ; 2=licit -> 0 ; unknown -> -1
    lab = np.full(n, -1, dtype=np.int8)
    for k, v in cls.items():
        lab[ids[k]] = 1 if v == "1" else (0 if v == "2" else -1)
    return src, dst, t, lab, ts_node, n


def load_saga(d):
    import pyarrow.parquet as papq
    srcs, dsts, ts, labs = [], [], [], []
    for f in sorted(glob.glob(os.path.join(d, "edges_*.parquet"))):
        tbl = papq.read_table(f, columns=["src", "dst", "t", "label"])
        srcs.append(tbl["src"].to_numpy()); dsts.append(tbl["dst"].to_numpy())
        ts.append(tbl["t"].to_numpy()); labs += tbl["label"].to_pylist()
    src = np.concatenate(srcs).astype(np.int64)
    dst = np.concatenate(dsts).astype(np.int64)
    t = np.concatenate(ts).astype(np.float64)
    elab = np.array([x != "normal" for x in labs])
    n = int(max(src.max(), dst.max())) + 1
    # node label: anomalous node = a member of any anomaly instance (ground truth)
    lab = np.zeros(n, dtype=np.int8)
    with open(os.path.join(d, "labels_nodes.csv")) as fh:
        r = csv.DictReader(fh)
        for row in r:
            nid = int(row["node_id"])
            if nid < n:
                lab[nid] = 1
    return src, dst, t, lab, elab, n


# ---------- subpopulation statistics ----------------------------------------
def build_adj(src, dst, n):
    und = collections.defaultdict(set)
    for a, b in zip(src.tolist(), dst.tolist()):
        if a != b:
            und[a].add(b); und[b].add(a)
    return und


def local_clustering(und, targets, rng, sample=3000):
    cand = [x for x in targets if len(und.get(int(x), ())) >= 2]
    if not cand:
        return float("nan"), 0
    if len(cand) > sample:
        cand = rng.choice(np.array(cand), sample, replace=False).tolist()
    tot = 0.0
    for v in cand:
        nb = list(und[int(v)]); k = len(nb); links = 0
        for i in range(k):
            ain = und.get(nb[i], ())
            for j in range(i + 1, k):
                if nb[j] in ain:
                    links += 1
        tot += 2.0 * links / (k * (k - 1))
    return tot / len(cand), len(cand)


def subpop(name, src, dst, t, lab, n, mask_val, und, fwd, rng):
    """All structure stats for the node subpopulation with lab == mask_val."""
    members = np.nonzero(lab == mask_val)[0]
    mset = set(members.tolist())
    out_deg = np.bincount(src, minlength=n)
    in_deg = np.bincount(dst, minlength=n)
    tot = out_deg + in_deg
    o, i_, tt = out_deg[members], in_deg[members], tot[members]

    # edges incident to the subpopulation
    m_src = np.array([s in mset for s in src.tolist()])
    m_dst = np.array([d in mset for d in dst.tolist()])
    inc = m_src | m_dst
    both = m_src & m_dst

    # homophily: of edges leaving members, what frac land on members
    out_edges = m_src.sum()
    homophily = float(both.sum() / out_edges) if out_edges else float("nan")
    # baseline expectation if partners were random among *labeled-or-not* nodes
    base = len(mset) / n

    # reciprocity restricted to edges incident to members
    ipairs = set()
    for k in np.nonzero(inc)[0].tolist():
        ipairs.add((int(src[k]), int(dst[k])))
    recip = sum(1 for (a, b) in ipairs if (b, a) in fwd) / max(1, len(ipairs))

    clus, nclus = local_clustering(und, members.tolist(), rng)

    alpha, xmin = st.powerlaw_mle(tt.astype(float))

    # fan-in/fan-out ratio (per-node, degree-1+ nodes only)
    both_d = (o > 0) & (i_ > 0)
    fan = float(np.median(i_[both_d] / o[both_d])) if both_d.sum() else float("nan")

    # temporal concentration of the subpopulation's incident edges
    ti = t[inc]
    if ti.size:
        cnt = collections.Counter(np.round(ti).astype(np.int64).tolist())
        vals = np.array(list(cnt.values()), dtype=float)
        # Gini of activity across occupied time buckets (burst-in-time proxy)
        sv = np.sort(vals); cum = np.cumsum(sv)
        gini = float((sv.size + 1 - 2 * np.sum(cum) / cum[-1]) / sv.size)
        nbuckets = int(sv.size)
    else:
        gini, nbuckets = float("nan"), 0

    return {
        "population": name,
        "n_nodes": int(members.size),
        "frac_of_nodes": float(members.size / n),
        "n_incident_edges": int(inc.sum()),
        "n_internal_edges": int(both.sum()),
        "mean_total_degree": float(np.mean(tt)) if tt.size else float("nan"),
        "median_total_degree": float(np.median(tt)) if tt.size else float("nan"),
        "max_total_degree": int(tt.max()) if tt.size else 0,
        "mean_in_degree": float(np.mean(i_)) if i_.size else float("nan"),
        "mean_out_degree": float(np.mean(o)) if o.size else float("nan"),
        "in_out_ratio_mean": float(np.mean(i_) / np.mean(o)) if o.size and np.mean(o) > 0 else float("nan"),
        "fanin_fanout_median": fan,
        "clustering": clus,
        "clustering_n_sampled": nclus,
        "reciprocity_incident": float(recip),
        "homophily_out": homophily,
        "homophily_baseline_random": float(base),
        "homophily_lift": float(homophily / base) if base > 0 and homophily == homophily else float("nan"),
        "powerlaw_alpha": alpha,
        "powerlaw_xmin": xmin,
        "time_buckets_occupied": nbuckets,
        "activity_gini_over_time": gini,
        "_deg": tt,
    }


def ks(a, b):
    from scipy import stats as ss
    a = np.asarray(a, float); b = np.asarray(b, float)
    if a.size < 5 or b.size < 5:
        return float("nan")
    rng = np.random.default_rng(0)
    if a.size > 200000: a = rng.choice(a, 200000, replace=False)
    if b.size > 200000: b = rng.choice(b, 200000, replace=False)
    return float(ss.ks_2samp(a, b).statistic)


def analyse(tag, src, dst, t, lab, n):
    rng = np.random.default_rng(0)
    und = build_adj(src, dst, n)
    fwd = set(zip(src.tolist(), dst.tolist()))
    A = subpop("anomalous/illicit", src, dst, t, lab, n, 1, und, fwd, rng)
    B = subpop("normal/licit", src, dst, t, lab, n, 0, und, fwd, rng)
    contrast = {
        "degree_KS_anom_vs_norm": ks(A["_deg"], B["_deg"]),
        "mean_degree_ratio": A["mean_total_degree"] / B["mean_total_degree"],
        "clustering_ratio": A["clustering"] / B["clustering"] if B["clustering"] else float("nan"),
        "homophily_lift_ratio": A["homophily_lift"] / B["homophily_lift"] if B["homophily_lift"] else float("nan"),
    }
    A.pop("_deg"); B.pop("_deg")
    return {"dataset": tag, "n_nodes_total": n, "n_edges_total": int(src.size),
            "anomalous": A, "normal": B, "contrast": contrast}


if __name__ == "__main__":
    what = sys.argv[1]
    if what == "elliptic":
        src, dst, t, lab, ts_node, n = load_elliptic()
        base = {
            "n_nodes": n, "n_edges": int(src.size),
            "n_illicit": int((lab == 1).sum()), "n_licit": int((lab == 0).sum()),
            "n_unknown": int((lab == -1).sum()),
            "illicit_rate_all_nodes": float((lab == 1).sum() / n),
            "illicit_rate_labeled": float((lab == 1).sum() / max(1, (lab != -1).sum())),
            "n_time_steps": int(len(set(ts_node[ts_node > 0].tolist()))),
            "t_min": float(t.min()), "t_max": float(t.max()),
        }
        # can we do inter-event times? check gaps within a source
        order = np.lexsort((t, src))
        s_s, t_s = src[order], t[order]
        same = s_s[1:] == s_s[:-1]
        gaps = (t_s[1:] - t_s[:-1])[same]
        base["intra_source_gaps_n"] = int(gaps.size)
        base["intra_source_gaps_all_zero"] = bool(np.all(gaps == 0)) if gaps.size else None
        base["intra_source_gap_max"] = float(gaps.max()) if gaps.size else None
        res = analyse("elliptic", src, dst, t, lab, n)
        res["basics"] = base
        # per-time-step illicit counts
        illicit_nodes = np.nonzero(lab == 1)[0]
        c = collections.Counter(ts_node[illicit_nodes].astype(int).tolist())
        res["illicit_per_time_step"] = {str(k): v for k, v in sorted(c.items())}
        cl = collections.Counter(ts_node[np.nonzero(lab == 0)[0]].astype(int).tolist())
        res["licit_per_time_step"] = {str(k): v for k, v in sorted(cl.items())}
        json.dump(res, open(f"{SCRATCH}/elliptic_anom.json", "w"), indent=2, default=float)
        print(json.dumps(res, indent=2, default=float)[:6000])
    else:
        d = sys.argv[2]
        src, dst, t, lab, elab, n = load_saga(d)
        res = analyse("saga", src, dst, t, lab, n)
        res["basics"] = {
            "n_nodes": n, "n_edges": int(src.size),
            "n_anomalous_nodes": int((lab == 1).sum()),
            "anomalous_node_rate": float((lab == 1).sum() / n),
            "n_anomalous_edges": int(elab.sum()),
            "anomalous_edge_rate": float(elab.mean()),
            "t_min": float(t.min()), "t_max": float(t.max()),
        }
        json.dump(res, open(f"{SCRATCH}/saga_anom.json", "w"), indent=2, default=float)
        print(json.dumps(res, indent=2, default=float)[:6000])
