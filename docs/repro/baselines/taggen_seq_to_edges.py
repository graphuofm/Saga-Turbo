"""Convert TagGen's generated random-walk sequences into a temporal edge list.

TagGen emits `<DATA>_output_sequences.txt`: comma-separated walks over composite
tokens, where each token indexes a (node, timestamp) pair through the
`index` dictionary saved in `graph.pickle` ("<node>_<time>" -> token).

The node/time decoding and the "consecutive pair becomes an edge at
max(t_i, t_j)" rule are taken from TagGen's own metrics.py
(temporal_metrics_only_FinTech, the loop over data_directory_2). metrics.py then
accumulates those edges into a cumulative adjacency and thresholds it to the
observed per-window edge count; that last step is TagGen's evaluation
convention, not part of generation, so it is not applied here. This script stops
at the temporal edge list, which is what the cross-generator comparison scores.

Usage: python3 taggen_seq_to_edges.py <TagGen/data/DIR> <NAME> <out.csv>
"""
import csv
import pickle
import sys

d, name, out = sys.argv[1], sys.argv[2], sys.argv[3]

attr = pickle.load(open(f"{d}/graph.pickle", "rb"))
node_index = attr["index"]                      # "<node>_<time>" -> token
inv = {v: k for k, v in node_index.items()}     # token -> "<node>_<time>"

rows = []
with open(f"{d}/{name}_output_sequences.txt") as f:
    for line in f:
        toks = [t.strip() for t in line.rstrip("\n").split(",") if t.strip()]
        try:
            toks = [int(t) for t in toks]
        except ValueError:
            continue
        for a, b in zip(toks, toks[1:]):
            if a not in inv or b not in inv:
                continue
            na, ta = inv[a].split("_")
            nb, tb = inv[b].split("_")
            if na == nb:
                continue
            rows.append((int(na), int(nb), max(int(ta), int(tb))))

with open(out, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["src", "dst", "t"])
    for s, dd, t in rows:
        w.writerow([s, dd, t])
print(f"{len(rows)} edges, {len({r[0] for r in rows} | {r[1] for r in rows})} nodes -> {out}")
