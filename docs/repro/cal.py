import sys, os, time
SP="./scratch"
sys.path.insert(0, os.path.join(SP,"pin"))
import numpy as np
from saga_eval.load import load_saga
from saga_eval.tgn_eval import run_tgn
s,d,t = load_saga("./scratch/cal_work/crypto")
# prefix 200k
K=200000
order=np.argsort(t,kind="stable")[:K]; order=np.sort(order)
s,d,t=s[order],d[order],t[order]
print("nodes",np.unique(np.concatenate([s,d])).size,"edges",s.size,flush=True)
import torch; print("torch threads",torch.get_num_threads(),flush=True)
t0=time.time(); r=run_tgn(s,d,t,epochs=1,seed=0); print("1 epoch wall %.1fs"%(time.time()-t0),"ap",round(r["test_ap"],3),flush=True)
