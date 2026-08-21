import numpy as np
from saga_eval import load, stats

s, d, t = load.load_real("$PROJECT/saga/realdata/snap-collegemsg/CollegeMsg.txt")
st = stats.compute_stats(s, d, t)
n = int(s.size)
dir_pairs = np.unique(np.stack([s, d], 1), axis=0).shape[0]
und = np.unique(np.sort(np.stack([s, d], 1), axis=1), axis=0).shape[0]
print("REAL CollegeMsg")
print("  harness stats.compute_stats.repeat_edge_ratio =", st.repeat_edge_ratio)
print("  my inline (directed distinct pairs)           =", 1 - dir_pairs / n)
print("  undirected variant                            =", 1 - und / n)
print("  n_edges", n, "distinct_directed", dir_pairs, "distinct_undirected", und)
