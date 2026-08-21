#!/usr/bin/env python3
"""Cross-domain downstream learnability: TGN + EdgeBank on each SAGA domain vs a
size-matched Barabasi-Albert graph. Answers: does a temporal model learn SAGA's
dynamics in EVERY domain (as it would from real data), while learning ~nothing from
bare topology?"""
import sys, os, json, time
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_saga
from saga_eval.tgn_eval import run_tgn
from saga_eval.downstream_extra import run_edgebank
from saga_eval.baselines import gen_ba

DS = "./scratch/ds"
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
K = 200_000  # time-prefix subsample so CPU TGN is tractable

def prefix(s, d, t, k):
    if s.size <= k:
        return s, d, t
    order = np.argsort(t, kind="stable")[:k]
    order = np.sort(order)
    return s[order], d[order], t[order]

results = []
for dom in DOMAINS:
    t0 = time.time()
    s, d, t = load_saga(os.path.join(DS, dom))
    s, d, t = prefix(s, d, t, K)
    n = int(np.unique(np.concatenate([s, d])).size)
    m = int(s.size)
    span = float(t.max() - t.min())
    # SAGA
    tg = run_tgn(s, d, t, epochs=6, seed=0)
    eb = run_edgebank(s, d, t, seed=0)
    # BA matched
    bs, bd, bt = gen_ba(n, m, span, 42)
    tg_ba = run_tgn(bs, bd, bt, epochs=6, seed=0)
    eb_ba = run_edgebank(bs, bd, bt, seed=0)
    row = dict(domain=dom, n_nodes=n, n_edges=m,
               tgn_saga=round(tg["test_ap"], 3), tgn_ba=round(tg_ba["test_ap"], 3),
               eb_saga=round(eb["inf_test_ap"], 3), eb_ba=round(eb_ba["inf_test_ap"], 3),
               ebtw_saga=round(eb["tw_test_ap"], 3))
    results.append(row)
    print(f"[{dom}] TGN saga={row['tgn_saga']} ba={row['tgn_ba']} | "
          f"EdgeBank saga={row['eb_saga']} ba={row['eb_ba']}  ({time.time()-t0:.0f}s)", flush=True)
    json.dump(results, open(os.path.join(DS, "..", "downstream_multi.json"), "w"), indent=2)

print("\n=== CROSS-DOMAIN DOWNSTREAM (test AP) ===")
print(f"{'domain':10s} {'TGN_saga':>9s} {'TGN_ba':>7s} {'EB_saga':>8s} {'EB_ba':>6s}")
for r in results:
    print(f"{r['domain']:10s} {r['tgn_saga']:>9.3f} {r['tgn_ba']:>7.3f} {r['eb_saga']:>8.3f} {r['eb_ba']:>6.3f}")
print("saved downstream_multi.json")
