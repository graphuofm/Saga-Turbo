#!/usr/bin/env python3
"""Load a canonical dataset (nodes.csv, edges.csv) into an embedded Kuzu
database and run the six-query temporal workload.

Protocol (per dataset):
  * /proc/loadavg is read before each timed block; if the 1-min load is > 8 the
    block waits (polls every 10 s) until it drops.
  * Load: fresh database, CREATE tables, COPY nodes then edges; timed twice
    (two fresh databases); the faster run is reported. On-disk size is the
    size of the database directory after a checkpoint and close.
  * Queries: on the loaded database, each query is run once as warm-up and then
    5 times; the median is kept. That whole block is run twice and the faster
    median is reported. All runs are warm (same process, same connection).
  * Sampling: sources for Q1/Q2 are drawn (numpy seed 42) from the distinct
    source ids of the dataset; pairs for Q4 are 100 distinct (src,dst) pairs
    sampled from the edge set; the 7-day window for Q1/Q2 starts at
    t_min + 0.4 * span, the 1-day window for Q3 starts at t_min + 0.5 * span.
    Windows are placed relative to each dataset's own span so they are
    like-for-like across datasets with different absolute time ranges.

usage: run_workload.py <name> <canon_dir> <db_root> <out_json> [threads]
"""
import json
import os
import shutil
import statistics
import subprocess
import sys
import time

import duckdb
import kuzu
import numpy as np

DAY = 86400


def loadavg():
    return float(open("/proc/loadavg").read().split()[0])


def wait_load(max_load=8.0):
    while loadavg() > max_load:
        print(f"  loadavg {loadavg():.2f} > {max_load}, waiting", flush=True)
        time.sleep(10)
    return loadavg()


def du(path):
    tot = int(subprocess.check_output(["du", "-sb", path]).split()[0])
    if os.path.exists(path + ".wal"):
        tot += int(subprocess.check_output(["du", "-sb", path + ".wal"]).split()[0])
    return tot


SCHEMA = [
    "CREATE NODE TABLE Node(id INT64, type STRING, attrs STRING, PRIMARY KEY(id))",
    "CREATE REL TABLE Edge(FROM Node TO Node, t INT64, event_type STRING, label STRING, amount DOUBLE)",
]


def rm_db(dbpath):
    # Kuzu >= 0.11 stores the database as a single file (+ optional .wal)
    for p in (dbpath, dbpath + ".wal"):
        if os.path.isdir(p):
            shutil.rmtree(p)
        elif os.path.exists(p):
            os.remove(p)


def load_once(canon, dbpath, threads):
    rm_db(dbpath)
    t0 = time.perf_counter()
    db = kuzu.Database(dbpath)
    con = kuzu.Connection(db, num_threads=threads)
    for s in SCHEMA:
        con.execute(s)
    t1 = time.perf_counter()
    con.execute(f"COPY Node FROM '{canon}/nodes.csv' (HEADER=true)")
    t2 = time.perf_counter()
    con.execute(f"COPY Edge FROM '{canon}/edges.csv' (HEADER=true)")
    t3 = time.perf_counter()
    con.execute("CHECKPOINT")
    n_nodes = con.execute("MATCH (n:Node) RETURN count(n)").get_next()[0]
    n_edges = con.execute("MATCH ()-[e:Edge]->() RETURN count(e)").get_next()[0]
    con.close()
    db.close()
    t4 = time.perf_counter()
    return dict(schema_s=t1 - t0, nodes_s=t2 - t1, edges_s=t3 - t2, total_s=t4 - t0,
                n_nodes=n_nodes, n_edges=n_edges, disk_bytes=du(dbpath))


def sample_params(canon, seed=42):
    c = duckdb.connect()
    tmin, tmax = c.execute(f"SELECT min(t), max(t) FROM read_csv('{canon}/edges.csv')").fetchone()
    srcs = np.array([r[0] for r in c.execute(f"SELECT DISTINCT src FROM read_csv('{canon}/edges.csv') ORDER BY src").fetchall()])
    pairs = c.execute(f"""SELECT src, dst FROM (SELECT DISTINCT src, dst FROM read_csv('{canon}/edges.csv'))
                          USING SAMPLE reservoir(100 ROWS) REPEATABLE ({seed})""").fetchall()
    rng = np.random.default_rng(seed)
    q1 = rng.choice(srcs, 100, replace=False).tolist()
    q2 = rng.choice(srcs, 20, replace=False).tolist()
    span = tmax - tmin
    return dict(t_min=int(tmin), t_max=int(tmax), span_days=span / DAY,
                w0=int(tmin + 0.4 * span), d0=int(tmin + 0.5 * span),
                q1_sources=q1, q2_sources=q2, q4_pairs=[[int(a), int(b)] for a, b in pairs])


def queries(p, has_labels):
    w0, w1 = p["w0"], p["w0"] + 7 * DAY
    d0, d1 = p["d0"], p["d0"] + DAY
    t0 = p["t_min"]
    q = {}
    q["Q1"] = f"""UNWIND {p['q1_sources']} AS sid
MATCH (a:Node {{id: sid}})-[e:Edge]->(b:Node)
WHERE e.t >= {w0} AND e.t < {w1}
RETURN sid, count(e) AS out_deg_7d"""
    q["Q2"] = f"""UNWIND {p['q2_sources']} AS sid
MATCH (a:Node {{id: sid}})-[e1:Edge]->(b:Node)-[e2:Edge]->(c:Node)
WHERE e1.t >= {w0} AND e1.t < {w1} AND e2.t > e1.t AND e2.t < {w1}
RETURN sid, count(DISTINCT c.id) AS reach_2hop"""
    q["Q3"] = f"""MATCH (a:Node)-[e:Edge]->(b:Node)
WHERE e.t >= {d0} AND e.t < {d1}
RETURN b.id AS node, count(e) AS in_deg
ORDER BY in_deg DESC, node ASC LIMIT 10"""
    q["Q4"] = f"""UNWIND {p['q4_pairs']} AS pr
MATCH (a:Node {{id: pr[1]}})-[e:Edge]->(b:Node {{id: pr[2]}})
RETURN pr[1] AS src, pr[2] AS dst, count(e) AS repeats"""
    if has_labels:
        q["Q5"] = """MATCH (a:Node)-[e:Edge]->(b:Node)
WHERE e.label <> 'normal'
RETURN a.id, a.type, a.attrs, b.id, b.type, b.attrs, e.t, e.event_type, e.label, e.amount"""
    q["Q6"] = f"""MATCH (a:Node)-[e:Edge]->(b:Node)
RETURN (e.t - {t0}) / {DAY} AS day, count(e) AS n_edges
ORDER BY day"""
    return q


def run_query(con, cypher):
    t0 = time.perf_counter()
    r = con.execute(cypher)
    n = 0
    last = None
    while r.has_next():
        last = r.get_next()
        n += 1
    return time.perf_counter() - t0, n, last


def time_queries(con, qs, reps=5):
    out = {}
    for name, cy in qs.items():
        run_query(con, cy)  # warm-up
        ts, n, last = [], None, None
        for _ in range(reps):
            t, n, last = run_query(con, cy)
            ts.append(t)
        out[name] = dict(median_ms=1000 * statistics.median(ts), min_ms=1000 * min(ts),
                         max_ms=1000 * max(ts), rows=n, sample_row=[str(x) for x in (last or [])][:6])
    return out


def main():
    name, canon, dbroot, outp = sys.argv[1:5]
    threads = int(sys.argv[5]) if len(sys.argv) > 5 else 0
    dbpath = os.path.join(dbroot, name + ".kuzu")
    res = dict(name=name, kuzu_version=kuzu.__version__, threads=threads or os.cpu_count(),
               canon_csv_bytes=du(canon))
    loads = []
    for i in range(2):
        la = wait_load()
        r = load_once(canon, dbpath, threads)
        r["loadavg_before"] = la
        print(f"  load#{i} total={r['total_s']:.2f}s nodes={r['nodes_s']:.2f}s edges={r['edges_s']:.2f}s loadavg={la}", flush=True)
        loads.append(r)
    res["loads"] = loads
    res["load"] = min(loads, key=lambda r: r["total_s"])
    p = sample_params(canon)
    res["params"] = p
    has_labels = duckdb.connect().execute(
        f"SELECT count(*) FROM read_csv('{canon}/edges.csv') WHERE label IS NOT NULL").fetchone()[0] > 0
    qs = queries(p, has_labels)
    res["queries"] = qs
    db = kuzu.Database(dbpath, read_only=True)
    con = kuzu.Connection(db, num_threads=threads)
    runs = []
    for i in range(2):
        la = wait_load()
        r = time_queries(con, qs)
        r["_loadavg_before"] = la
        print(f"  queries#{i} loadavg={la} " + " ".join(f"{k}={v['median_ms']:.1f}ms" for k, v in r.items() if not k.startswith("_")), flush=True)
        runs.append(r)
    con.close()
    db.close()
    res["query_runs"] = runs
    best = {}
    for k in qs:
        a, b = runs[0][k], runs[1][k]
        best[k] = a if a["median_ms"] <= b["median_ms"] else b
    if not has_labels:
        best["Q5"] = dict(median_ms=None, rows=None, note="N/A: no ground-truth labels")
    res["best"] = best
    rm_db(dbpath)
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps({k: (v["median_ms"], v["rows"]) for k, v in best.items()}))


if __name__ == "__main__":
    main()
