"""Anomaly-subgraph fidelity AFTER the fix — same code path as the paper (saga_eval.compare),
same subgraph definitions as the pre-fix run (incident = either endpoint in pop; internal = both)."""
import sys, json, csv, glob, os
sys.path.insert(0,"./python")
import numpy as np
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
sys.path.insert(0,"$WORK")
from anom import load_elliptic, load_saga
S="$WORK"

def sub(src,dst,t,lab,mode):
    m=np.isin(src,np.nonzero(lab==1)[0]); m2=np.isin(dst,np.nonzero(lab==1)[0])
    k=(m|m2) if mode=="incident" else (m&m2)
    return src[k],dst[k],t[k]

es,ed,et,elab,_,en=load_elliptic()
out={}
for tag,d in [("cfgA_matched","saga_after_matched"),("cfgB_1e7","probe_10000000")]:
    ss,sd,st_,slab,_,sn=load_saga(f"{S}/{d}")
    for mode in ["incident","internal"]:
        rs,rd,rt=sub(es,ed,et,elab,mode)
        ys,yd,yt=sub(ss,sd,st_,slab,mode)
        rec={"real_n_edges":int(rs.size),"synth_n_edges":int(ys.size)}
        if ys.size<5 or rs.size<5:
            rec["fidelity_score"]=None; rec["note"]="too few edges to compute"
        else:
            c=compare(compute_stats(rs,rd,rt,seed=0),compute_stats(ys,yd,yt,seed=0))
            rec.update({"fidelity_score":c["fidelity_score"],"ks_distances":c["ks_distances"],
                        "scalar_metrics":c["scalar_metrics"]})
        out[f"{tag}_{mode}"]=rec
        print(tag,mode,"real_edges",rs.size,"synth_edges",ys.size,"fid",rec.get("fidelity_score"))
    # whole graph
    c=compare(compute_stats(es,ed,et,seed=0),compute_stats(ss,sd,st_,seed=0))
    out[f"{tag}_wholegraph"]={"fidelity_score":c["fidelity_score"],"ks_distances":c["ks_distances"],
                              "scalar_metrics":c["scalar_metrics"],
                              "real_n_edges":int(es.size),"synth_n_edges":int(ss.size)}
    print(tag,"wholegraph fid",c["fidelity_score"])
json.dump(out,open(f"{S}/anom_subgraph_fidelity_after.json","w"),indent=2,default=float)
