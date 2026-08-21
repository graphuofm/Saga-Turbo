"""JOB 4 — the five advanced evaluation axes, SAGA vs each real graph.

Matching protocol copied from docs/VALIDATION.md ("SAGA graphs were scale-matched
(same edge/node/time budget) and generated with anomalies off") and docs/EXPERIMENTS.md
sec.9 ("Match SAGA to each real graph's (n, m, span); cap 2.5M edges (time prefix)").
Domain mapping is the one the docs actually used.
"""
import json, os, shutil, subprocess, sys, time
import numpy as np

sys.path.insert(0, "./python")
from saga_eval import load, stats as st, metrics_advanced as adv

REPO = "./"
SAGA = f"{REPO}/target/release/saga"
SP = "./scratch"
CAP = 2_500_000  # docs/EXPERIMENTS.md sec.9 time-prefix cap
SEED = 42

DATASETS = [
    # name, path, loader kwargs, saga domain, doc precedent for the domain choice
    ("snap-email-eu", f"{REPO}/realdata/snap-email-eu/email-Eu-core-temporal.txt",
     dict(src_col=0, dst_col=1, t_col=2, has_header=False), "finance",
     "docs/VALIDATION.md 'SAGA finance vs email-Eu'"),
    ("snap-collegemsg", f"{REPO}/realdata/snap-collegemsg/CollegeMsg.txt",
     dict(src_col=0, dst_col=1, t_col=2, has_header=False), "finance",
     "docs/VALIDATION.md 'SAGA finance vs CollegeMsg'"),
    ("tgbl-review", f"{REPO}/realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
     dict(src_col=1, dst_col=2, t_col=0, has_header=True), "ecommerce",
     "docs/VALIDATION.md 'SAGA ecommerce vs tgbl-review'"),
    ("tgbl-wiki", f"{REPO}/realdata/tgbl-wiki/tgbl-wiki_edgelist_v2.csv",
     dict(src_col=0, dst_col=1, t_col=2, has_header=True), "ecommerce",
     "NO doc precedent; realdata/INDEX.txt calls it 'ECOMMERCE-like: bipartite user->item'"),
]


def time_prefix(src, dst, t, cap):
    """Cap to `cap` edges by taking the earliest-in-time prefix (docs sec.9)."""
    if src.size <= cap:
        return src, dst, t, False
    order = np.argsort(t, kind="stable")[:cap]
    order = np.sort(order)
    return src[order], dst[order], t[order], True


results = {}
for name, path, kw, domain, precedent in DATASETS:
    print(f"\n{'='*70}\n{name}  (domain={domain})\n{'='*70}", flush=True)
    rsrc, rdst, rt = load.load_real(path, **kw)
    rsrc, rdst, rt, capped = time_prefix(rsrc, rdst, rt, CAP)
    n_edges = int(rsrc.size)
    n_nodes = int(np.unique(np.concatenate([rsrc, rdst])).size)
    span_days = float((rt.max() - rt.min()) / 86400.0)
    print(f"real: {n_edges:,} edges  {n_nodes:,} nodes  span {span_days:.3f} d  capped={capped}", flush=True)

    out = f"/tmp/j4_{name}"
    shutil.rmtree(out, ignore_errors=True)
    cmd = [SAGA, "generate", "--domain", domain, "--nodes", str(n_nodes),
           "--edges", str(n_edges), "--time-span", f"{max(span_days, 1e-3):.6f}",
           "--no-anomalies", "--seed", str(SEED), "--out", out]
    print("gen:", " ".join(cmd), flush=True)
    t0 = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        print(f"!! GENERATE FAILED rc={p.returncode}\nSTDOUT:{p.stdout}\nSTDERR:{p.stderr}", flush=True)
        results[name] = {"error": "generate failed", "rc": p.returncode,
                         "stderr": p.stderr.strip(), "stdout": p.stdout.strip(), "cmd": cmd}
        continue
    print(f"gen ok in {time.time()-t0:.1f}s", flush=True)

    ssrc, sdst, stt = load.load_saga(out)
    print(f"synth: {ssrc.size:,} edges  {np.unique(np.concatenate([ssrc,sdst])).size:,} nodes", flush=True)

    rstats = st.compute_stats(rsrc, rdst, rt)
    sstats = st.compute_stats(ssrc, sdst, stt)
    rep = adv.advanced_report((rsrc, rdst, rt), (ssrc, sdst, stt), rstats, sstats)
    print(adv.format_advanced(rep), flush=True)

    results[name] = {
        "domain": domain, "domain_precedent": precedent, "cmd": cmd,
        "real": {"edges": n_edges, "nodes": n_nodes, "span_days": span_days,
                 "time_prefix_capped_to": CAP if capped else None},
        "synth": {"edges": int(ssrc.size),
                  "nodes": int(np.unique(np.concatenate([ssrc, sdst])).size)},
        "advanced": rep,
    }
    shutil.rmtree(out, ignore_errors=True)
    print(f"deleted {out}", flush=True)

with open(f"{SP}/advanced_axes.json", "w") as fh:
    json.dump(results, fh, indent=2, default=lambda o: None)
print(f"\nwrote {SP}/advanced_axes.json")
