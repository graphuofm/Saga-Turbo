#!/usr/bin/env python3
"""Aggregate the per-dataset JSONs from run_workload.py into summary.csv and a
markdown table block (printed to stdout and written to summary.md)."""
import csv
import glob
import json
import os
import sys

ORDER = ["saga_1m", "saga_1m_guards", "ba_1m", "er_1m", "saga_10m", "ldbc_sf1"]
QS = ["Q1", "Q2", "Q3", "Q4", "Q5", "Q6"]


def fmt(v, nd=1):
    return "N/A" if v is None else f"{v:.{nd}f}"


def main():
    res_dir = sys.argv[1]
    rs = {}
    for f in glob.glob(os.path.join(res_dir, "*.json")):
        r = json.load(open(f))
        if "name" not in r:
            continue
        rs[r["name"]] = r
    names = [n for n in ORDER if n in rs] + sorted(set(rs) - set(ORDER))
    rows = []
    for n in names:
        r = rs[n]
        L = r["load"]
        row = dict(dataset=n, kuzu=r["kuzu_version"], nodes=L["n_nodes"], edges=L["n_edges"],
                   load_total_s=round(L["total_s"], 2), load_nodes_s=round(L["nodes_s"], 2),
                   load_edges_s=round(L["edges_s"], 2), db_mb=round(L["disk_bytes"] / 1e6, 1),
                   csv_mb=round(r["canon_csv_bytes"] / 1e6, 1), span_days=round(r["params"]["span_days"], 1),
                   loadavg_load=min(x["loadavg_before"] for x in r["loads"]),
                   loadavg_query=min(x["_loadavg_before"] for x in r["query_runs"]))
        for q in QS:
            b = r["best"].get(q)
            row[q + "_ms"] = None if b is None or b["median_ms"] is None else round(b["median_ms"], 2)
            row[q + "_rows"] = None if b is None else b["rows"]
        rows.append(row)
    with open(os.path.join(res_dir, "summary.csv"), "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    out = []
    out.append("| dataset | nodes | edges | span (days) | CSV MB | load total s | nodes s | edges s | DB on disk MB | loadavg |")
    out.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        out.append(f"| {r['dataset']} | {r['nodes']:,} | {r['edges']:,} | {r['span_days']} | {r['csv_mb']} | {r['load_total_s']} | {r['load_nodes_s']} | {r['load_edges_s']} | {r['db_mb']} | {r['loadavg_load']} |")
    out.append("")
    out.append("| dataset | " + " | ".join(f"{q} ms (rows)" for q in QS) + " | loadavg |")
    out.append("|---|" + "---:|" * (len(QS) + 1))
    for r in rows:
        cells = []
        for q in QS:
            v, n = r[q + "_ms"], r[q + "_rows"]
            cells.append("N/A" if v is None else f"{v:.1f} ({n})")
        out.append(f"| {r['dataset']} | " + " | ".join(cells) + f" | {r['loadavg_query']} |")
    if "saga_1m" in rs and "saga_10m" in rs:
        a, b = rs["saga_1m"], rs["saga_10m"]
        out.append("")
        out.append("| metric | SAGA 1M | SAGA 10M | ratio (10M/1M) |")
        out.append("|---|---:|---:|---:|")
        ea, eb = a["load"]["n_edges"], b["load"]["n_edges"]
        out.append(f"| edges | {ea:,} | {eb:,} | {eb/ea:.2f} |")
        out.append(f"| load total s | {a['load']['total_s']:.2f} | {b['load']['total_s']:.2f} | {b['load']['total_s']/a['load']['total_s']:.2f} |")
        out.append(f"| load edges s | {a['load']['edges_s']:.2f} | {b['load']['edges_s']:.2f} | {b['load']['edges_s']/a['load']['edges_s']:.2f} |")
        out.append(f"| DB on disk MB | {a['load']['disk_bytes']/1e6:.1f} | {b['load']['disk_bytes']/1e6:.1f} | {b['load']['disk_bytes']/a['load']['disk_bytes']:.2f} |")
        for q in ("Q1", "Q3", "Q6"):
            x, y = a["best"][q]["median_ms"], b["best"][q]["median_ms"]
            out.append(f"| {q} ms | {x:.1f} | {y:.1f} | {y/x:.2f} |")
    txt = "\n".join(out)
    open(os.path.join(res_dir, "summary.md"), "w").write(txt + "\n")
    print(txt)


if __name__ == "__main__":
    main()
