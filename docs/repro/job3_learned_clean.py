#!/usr/bin/env python3
"""Learned-generator contest (CLEAN re-measurement, quiet host).

Same protocol as job3_learned_after.py. Changes vs that file:
  - BIN -> ./target/release/saga (the fixed-engine binary named in the task).
  - per-step wall-clock logging for EVERY generator and EVERY fscore/compute_stats.
  - a per-call SIGALRM timeout around each generator AND each fscore, so one stuck
    step (the previous run hung ~15h with no cap) is reported+skipped as nan
    rather than hanging the whole job.

TIGGER-Lite / CTGAN / TVAE vs SAGA and two references (BA, config), each real graph
capped to ~2500 nodes: a controlled size where every method can run. This IS the
finding: SAGA + structural generators run at real scale; learned generators only at
toy scale, here.
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
SP = "./scratch/lrn_after"
BIN = "./target/release/saga"
NODE_CAP = 2500
GEN_TIMEOUT = 2400   # per learned generator (TIGGER/CTGAN/TVAE); prev run hung with no cap
STAT_TIMEOUT = 900   # per fscore/compute_stats (dense-random calibrated at ~6s)
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


PAIRS = [
    ("finance",   "email-Eu",    f"./realdata/snap-email-eu/email-Eu-core-temporal.txt", {}),
    ("crypto",    "wiki-talk",   f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}),
    ("cyber",     "sx-superuser",f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}),
    ("ecommerce", "tgbl-review", f"./realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
                    dict(src_col="source", dst_col="target", t_col="ts")),
    ("transport", "sx-askubuntu",f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}),
    ("healthcare","sx-mathoverflow",f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}),
]

def cap_by_nodes(s, d, t, cap):
    order = np.argsort(t, kind="stable")
    s, d, t = s[order], d[order], t[order]
    both = np.empty(2 * s.size, dtype=np.int64)
    both[0::2] = s; both[1::2] = d
    _, first = np.unique(both, return_index=True)
    is_new = np.zeros(both.size, bool); is_new[first] = True
    edge_nodes = np.cumsum(is_new)[1::2]
    cut = min(int(np.searchsorted(edge_nodes, cap)) + 1, s.size)
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

matrix = {}
for domain, rname, path, kw in PAIRS:
    t0 = time.time()
    rs, rd, rt = cap_by_nodes(*load_real(path, **kw), NODE_CAP)
    real = (rs, rd, rt)
    n = int(np.unique(np.concatenate([rs, rd])).size)
    m = int(rs.size)
    span = float(rt.max() - rt.min())
    ts = time.time()
    rstat = compute_stats(rs, rd, rt)
    print(f"[{domain}] real={rname} n={n} m={m} rstat={time.time()-ts:.1f}s", flush=True)
    row = {"_real": rname, "_nodes": n, "_edges": m}
    # SAGA matched
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
    # learned
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
    # references
    ts = time.time(); row["BA"] = fscore(rstat, bl.gen_ba(n, m, span, 42), "BA")
    row["config"] = fscore(rstat, bl.gen_config(real, span, 42), "config")
    print(f"   BA fid={row['BA']} config fid={row['config']} ({time.time()-ts:.1f}s)", flush=True)
    matrix[domain] = row
    json.dump(matrix, open(os.path.join(SP, "..", "learned_clean.json"), "w"), indent=2, default=float)
    print(f"[{domain}] DONE n={n} m={m}  SAGA={row['SAGA']} TIGGER={row.get('TIGGER')} "
          f"CTGAN={row.get('CTGAN')} TVAE={row.get('TVAE')} BA={row['BA']} config={row['config']} "
          f"({time.time()-t0:.0f}s)", flush=True)

print("\n=== LEARNED-GENERATOR MATRIX (fid, ~2500-node cap) ===")
ORDER = ["SAGA", "TIGGER", "CTGAN", "TVAE", "config", "BA"]
print(f"{'method':10s}" + "".join(f"{d[:7]:>9s}" for d, *_ in PAIRS))
for g in ORDER:
    print(f"{g:10s}" + "".join(f"{matrix[d].get(g, float('nan')):>9.3f}" for d, *_ in PAIRS))
print("saved learned_clean.json")
