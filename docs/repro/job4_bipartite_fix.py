"""JOB 4 addendum — re-run the two BIPARTITE datasets with the user/item ID
namespace collision FIXED, to quantify how much the harness bug moves the axes.

Bug: tgbl-review and tgbl-wiki store user ids and item ids in SEPARATE namespaces
that both start at 0. load.load_real() maps raw ids directly, so user X and item X
collapse into ONE node. Fix: offset dst ids by (max_src_id + 1).
"""
import json, shutil, subprocess, sys, time
import numpy as np

sys.path.insert(0, "./python")
from saga_eval import load, stats as st, metrics_advanced as adv

REPO = "./"
SAGA = f"{REPO}/target/release/saga"
SP = "./scratch"
CAP, SEED = 2_500_000, 42

DATASETS = [
    ("tgbl-review", f"{REPO}/realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
     dict(src_col=1, dst_col=2, t_col=0, has_header=True), "ecommerce"),
    ("tgbl-wiki", f"{REPO}/realdata/tgbl-wiki/tgbl-wiki_edgelist_v2.csv",
     dict(src_col=0, dst_col=1, t_col=2, has_header=True), "ecommerce"),
]

out_all = {}
for name, path, kw, domain in DATASETS:
    print(f"\n{'='*70}\n{name} (NAMESPACE-FIXED)\n{'='*70}", flush=True)
    rsrc, rdst, rt = load.load_real(path, **kw)
    if rsrc.size > CAP:
        o = np.sort(np.argsort(rt, kind="stable")[:CAP])
        rsrc, rdst, rt = rsrc[o], rdst[o], rt[o]

    nodes_buggy = int(np.unique(np.concatenate([rsrc, rdst])).size)
    offset = int(rsrc.max()) + 1
    rdst_fixed = rdst + offset          # disjoint namespaces
    nodes_fixed = int(np.unique(np.concatenate([rsrc, rdst_fixed])).size)
    print(f"nodes as-loaded (BUGGY, merged): {nodes_buggy:,}")
    print(f"nodes namespace-fixed          : {nodes_fixed:,}  (+{nodes_fixed-nodes_buggy:,})", flush=True)

    n_edges = int(rsrc.size)
    span_days = float((rt.max() - rt.min()) / 86400.0)
    out = f"/tmp/j4fix_{name}"
    shutil.rmtree(out, ignore_errors=True)
    cmd = [SAGA, "generate", "--domain", domain, "--nodes", str(nodes_fixed),
           "--edges", str(n_edges), "--time-span", f"{max(span_days,1e-3):.6f}",
           "--no-anomalies", "--seed", str(SEED), "--out", out]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        print("!! GENERATE FAILED\n", p.stderr, flush=True)
        out_all[name] = {"error": p.stderr.strip()}
        continue
    ssrc, sdst, stt = load.load_saga(out)
    rstats = st.compute_stats(rsrc, rdst_fixed, rt)
    sstats = st.compute_stats(ssrc, sdst, stt)
    rep = adv.advanced_report((rsrc, rdst_fixed, rt), (ssrc, sdst, stt), rstats, sstats)
    print(adv.format_advanced(rep), flush=True)
    out_all[name] = {
        "nodes_buggy_merged": nodes_buggy, "nodes_namespace_fixed": nodes_fixed,
        "real": {"edges": n_edges, "span_days": span_days},
        "synth": {"edges": int(ssrc.size),
                  "nodes": int(np.unique(np.concatenate([ssrc, sdst])).size)},
        "advanced": rep, "cmd": cmd,
    }
    shutil.rmtree(out, ignore_errors=True)
    print(f"deleted {out}", flush=True)

with open(f"{SP}/advanced_axes_bipartite_fixed.json", "w") as fh:
    json.dump(out_all, fh, indent=2, default=lambda o: None)
print(f"\nwrote {SP}/advanced_axes_bipartite_fixed.json")
