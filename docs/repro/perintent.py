import json,sys,glob,os,collections
import numpy as np
sys.path.insert(0,"./python")
sys.path.insert(0,os.path.dirname(os.path.abspath(__file__)))
from anom import build_adj, local_clustering
import pyarrow.parquet as papq
d=sys.argv[1]
srcs,dsts=[],[]
for f in sorted(glob.glob(os.path.join(d,"edges_*.parquet"))):
    t=papq.read_table(f,columns=["src","dst"])
    srcs.append(t["src"].to_numpy()); dsts.append(t["dst"].to_numpy())
src=np.concatenate(srcs).astype(np.int64); dst=np.concatenate(dsts).astype(np.int64)
n=int(max(src.max(),dst.max()))+1
und=build_adj(src,dst,n)
gt=json.load(open(os.path.join(d,"ground_truth.json")))
recs = gt["instances"]
by=collections.defaultdict(list); ring=collections.defaultdict(list)
for r in recs:
    by[r["intent"]] += r["members"]; ring[r["intent"]].append(r["n_members"])
rng=np.random.default_rng(0)
out_deg=np.bincount(src,minlength=n); in_deg=np.bincount(dst,minlength=n)
print(f"{'intent':<16}{'nodes':>7}{'clust':>10}{'meandeg':>9}{'in/out':>8}{'meanring':>9}")
for k,v in sorted(by.items()):
    v=sorted(set(v)); c,_=local_clustering(und,v,rng)
    md=float(np.mean((out_deg+in_deg)[v])); mi=float(np.mean(in_deg[v])); mo=float(np.mean(out_deg[v]))
    print(f"{k:<16}{len(v):>7}{c:>10.4f}{md:>9.2f}{(mi/mo if mo else float('nan')):>8.3f}{np.mean(ring[k]):>9.1f}")
