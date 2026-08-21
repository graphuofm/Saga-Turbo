#!/usr/bin/env python3
import sys, os, subprocess, shutil, glob, collections
sys.path.insert(0, "./python")
import numpy as np, pyarrow.parquet as papq
HOME=os.path.expanduser("~")
BIN="./target/release/saga"
OUT="./scratch/health_out2"
n,m,span=24818,506550,2350.34
shutil.rmtree(OUT,ignore_errors=True)
subprocess.run([BIN,"generate","--domain","healthcare","--nodes",str(n),"--edges",str(m),
    "--time-span",str(span),"--seed","42","--anomaly-rate","0.02","--out",OUT,"--quiet"],check=True)
# node id ranges per type
ranges={}
for f in glob.glob(os.path.join(OUT,"nodes_*.parquet")):
    t=papq.read_table(f)
    typ=os.path.basename(f).replace("nodes_","").replace(".parquet","")
    ids=t["id"].to_numpy() if "id" in t.column_names else t[t.column_names[0]].to_numpy()
    ranges[typ]=(int(ids.min()),int(ids.max()),ids.size)
    # attribute cols
    print(typ, "cols:", t.column_names, "count", ids.size, "idrange", ranges[typ][:2])
# active nodes per type
srcs=[];dsts=[]
for f in sorted(glob.glob(os.path.join(OUT,"edges_*.parquet"))):
    tb=papq.read_table(f,columns=["src","dst"])
    srcs.append(tb["src"].to_numpy()); dsts.append(tb["dst"].to_numpy())
S=np.concatenate(srcs);D=np.concatenate(dsts)
active=np.unique(np.concatenate([S,D]))
def typ_of(i):
    for typ,(lo,hi,c) in ranges.items():
        if lo<=i<=hi: return typ
    return "?"
print("\n-- participation by type --")
cnt=collections.Counter(typ_of(i) for i in active)
for typ,(lo,hi,c) in ranges.items():
    print(f"  {typ:10s} total={c:6d} active={cnt.get(typ,0):6d}  ({100*cnt.get(typ,0)/c:5.1f}%)")
# provider specialty participation
pf=os.path.join(OUT,"nodes_provider.parquet")
t=papq.read_table(pf)
pid=t["id"].to_numpy() if "id" in t.column_names else t[t.column_names[0]].to_numpy()
spec=None
for c in t.column_names:
    if "special" in c: spec=t[c].to_pylist()
if spec:
    actset=set(active.tolist())
    byspec_tot=collections.Counter(spec)
    byspec_act=collections.Counter(s for i,s in zip(pid.tolist(),spec) if i in actset)
    print("\n-- provider specialty participation --")
    for s in byspec_tot:
        print(f"  {str(s):16s} total={byspec_tot[s]:5d} active={byspec_act.get(s,0):5d} ({100*byspec_act.get(s,0)/byspec_tot[s]:5.1f}%)")
shutil.rmtree(OUT,ignore_errors=True)
