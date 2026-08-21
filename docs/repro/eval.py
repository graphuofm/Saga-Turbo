import numpy as np
from saga_eval.stats import compute_stats
from saga_eval.compare import compare
from saga_eval.tgn_eval import run_tgn

SCRATCH = "./scratch"

# --- REAL ---
real = np.load(f"{SCRATCH}/real.npy")
r_src, r_dst, r_t = real[:, 0].astype(np.int64), real[:, 1].astype(np.int64), real[:, 2].astype(np.int64)

# --- SAGA ecommerce ---
saga = np.loadtxt("/tmp/saga_ecom/edges_00000.csv", delimiter=",", skiprows=1,
                  usecols=(0, 1, 2), dtype=np.int64)
s_src, s_dst, s_t = saga[:, 0], saga[:, 1], saga[:, 2]

print(f"REAL : {r_src.size} edges, {np.unique(np.concatenate([r_src,r_dst])).size} nodes, span_days={(r_t.max()-r_t.min())/86400:.2f}")
print(f"SAGA : {s_src.size} edges, {np.unique(np.concatenate([s_src,s_dst])).size} nodes, span_days={(s_t.max()-s_t.min())/86400:.2f}")

# --- FIDELITY ---
print("\n[fidelity] computing stats...")
rs = compute_stats(r_src, r_dst, r_t)
ss_ = compute_stats(s_src, s_dst, s_t)
cmp = compare(rs, ss_)
fidelity = cmp["fidelity_score"]
print(f"FIDELITY = {fidelity:.4f}")
print("KS distances:", {k: (round(v, 3) if v == v else None) for k, v in cmp["ks_distances"].items()})
print("scalars:")
for k, v in cmp["scalar_metrics"].items():
    print(f"  {k:<18} real={v['real']} synth={v['synth']} dist={v['distance']}")

# --- TGN ---
print("\n[TGN] REAL (6 epochs)")
tgn_real = run_tgn(r_src, r_dst, r_t, epochs=6)
print("\n[TGN] SAGA (6 epochs)")
tgn_saga = run_tgn(s_src, s_dst, s_t, epochs=6)

print("\n================ RESULTS ================")
print(f"{'graph':<18}{'fidelity':>10}{'TGN_AP':>10}{'TGN_AUC':>10}")
print(f"{'REAL amazon':<18}{'--':>10}{tgn_real['test_ap']:>10.4f}{tgn_real['test_auc']:>10.4f}")
print(f"{'SAGA ecommerce':<18}{fidelity:>10.4f}{tgn_saga['test_ap']:>10.4f}{tgn_saga['test_auc']:>10.4f}")
print(f"\nfidelity(real vs SAGA) = {fidelity:.4f}")
