#!/usr/bin/env python3
"""Structural properties used to explain the query-latency differences:
degree skew (max/mean and top-1% share of out- and in-degree), recurrence
(fraction of edges whose (src,dst) pair appears more than once; distinct-pair
ratio), and burstiness (Goh-Barabasi B on the global inter-event gap, and the
fraction of edges in the busiest 1% of days). Computed with DuckDB (4 threads)
over the canonical edges.csv.  usage: struct_stats.py <out_json> name=dir ..."""
import json
import sys

import duckdb


def stats(d):
    c = duckdb.connect()
    c.execute("SET threads = 4")
    c.execute(f"CREATE TABLE e AS SELECT src, dst, t FROM read_csv('{d}/edges.csv')")
    m = c.execute("SELECT count(*) FROM e").fetchone()[0]
    r = {"edges": m, "nodes": c.execute(f"SELECT count(*) FROM read_csv('{d}/nodes.csv')").fetchone()[0]}
    for col, nm in (("src", "out"), ("dst", "in")):
        deg = c.execute(f"""WITH d AS (SELECT {col} AS v, count(*) AS k FROM e GROUP BY 1)
            SELECT max(k), avg(k), count(*),
                   (SELECT sum(k) FROM (SELECT k FROM d ORDER BY k DESC LIMIT greatest(1, cast(0.01 * (SELECT count(*) FROM d) AS BIGINT)))) FROM d""").fetchone()
        r[f"{nm}_deg_max"] = deg[0]
        r[f"{nm}_deg_mean"] = round(deg[1], 2)
        r[f"{nm}_deg_max_over_mean"] = round(deg[0] / deg[1], 1)
        r[f"{nm}_top1pct_share"] = round(deg[3] / m, 3)
    pairs = c.execute("""SELECT count(*), sum(CASE WHEN k > 1 THEN k ELSE 0 END) FROM (SELECT src, dst, count(*) AS k FROM e GROUP BY 1, 2)""").fetchone()
    r["distinct_pairs"] = pairs[0]
    r["distinct_pair_ratio"] = round(pairs[0] / m, 3)
    r["repeat_edge_fraction"] = round(pairs[1] / m, 3)
    g = c.execute("""SELECT avg(g), stddev_pop(g) FROM (SELECT t - lag(t) OVER (ORDER BY t) AS g FROM e) WHERE g IS NOT NULL""").fetchone()
    r["burstiness_B"] = round((g[1] - g[0]) / (g[1] + g[0]), 3)
    day = c.execute("""WITH d AS (SELECT (t - (SELECT min(t) FROM e)) // 86400 AS day, count(*) AS k FROM e GROUP BY 1)
        SELECT count(*), max(k), avg(k), (SELECT sum(k) FROM (SELECT k FROM d ORDER BY k DESC LIMIT greatest(1, cast(0.01 * (SELECT count(*) FROM d) AS BIGINT)))) FROM d""").fetchone()
    r["days"] = day[0]
    r["day_max_over_mean"] = round(day[1] / day[2], 2)
    r["busiest1pct_days_share"] = round(day[3] / m, 3)
    return r


if __name__ == "__main__":
    out = {}
    for a in sys.argv[2:]:
        n, d = a.split("=", 1)
        out[n] = stats(d)
        print(n, json.dumps(out[n]), flush=True)
    json.dump(out, open(sys.argv[1], "w"), indent=1)
