import numpy as np, pandas as pd
from saga_eval.stats import compute_stats
from saga_eval.compare import compare, format_scorecard
from saga_eval.tgn_eval import run_tgn

# --- REAL (rescaled to nominal 1-day span for unit-comparability) ---
z = np.load('/tmp/claude-real-cyber.npz')
rs, rd, rt = z['src'].astype(np.int64), z['dst'].astype(np.int64), z['t'].astype(float)
rt = (rt - rt.min())/(rt.max()-rt.min()) * 86400.0  # monotonic rescale to 1 day
rt = rt.astype(np.int64)

# --- SAGA ---
sd = pd.read_csv('/tmp/saga_cyber/edges_00000.csv', usecols=['src','dst','t'])
ss, sdst, st = sd['src'].to_numpy(np.int64), sd['dst'].to_numpy(np.int64), sd['t'].to_numpy(np.int64)
st = st - st.min()

print("=== FIDELITY ===")
real_stats = compute_stats(rs, rd, rt.astype(float))
saga_stats = compute_stats(ss, sdst, st.astype(float))
res = compare(real_stats, saga_stats)
print(format_scorecard(res, "CICIDS2017-benign", "SAGA-cyber"))
fid = res['fidelity_score']

print("\n=== TGN real ===")
tg_real = run_tgn(rs, rd, rt, epochs=6)
print("=== TGN saga ===")
tg_saga = run_tgn(ss, sdst, st, epochs=6)

print("\n\n================ SUMMARY TABLE ================")
print(f"{'graph':<22}{'nodes':>7}{'edges':>8}{'fidelity':>10}{'TGN AP':>9}{'TGN AUC':>9}")
print(f"{'REAL CICIDS-benign':<22}{real_stats.n_nodes:>7}{real_stats.n_edges:>8}{'—':>10}{tg_real['test_ap']:>9.3f}{tg_real['test_auc']:>9.3f}")
print(f"{'SAGA-cyber':<22}{saga_stats.n_nodes:>7}{saga_stats.n_edges:>8}{fid:>10.3f}{tg_saga['test_ap']:>9.3f}{tg_saga['test_auc']:>9.3f}")
print(f"\nfidelity(SAGA vs REAL) = {fid:.3f}")
print(f"real top-node degree concentration vs saga: real recip={real_stats.reciprocity:.3f} saga recip={saga_stats.reciprocity:.3f}")
