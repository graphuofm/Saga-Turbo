import sys, time
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.stats import compute_stats
rng = np.random.default_rng(0)
n, m = 986, 332334
# dense random graph like TIGGER output
s = rng.integers(0, n, m).astype(np.int64)
d = rng.integers(0, n, m).astype(np.int64)
t = np.sort(rng.integers(0, 69459254, m)).astype(np.float64)
t0=time.time(); st=compute_stats(s,d,t); print("dense-random compute_stats %.1fs clustering=%.3f"%(time.time()-t0, st.clustering), flush=True)
