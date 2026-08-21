"""run_cell.py — one downstream cell: load a graph, run TGN + EdgeBank, dump JSON.

Reuses the project harness UNMODIFIED:
  saga_eval.tgn_eval.run_tgn        (TGN, 6 epochs, CPU-pinned by the harness)
  saga_eval.downstream_extra.run_edgebank
  saga_eval.baselines.gen_ba        (size-matched Barabasi-Albert, random timestamps)
  saga_eval.load.load_saga / load_real

One process = one cell, so many cells run concurrently with OMP_NUM_THREADS=1.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time

import numpy as np


def repeat_edge_fraction(src, dst):
    """Fraction of events whose (src,dst) pair is a repeat of an earlier event.
    Identical formula to saga_eval.stats.compute_stats.repeat_edge_ratio
    (1 - distinct_pairs / n_edges); inlined to avoid running the full and much
    more expensive statistic battery just for this one number."""
    n = int(src.size)
    if n == 0:
        return float("nan")
    pairs = np.unique(np.stack([src, dst], axis=1), axis=0).shape[0]
    return 1.0 - pairs / n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--kind", required=True, choices=["saga", "real", "ba"])
    ap.add_argument("--path", default=None, help="saga out dir or real file")
    ap.add_argument("--match-path", default=None,
                    help="for kind=ba: graph whose (n,m,span) BA is matched to")
    ap.add_argument("--match-kind", default="saga", choices=["saga", "real"])
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--limit", type=int, default=0,
                    help="time-prefix to the first N events (0 = use all)")
    ap.add_argument("--skip-tgn", action="store_true")
    a = ap.parse_args()

    from saga_eval import load
    from saga_eval import baselines as bl
    from saga_eval.tgn_eval import run_tgn
    from saga_eval.downstream_extra import run_edgebank

    def _load(kind, path):
        if kind == "saga":
            return load.load_saga(path)
        return load.load_real(path)

    if a.kind == "ba":
        ms, md, mt = _load(a.match_kind, a.match_path)
        if a.limit:
            o = np.argsort(mt, kind="stable")[: a.limit]
            ms, md, mt = ms[o], md[o], mt[o]
        n = int(np.unique(np.concatenate([ms, md])).size)
        m = int(ms.size)
        span = float(mt.max() - mt.min())
        # exactly as saga_eval.downstream_extra._load_datasets builds its BA
        s, d, t = bl.gen_ba(n, m, span, a.seed)
    else:
        s, d, t = _load(a.kind, a.path)
        if a.limit:
            o = np.argsort(t, kind="stable")[: a.limit]
            s, d, t = s[o], d[o], t[o]

    rec = {
        "label": a.label, "kind": a.kind, "seed": a.seed,
        "n_edges": int(s.size),
        "n_nodes": int(np.unique(np.concatenate([s, d])).size),
        "repeat_edge_fraction": repeat_edge_fraction(s, d),
    }
    print(f"[{a.label} seed={a.seed}] edges={rec['n_edges']} nodes={rec['n_nodes']} "
          f"repeat={rec['repeat_edge_fraction']:.3f}", flush=True)

    t0 = time.time()
    try:
        eb = run_edgebank(s, d, t, seed=a.seed)
        rec["edgebank"] = eb
        print(f"[{a.label} seed={a.seed}] EdgeBank inf_AP={eb['inf_test_ap']:.4f} "
              f"tw_AP={eb['tw_test_ap']:.4f}  ({time.time()-t0:.0f}s)", flush=True)
    except Exception as e:
        rec["edgebank"] = {"FAILED": repr(e)}
        print(f"[{a.label} seed={a.seed}] EdgeBank FAILED: {e!r}", flush=True)

    if not a.skip_tgn:
        t0 = time.time()
        try:
            tg = run_tgn(s, d, t, seed=a.seed)
            rec["tgn"] = tg
            print(f"[{a.label} seed={a.seed}] TGN test_AP={tg['test_ap']:.4f} "
                  f"({time.time()-t0:.0f}s)", flush=True)
        except Exception as e:
            rec["tgn"] = {"FAILED": repr(e)}
            print(f"[{a.label} seed={a.seed}] TGN FAILED: {e!r}", flush=True)
    else:
        rec["tgn"] = {"SKIPPED": "skip-tgn"}

    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w") as fh:
        json.dump(rec, fh, indent=2)
    print(f"[{a.label} seed={a.seed}] wrote {a.out}", flush=True)


if __name__ == "__main__":
    sys.exit(main())
