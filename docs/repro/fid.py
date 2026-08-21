#!/usr/bin/env python3
import sys, os, subprocess, shutil, glob, collections
sys.path.insert(0,"./python")
import numpy as np, pyarrow.parquet as papq
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
HOME=os.path.expanduser("~"); BIN="./target/release/saga"; CAP=1_500_000
REF=f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz"
DOM=sys.argv[1] if len(sys.argv)>1 else "healthcare"
OUT="./scratch/fid_out"
def cap_t(s,d,t,cap):
    if s.size<=cap: return s,d,t
    o=np.sort(np.argsort(t,kind="stable")[:cap]); return s[o],d[o],t[o]
rs,rd,rt=cap_t(*load_real(REF),CAP)
n=int(np.unique(np.concatenate([rs,rd])).size); m=int(rs.size); span=float(rt.max()-rt.min())
rstat=compute_stats(rs,rd,rt)
shutil.rmtree(OUT,ignore_errors=True)
subprocess.run([BIN,"generate","--domain",DOM,"--nodes",str(n),"--edges",str(m),
    "--time-span",str(round(span/86400.0,2)),"--seed","42","--anomaly-rate","0.02",
    "--out",OUT,"--quiet"],check=True)
ss,sd,st=cap_t(*load_saga(OUT),CAP); sstat=compute_stats(ss,sd,st)
res=compare(rstat,sstat)
print(f"FIDELITY = {res['fidelity_score']:.4f}   (synth edges={ss.size} active_nodes={np.unique(np.concatenate([ss,sd])).size} / real n={n})")
for k,v in res["ks_distances"].items(): print(f"  KS {k:18s} {v:.4f}")
for k,v in res["scalar_metrics"].items():
    print(f"  SC {k:18s} real={v['real']:>8.3f} synth={v['synth']:>8.3f} dist={v['distance']:.4f}")
ev=collections.Counter()
for f in sorted(glob.glob(os.path.join(OUT,"edges_*.parquet"))):
    for e in papq.read_table(f,columns=["event_type"])["event_type"].to_pylist(): ev[e]+=1
tot=sum(ev.values())
print("  MIX "+" ".join(f"{e}={100*c/tot:.0f}%" for e,c in ev.most_common()))
shutil.rmtree(OUT,ignore_errors=True)
