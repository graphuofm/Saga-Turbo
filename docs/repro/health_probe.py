#!/usr/bin/env python3
import sys, os, json, subprocess, shutil, collections
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare

HOME = os.path.expanduser("~")
BIN = "./target/release/saga"
CAP = 1_500_000
REF = f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz"
OUT = "$WORK/health_out"

def cap_t(s, d, t, cap):
    if s.size <= cap: return s, d, t
    order = np.sort(np.argsort(t, kind="stable")[:cap])
    return s[order], d[order], t[order]

rs, rd, rt = cap_t(*load_real(REF), CAP)
n = int(np.unique(np.concatenate([rs, rd])).size)
m = int(rs.size)
span = float(rt.max() - rt.min())
rstat = compute_stats(rs, rd, rt)
print(f"REAL sx-mathoverflow: n={n} m={m} span_days={span/86400:.2f}")

shutil.rmtree(OUT, ignore_errors=True)
subprocess.run([BIN, "generate", "--domain", "healthcare", "--nodes", str(n), "--edges", str(m),
                "--time-span", str(round(span/86400.0, 2)), "--seed", "42",
                "--anomaly-rate", "0.02", "--out", OUT, "--quiet"], check=True)

ss, sd, st = cap_t(*load_saga(OUT), CAP)
sstat = compute_stats(ss, sd, st)
res = compare(rstat, sstat)
print(f"\nFIDELITY = {res['fidelity_score']:.4f}")
print("\n-- KS distances (real vs synth) --")
for k, v in res["ks_distances"].items():
    print(f"  {k:20s} KS={v:.4f}")
print("\n-- scalar metrics --")
for k, v in res["scalar_metrics"].items():
    print(f"  {k:20s} real={v['real']!s:>10.10} synth={v['synth']!s:>10.10} dist={v['distance']:.4f}")

# node participation of synth
alln = np.concatenate([ss, sd])
active = np.unique(alln).size
print(f"\nSYNTH: edges={ss.size} active_nodes={active}")

# per-event-type mix
import glob, pyarrow.parquet as papq
ev = collections.Counter()
partic = collections.defaultdict(set)
for f in sorted(glob.glob(os.path.join(OUT, "edges_*.parquet"))):
    tbl = papq.read_table(f, columns=["src","dst","event_type"])
    ets = tbl["event_type"].to_pylist()
    srcs = tbl["src"].to_numpy(); dsts = tbl["dst"].to_numpy()
    for e in ets: ev[e]+=1
tot = sum(ev.values())
print("\n-- EVENT MIX --")
for e,c in ev.most_common():
    print(f"  {e:20s} {c:>9d}  {100*c/tot:5.1f}%")

# node type participation: read entity types from node file if present
nf = glob.glob(os.path.join(OUT, "nodes*.parquet")) + glob.glob(os.path.join(OUT, "labels_nodes*.parquet"))
print("\nnode files:", [os.path.basename(x) for x in nf])
shutil.rmtree(OUT, ignore_errors=True)
