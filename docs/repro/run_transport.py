import numpy as np
from saga_eval.stats import compute_stats
from saga_eval.tgn_eval import run_tgn

path = "/tmp/saga_transport/edges_00000.csv"
arr = np.genfromtxt(path, delimiter=",", skip_header=1, usecols=(0, 1, 2))
src = arr[:, 0].astype(np.int64)
dst = arr[:, 1].astype(np.int64)
t = arr[:, 2].astype(np.int64)
print(f"loaded {src.size} edges", flush=True)

st = compute_stats(src, dst, t)
print("STATS_DONE", flush=True)

res = run_tgn(src, dst, t, epochs=6)
print("TGN_DONE", flush=True)

print("RESULT_LINE")
print(f"domain=transport | nodes={st.n_nodes} | edges={st.n_edges} | "
      f"mean_degree={st.mean_degree:.3f} | clustering={st.clustering:.4f} | "
      f"burstiness={st.burstiness_b:.4f} | powerlaw_alpha={st.powerlaw_alpha} | "
      f"repeat_edge_ratio={st.repeat_edge_ratio:.4f} | "
      f"TGN_test_AP={res['test_ap']:.4f} | TGN_test_AUC={res['test_auc']:.4f}")
