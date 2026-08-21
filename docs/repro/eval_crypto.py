import numpy as np
from saga_eval.stats import compute_stats
from saga_eval.tgn_eval import run_tgn

path = "/tmp/saga_crypto/edges_00000.csv"
data = np.genfromtxt(path, delimiter=",", skip_header=1, usecols=(0, 1, 2))
src = data[:, 0].astype(np.int64)
dst = data[:, 1].astype(np.int64)
t = data[:, 2].astype(np.int64)
print(f"loaded {src.size} edges")

st = compute_stats(src, dst, t)
print("stats done")

res = run_tgn(src, dst, t, epochs=6)
print("tgn done")

print("RESULT_ROW", "|".join([
    "domain=crypto",
    f"nodes={st.n_nodes}",
    f"edges={st.n_edges}",
    f"mean_degree={st.mean_degree:.3f}",
    f"clustering={st.clustering:.4f}" if st.clustering is not None else "clustering=NA",
    f"burstiness={st.burstiness_b:.4f}" if st.burstiness_b is not None else "burstiness=NA",
    f"powerlaw_alpha={st.powerlaw_alpha:.3f}" if st.powerlaw_alpha is not None else "powerlaw_alpha=NA",
    f"repeat_edge_ratio={st.repeat_edge_ratio:.4f}",
    f"TGN_test_AP={res['test_ap']:.4f}",
    f"TGN_test_AUC={res['test_auc']:.4f}",
]))
