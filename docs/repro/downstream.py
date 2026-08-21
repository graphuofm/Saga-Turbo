"""Downstream re-measurement: TGN + EdgeBank on real CollegeMsg / SAGA / BA,
plus repeated-edge fraction. Reuses the frozen harness functions."""
import sys, os
sys.path.insert(0, "./python")
import numpy as np
from saga_eval import load
from saga_eval import baselines as bl
from saga_eval.tgn_eval import run_tgn
from saga_eval.downstream_extra import run_edgebank

REAL = "./realdata/snap-collegemsg/CollegeMsg.txt"
SAGA_DIR = sys.argv[1]

def repeat_frac(s, d):
    n = s.size
    pairs = np.stack([s, d], axis=1)
    distinct = np.unique(pairs, axis=0).shape[0]
    return 1.0 - distinct / n if n else 0.0

rs, rd, rt = load.load_real(REAL)
ss, sd, st = load.load_saga(SAGA_DIR)
span = float(rt.max() - rt.min())
n = int(np.unique(np.concatenate([rs, rd])).size)
m = int(rs.size)
bs, bd, bt = bl.gen_ba(n, m, span, 42)

ds = {"real": (rs, rd, rt), "SAGA": (ss, sd, st), "BA": (bs, bd, bt)}
for name, (s, d, t) in ds.items():
    print(f"[{name}] edges={s.size} nodes={np.unique(np.concatenate([s,d])).size} "
          f"repeat_frac={repeat_frac(s,d):.4f}", flush=True)

tgn, eb = {}, {}
for name, (s, d, t) in ds.items():
    print(f"\n### TGN on {name} ###", flush=True)
    tgn[name] = run_tgn(s, d, t, epochs=6, seed=0)
    print(f"\n### EdgeBank on {name} ###", flush=True)
    eb[name] = run_edgebank(s, d, t, seed=0)

print("\n\n=========== DOWNSTREAM RESULTS (test AP) ===========")
print(f"{'model':<26}{'real':>10}{'SAGA':>10}{'BA':>10}")
print(f"{'TGN (learned) AP':<26}" + "".join(f"{tgn[c]['test_ap']:>10.3f}" for c in ('real','SAGA','BA')))
print(f"{'TGN AUC':<26}" + "".join(f"{tgn[c]['test_auc']:>10.3f}" for c in ('real','SAGA','BA')))
print(f"{'EdgeBank_inf AP':<26}" + "".join(f"{eb[c]['inf_test_ap']:>10.3f}" for c in ('real','SAGA','BA')))
print(f"{'EdgeBank_inf AUC':<26}" + "".join(f"{eb[c]['inf_test_auc']:>10.3f}" for c in ('real','SAGA','BA')))
print(f"{'EdgeBank_tw AP':<26}" + "".join(f"{eb[c]['tw_test_ap']:>10.3f}" for c in ('real','SAGA','BA')))
print(f"{'repeated-edge frac':<26}" + "".join(f"{repeat_frac(*ds[c][:2]):>10.3f}" for c in ('real','SAGA','BA')))
print("DONE_DOWNSTREAM")
