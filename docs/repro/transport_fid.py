import sys,os,subprocess,shutil
sys.path.insert(0,"./python")
import numpy as np
from saga_eval.load import load_real,load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
HOME=os.path.expanduser("~");BIN="./target/release/saga";CAP=1_500_000
REF=f"{HOME}/.cache/zord/sx-askubuntu.txt.gz"
def cap_t(s,d,t,c):
    if s.size<=c:return s,d,t
    o=np.sort(np.argsort(t,kind="stable")[:c]);return s[o],d[o],t[o]
rs,rd,rt=cap_t(*load_real(REF),CAP)
n=int(np.unique(np.concatenate([rs,rd])).size);m=int(rs.size);span=float(rt.max()-rt.min())
rstat=compute_stats(rs,rd,rt)
OUT="./scratch/tr_out"
shutil.rmtree(OUT,ignore_errors=True)
subprocess.run([BIN,"generate","--domain","transport","--nodes",str(n),"--edges",str(m),
 "--time-span",str(round(span/86400,2)),"--seed","42","--anomaly-rate","0.02","--out",OUT,"--quiet"],check=True)
ss,sd,st=cap_t(*load_saga(OUT),CAP)
print(f"TRANSPORT fidelity = {compare(rstat,compute_stats(ss,sd,st))['fidelity_score']:.4f}")
shutil.rmtree(OUT,ignore_errors=True)
