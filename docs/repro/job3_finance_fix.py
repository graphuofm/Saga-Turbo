#!/usr/bin/env python3
"""Finance-only re-run of the learned-generator contest with the CAP FIX (P1-b).

Root cause: the original cap_by_nodes() caps by cumulative distinct-node count
reaching NODE_CAP=2500. Finance's reference email-Eu-core-temporal has only 986
nodes TOTAL (< 2500), so the node cap NEVER binds and all 332,334 edges are kept
- a tiny-but-dense graph. CTGAN/TVAE then get a 332k-row table (~5x their tuned
budget) and time out at 2400s -> nan.

Fix: cap = min(node-cut@2500, edge-cut@EDGE_CAP). EDGE_CAP=50000 is a NO-OP for
the five sparse references (all realized <= 49,200 edges when node-capped) and
only reshapes the dense finance reference to a comparable toy budget.

Protocol otherwise IDENTICAL to job3_learned_clean.py: same NODE_CAP, same
generators, seed 42, same fidelity metric, per-step SIGALRM timeouts. Runs finance
ONLY; the other five rows are kept from the prior clean run.
"""
import sys, os, json, time, subprocess, shutil, signal
from contextlib import contextmanager
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
from saga_eval import baselines as bl
from saga_eval.deep_gen import fit_generate
from saga_eval.tabular_gen import ctgan_generate, tvae_generate

HOME = os.path.expanduser("~")
SP = "$WORK/fin_fix"
BIN = "./target/release/saga"
NODE_CAP = 2500
EDGE_CAP = 50000     # NEW: no-op for the five sparse refs (<=49200); binds only for dense finance
GEN_TIMEOUT = 2400
STAT_TIMEOUT = 900
os.makedirs(SP, exist_ok=True)


class StepTimeout(Exception):
    pass


@contextmanager
def time_limit(seconds, label):
    def _h(signum, frame):
        raise StepTimeout(f"{label} exceeded {seconds}s")
    old = signal.signal(signal.SIGALRM, _h)
    signal.alarm(seconds)
    try:
        yield
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, old)


def cap_by_nodes(s, d, t, cap, edge_cap=None):
    """Chronological cut at the earlier of: cumulative distinct nodes == cap, OR
    edge_cap edges. The edge_cap arm makes the toy budget bind for a dense
    reference whose total node count never reaches `cap` (fixes finance P1-b)."""
    order = np.argsort(t, kind="stable")
    s, d, t = s[order], d[order], t[order]
    both = np.empty(2 * s.size, dtype=np.int64)
    both[0::2] = s; both[1::2] = d
    _, first = np.unique(both, return_index=True)
    is_new = np.zeros(both.size, bool); is_new[first] = True
    edge_nodes = np.cumsum(is_new)[1::2]
    cut = min(int(np.searchsorted(edge_nodes, cap)) + 1, s.size)
    if edge_cap is not None:
        cut = min(cut, int(edge_cap))
    return s[:cut], d[:cut], t[:cut]


def fscore(rstat, arr, label):
    s, d, t = arr
    if s is None or len(s) == 0:
        return float("nan")
    try:
        with time_limit(STAT_TIMEOUT, "fscore:" + label):
            return round(compare(rstat, compute_stats(s, d, t))["fidelity_score"], 3)
    except StepTimeout as e:
        print("   fscore TIMEOUT", e, flush=True); return float("nan")
    except Exception as e:
        print("   fscore err", e, flush=True); return float("nan")


domain, rname, path, kw = ("finance", "email-Eu",
    f"{HOME}/saga-sigmod/realdata/snap-email-eu/email-Eu-core-temporal.txt", {})

t0 = time.time()
rs, rd, rt = cap_by_nodes(*load_real(path, **kw), NODE_CAP, EDGE_CAP)
real = (rs, rd, rt)
n = int(np.unique(np.concatenate([rs, rd])).size)
m = int(rs.size)
span = float(rt.max() - rt.min())
rstat = compute_stats(rs, rd, rt)
print(f"[{domain}] real={rname} CAPPED n={n} m={m} span_days={span/86400:.1f}", flush=True)
row = {"_real": rname, "_nodes": n, "_edges": m, "_cap_note": "min(2500 nodes, 50000 edges)"}

# SAGA matched to the capped reference
outdir = os.path.join(SP, domain)
shutil.rmtree(outdir, ignore_errors=True)
ts = time.time()
subprocess.run([BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
                "--time-span", str(round(span/86400.0, 3)), "--seed", "42",
                "--anomaly-rate", "0.02", "--out", outdir, "--quiet"],
               check=True, stderr=subprocess.DEVNULL)
row["SAGA"] = fscore(rstat, cap_by_nodes(*load_saga(outdir), 10**9), "SAGA")
print(f"   SAGA fid={row['SAGA']} ({time.time()-ts:.1f}s)", flush=True)
shutil.rmtree(outdir, ignore_errors=True)

for name, fn in [("TIGGER", fit_generate), ("CTGAN", ctgan_generate), ("TVAE", tvae_generate)]:
    tt = time.time()
    try:
        with time_limit(GEN_TIMEOUT, "gen:" + name):
            arr = fn(rs, rd, rt, n, m, span, 42)
        row[name + "_gen_s"] = round(time.time() - tt, 0)
        row[name] = fscore(rstat, arr, name)
    except StepTimeout as e:
        print(f"   {name} GEN TIMEOUT after {GEN_TIMEOUT}s", flush=True)
        row[name] = float("nan"); row[name + "_gen_s"] = f">{GEN_TIMEOUT}(timeout)"
    except Exception as e:
        print(f"   {name} err {type(e).__name__}: {e}", flush=True)
        row[name] = float("nan"); row[name + "_gen_s"] = round(time.time() - tt, 0)
    row[name + "_s"] = round(time.time() - tt, 0)
    print(f"   {name} fid={row.get(name)} total={row[name+'_s']}s", flush=True)

ts = time.time(); row["BA"] = fscore(rstat, bl.gen_ba(n, m, span, 42), "BA")
row["config"] = fscore(rstat, bl.gen_config(real, span, 42), "config")
print(f"   BA fid={row['BA']} config fid={row['config']} ({time.time()-ts:.1f}s)", flush=True)

json.dump(row, open(os.path.join(SP, "finance_fixed.json"), "w"), indent=2, default=float)
print(f"[{domain}] DONE n={n} m={m}  SAGA={row['SAGA']} TIGGER={row.get('TIGGER')} "
      f"CTGAN={row.get('CTGAN')} TVAE={row.get('TVAE')} BA={row['BA']} config={row['config']} "
      f"({time.time()-t0:.0f}s)", flush=True)
