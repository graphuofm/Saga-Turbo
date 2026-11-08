#!/usr/bin/env python3
"""Detector-ranking agreement between real labeled data and SAGA.

The advisor review's key benchmark-validity question: do detectors RANK the same
way on SAGA as on real labeled data? Six qualitatively different detector
families score nodes of a temporal graph; we report average precision and
precision@k per (dataset, detector), then Spearman and Kendall correlation
between the rankings on real data and on a size-matched SAGA graph.
Every number comes from a real run. CPU only.
"""
import csv, json, math, os, subprocess, sys, tempfile, shutil
import numpy as np
from scipy import sparse
from sklearn.ensemble import HistGradientBoostingClassifier, IsolationForest
from sklearn.linear_model import LogisticRegression
from sklearn.decomposition import TruncatedSVD
from sklearn.metrics import average_precision_score
from sklearn.model_selection import train_test_split
from scipy.stats import spearmanr, kendalltau

ROOT = "."
BIN = f"{ROOT}/target/release/saga"

# ---------- data loading ----------
def load_elliptic():
    d = f"{ROOT}/realdata/elliptic-bitcoin"
    ts = {}
    with open(f"{d}/elliptic_txs_features.csv") as f:
        for line in f:
            p = line.split(",", 3)
            ts[int(p[0])] = int(p[1])
    lab = {}
    with open(f"{d}/elliptic_txs_classes.csv") as f:
        next(f)
        for line in f:
            a, b = line.strip().split(",")
            if b in ("1", "2"):
                lab[int(a)] = 1 if b == "1" else 0
    src, dst, t = [], [], []
    with open(f"{d}/elliptic_txs_edgelist.csv") as f:
        next(f)
        for line in f:
            a, b = line.strip().split(",")
            a, b = int(a), int(b)
            if a in ts and b in ts:
                src.append(a); dst.append(b); t.append(min(ts[a], ts[b]))
    return np.array(src), np.array(dst), np.array(t, dtype=float), lab

def load_saga(nodes, edges, span, seed, difficulty=None, out=None):
    tmp = out or tempfile.mkdtemp(prefix="rank_saga_")
    cmd = [BIN, "generate", "--domain", "finance", "--nodes", str(nodes), "--edges", str(edges),
           "--time-span", str(span), "--seed", str(seed), "--anomaly-rate", "0.02",
           "--format", "csv", "--out", tmp]
    if difficulty is not None:
        cmd += ["--difficulty", str(difficulty)]
    subprocess.run(cmd, capture_output=True, check=True)
    s, d, tt = [], [], []
    with open(f"{tmp}/edges_00000.csv") as f:
        r = csv.DictReader(f)
        for row in r:
            s.append(int(row["src"])); d.append(int(row["dst"])); tt.append(float(row["t"]))
    pos = set()
    with open(f"{tmp}/labels_nodes.csv") as f:
        for row in csv.DictReader(f):
            pos.add(int(row["node_id"]))
    ids = set(s) | set(d)
    lab = {i: (1 if i in pos else 0) for i in ids}
    shutil.rmtree(tmp, ignore_errors=True)
    return np.array(s), np.array(d), np.array(tt), lab

# ---------- features ----------
def build(src, dst, t, lab):
    ids = np.unique(np.concatenate([src, dst]))
    idx = {v: i for i, v in enumerate(ids)}
    n = len(ids)
    si = np.array([idx[v] for v in src]); di = np.array([idx[v] for v in dst])
    A = sparse.coo_matrix((np.ones(len(si)), (si, di)), shape=(n, n)).tocsr()
    outd = np.asarray(A.sum(1)).ravel(); ind = np.asarray(A.sum(0)).ravel()
    Ab = (A + A.T).tocsr()
    tot = outd + ind
    # per-node time stats
    ev = [[] for _ in range(n)]
    for a, b, tv in zip(si, di, t):
        ev[a].append(tv); ev[b].append(tv)
    first = np.zeros(n); last = np.zeros(n); nact = np.zeros(n)
    iet_m = np.zeros(n); iet_s = np.zeros(n); burst = np.zeros(n); maxpert = np.zeros(n)
    for i, e in enumerate(ev):
        if not e: continue
        e = np.sort(np.array(e)); first[i] = e[0]; last[i] = e[-1]
        u, c = np.unique(e, return_counts=True); nact[i] = len(u); maxpert[i] = c.max()
        if len(e) > 1:
            g = np.diff(e); iet_m[i] = g.mean(); iet_s[i] = g.std()
            den = iet_s[i] + iet_m[i]
            burst[i] = (iet_s[i] - iet_m[i]) / den if den > 0 else 0.0
    distinct_out = np.asarray((A > 0).sum(1)).ravel(); distinct_in = np.asarray((A > 0).sum(0)).ravel()
    rep = 1.0 - np.divide(distinct_out + distinct_in, np.maximum(tot, 1))
    nbr = np.asarray(Ab.dot(tot.reshape(-1, 1))).ravel() / np.maximum(np.asarray((Ab > 0).sum(1)).ravel(), 1)
    X = np.column_stack([outd, ind, tot, distinct_out, distinct_in, rep, first, last,
                         last - first, nact, iet_m, iet_s, burst, maxpert, nbr])
    X = np.nan_to_num(X, nan=0.0, posinf=0.0, neginf=0.0)
    mask = np.array([ids[i] in lab for i in range(n)])
    y = np.array([lab.get(ids[i], 0) for i in range(n)])
    return dict(ids=ids, A=A, Ab=Ab, X=X, y=y, mask=mask, si=si, di=di, t=t, n=n, tot=tot)

# ---------- detectors: each returns a score vector over all nodes ----------
def det_gbm(G, tr, te, seed):
    m = HistGradientBoostingClassifier(max_iter=200, random_state=seed)
    m.fit(G["X"][tr], G["y"][tr]); s = np.zeros(G["n"]); s[te] = m.predict_proba(G["X"][te])[:, 1]; return s

def det_spectral_lr(G, tr, te, seed):
    k = 16
    emb = TruncatedSVD(n_components=k, random_state=seed).fit_transform(G["Ab"])
    m = LogisticRegression(max_iter=1000)
    m.fit(emb[tr], G["y"][tr]); s = np.zeros(G["n"]); s[te] = m.predict_proba(emb[te])[:, 1]; return s

def det_iforest(G, tr, te, seed):
    m = IsolationForest(n_estimators=100, random_state=seed).fit(G["X"][tr])
    s = np.zeros(G["n"]); s[te] = -m.score_samples(G["X"][te]); return s

def _pagerank(A, iters=30, d=0.85):
    n = A.shape[0]; out = np.asarray(A.sum(1)).ravel(); out[out == 0] = 1
    P = sparse.diags(1.0 / out).dot(A).T.tocsr()
    r = np.full(n, 1.0 / n)
    for _ in range(iters):
        r = (1 - d) / n + d * P.dot(r)
    return r

def det_anomrank(G, tr, te, seed):
    med = np.median(G["t"])
    for half in (0, 1):
        sel = G["t"] <= med if half == 0 else G["t"] > med
        Ah = sparse.coo_matrix((np.ones(sel.sum()), (G["si"][sel], G["di"][sel])),
                               shape=(G["n"], G["n"])).tocsr()
        r = _pagerank(Ah + Ah.T)
        if half == 0: r0 = r
        else: r1 = r
    return np.abs(r1 - r0)

def det_midas(G, tr, te, seed):
    # count-min style surprise on (src,dst) and (src) counts per time bucket
    nb = 50
    tb = np.digitize(G["t"], np.linspace(G["t"].min(), G["t"].max(), nb))
    W, D = 4, 4096
    rng = np.random.default_rng(seed)
    seeds = rng.integers(1, 1 << 30, size=(W, 2))
    cur = np.zeros((W, D)); tot = np.zeros((W, D)); score = np.zeros(G["n"]); prev = -1
    for k in np.argsort(tb, kind="stable"):
        b = tb[k]
        if b != prev:
            tot += cur; cur[:] = 0; prev = b
        u, v = int(G["si"][k]), int(G["di"][k])
        h = [((u * int(seeds[w, 0]) + v * int(seeds[w, 1])) % 1000003) % D for w in range(W)]
        cur[np.arange(W), h] += 1
        c = min(cur[w, h[w]] for w in range(W)); s = min(tot[w, h[w]] for w in range(W))
        exp = max(s / max(b, 1), 1e-9)
        a = (c - exp) ** 2 / exp
        score[u] = max(score[u], a); score[v] = max(score[v], a)
    return score

def det_degree_z(G, tr, te, seed):
    x = G["tot"].astype(float); mu, sd = x.mean(), x.std() + 1e-9
    return (x - mu) / sd

DETS = [("GBM (supervised, features)", det_gbm),
        ("Spectral+LR (supervised)", det_spectral_lr),
        ("IsolationForest (unsupervised)", det_iforest),
        ("AnomRank-style (centrality shift)", det_anomrank),
        ("MIDAS-style (streaming sketch)", det_midas),
        ("Degree z-score (heuristic floor)", det_degree_z)]

def evaluate(G, seeds=(0, 1, 2)):
    lab_idx = np.where(G["mask"])[0]
    res = {}
    for name, fn in DETS:
        aps, pks = [], []
        for sd in seeds:
            tr, te = train_test_split(lab_idx, test_size=0.3, random_state=sd,
                                      stratify=G["y"][lab_idx])
            s = fn(G, tr, te, sd)
            yt, st = G["y"][te], s[te]
            aps.append(average_precision_score(yt, st))
            k = max(int(yt.sum()), 1)
            top = np.argsort(-st)[:k]
            pks.append(yt[top].mean())
        res[name] = dict(ap=float(np.mean(aps)), ap_sd=float(np.std(aps)),
                         pk=float(np.mean(pks)), n_test_pos=int(G["y"][lab_idx].sum()))
    return res

if __name__ == "__main__":
    out = {}
    print("loading Elliptic ...", flush=True)
    es, ed, et, elab = load_elliptic()
    GE = build(es, ed, et, elab)
    print(f"  Elliptic: {GE['n']} nodes, {len(es)} edges, labeled {GE['mask'].sum()}, positives {GE['y'][GE['mask']].sum()}", flush=True)
    out["elliptic"] = evaluate(GE)
    n_nodes, n_edges, span = GE["n"], len(es), 49
    for diff in [None, 0.0, 0.5, 1.0]:
        tag = "saga" if diff is None else f"saga_d{diff}"
        print(f"generating {tag} ...", flush=True)
        ss, sd_, st_, slab = load_saga(n_nodes, n_edges, span, 42, difficulty=diff)
        GS = build(ss, sd_, st_, slab)
        print(f"  {tag}: {GS['n']} nodes, {len(ss)} edges, positives {GS['y'].sum()}", flush=True)
        out[tag] = evaluate(GS)
    # rank correlations
    names = [n for n, _ in DETS]
    base = np.array([out["elliptic"][n]["ap"] for n in names])
    out["rank_correlation"] = {}
    for key in [k for k in out if k.startswith("saga")]:
        v = np.array([out[key][n]["ap"] for n in names])
        rho, pr = spearmanr(base, v); tau, pt = kendalltau(base, v)
        out["rank_correlation"][key] = dict(spearman=float(rho), spearman_p=float(pr),
                                            kendall=float(tau), kendall_p=float(pt))
    json.dump(out, open(f"{os.path.dirname(os.path.abspath(__file__))}/results.json", "w"), indent=1)
    print(json.dumps(out["rank_correlation"], indent=1))
