#!/usr/bin/env python3
"""Like-for-like structural baselines (BA, ER) at the same node/edge counts as
SAGA finance 1M, written in the canonical db_workload schema.

Reuses python/saga_eval/baselines.py (gen_ba / gen_er): uniform random
timestamps over the span, BA edges randomly oriented. Timestamps are mapped to
the same epoch/span SAGA uses (epoch 1735689600, 90 days) so the workload's
windows are comparable.

usage: gen_baselines.py <out_root> <n_nodes> <n_edges> [seed]
"""
import os
import sys
import time

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "python"))
from saga_eval.baselines import gen_ba, gen_er  # noqa: E402

EPOCH = 1735689600
SPAN_S = 90 * 86400


def write(out, name, n, s, d, t):
    os.makedirs(os.path.join(out, name), exist_ok=True)
    tt = (EPOCH + t).astype(np.int64)
    with open(os.path.join(out, name, "nodes.csv"), "w") as f:
        f.write("id,type,attrs\n")
        for i in range(n):
            f.write(f"{i},node,\n")
    with open(os.path.join(out, name, "edges.csv"), "w") as f:
        f.write("src,dst,t,event_type,label,amount\n")
        for a, b, c in zip(s.tolist(), d.tolist(), tt.tolist()):
            f.write(f"{a},{b},{c},edge,,\n")
    print(f"{name}: nodes={n} edges={s.size}")


def main():
    out, n, m = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    seed = int(sys.argv[4]) if len(sys.argv) > 4 else 42
    t0 = time.time()
    s, d, t = gen_ba(n, m, SPAN_S, seed)
    write(out, "ba_1m", n, s, d, t)
    print(f"gen_ba wall={time.time()-t0:.1f}s")
    t0 = time.time()
    s, d, t = gen_er(n, m, SPAN_S, seed)
    write(out, "er_1m", n, s, d, t)
    print(f"gen_er wall={time.time()-t0:.1f}s")


if __name__ == "__main__":
    main()
