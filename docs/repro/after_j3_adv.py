#!/usr/bin/env python3
"""JOB 3 (AFTER-fix): advanced evaluation axes, SAGA vs each real graph.

Replica of before_snapshot/job4_axes.py (same matching protocol, --no-anomalies,
seed 42, 2.5M time-prefix cap); ONLY the binary path changed. EXTENDED from the
original 4 datasets to all 7 real graphs, using the domain mapping that
before_snapshot/j4_fid.py (the tab:fidelity driver) uses for the sx-* graphs.
"""
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np

sys.path.insert(0, "./python")
from saga_eval import load, metrics_advanced as adv, stats as st  # noqa: E402

REPO = "."
HOME = os.path.expanduser("~")
SAGA = "./target/release/saga"
SP = "$WORK"
CAP = 2_500_000
SEED = 42

DATASETS = [
    # name, path, loader kwargs, saga domain, precedent for the domain choice
    ("snap-email-eu", f"{REPO}/realdata/snap-email-eu/email-Eu-core-temporal.txt",
     dict(src_col=0, dst_col=1, t_col=2, has_header=False), "finance",
     "job4_axes.py / docs/VALIDATION.md 'SAGA finance vs email-Eu'"),
    ("snap-collegemsg", f"{REPO}/realdata/snap-collegemsg/CollegeMsg.txt",
     dict(src_col=0, dst_col=1, t_col=2, has_header=False), "finance",
     "job4_axes.py / docs/VALIDATION.md 'SAGA finance vs CollegeMsg'"),
    ("tgbl-review", f"{REPO}/realdata/tgbl-review/tgbl-review_edgelist_v2.csv",
     dict(src_col=1, dst_col=2, t_col=0, has_header=True), "ecommerce",
     "job4_axes.py / docs/VALIDATION.md 'SAGA ecommerce vs tgbl-review'"),
    ("tgbl-wiki", f"{REPO}/realdata/tgbl-wiki/tgbl-wiki_edgelist_v2.csv",
     dict(src_col=0, dst_col=1, t_col=2, has_header=True), "ecommerce",
     "job4_axes.py; NO doc precedent; INDEX.txt calls it 'ECOMMERCE-like bipartite'"),
    # --- EXTENSION beyond the pre-fix job4_axes.py run (no pre-fix baseline) ---
    ("wiki-talk", f"{HOME}/.cache/zord/wiki-talk-temporal.txt.gz", {}, "crypto",
     "domain mapping from j4_fid.py (tab:fidelity driver)"),
    ("sx-mathoverflow", f"{HOME}/.cache/zord/sx-mathoverflow.txt.gz", {}, "crypto",
     "domain mapping from j4_fid.py (tab:fidelity driver)"),
    ("sx-superuser", f"{HOME}/.cache/zord/sx-superuser.txt.gz", {}, "cyber",
     "domain mapping from j4_fid.py (tab:fidelity driver)"),
    ("sx-askubuntu", f"{HOME}/.cache/zord/sx-askubuntu.txt.gz", {}, "cyber",
     "domain mapping from j4_fid.py (tab:fidelity driver)"),
]


def time_prefix(src, dst, t, cap):
    if src.size <= cap:
        return src, dst, t, False
    order = np.argsort(t, kind="stable")[:cap]
    order = np.sort(order)
    return src[order], dst[order], t[order], True


results = {}
for name, path, kw, domain, precedent in DATASETS:
    print(f"\n{'='*70}\n{name}  (domain={domain})\n{'='*70}", flush=True)
    out = f"/tmp/j3after_{name}"
    try:
        if not os.path.exists(path):
            raise FileNotFoundError(path)
        rsrc, rdst, rt = load.load_real(path, **kw)
        rsrc, rdst, rt, capped = time_prefix(rsrc, rdst, rt, CAP)
        n_edges = int(rsrc.size)
        n_nodes = int(np.unique(np.concatenate([rsrc, rdst])).size)
        span_days = float((rt.max() - rt.min()) / 86400.0)
        print(f"real: {n_edges:,} edges  {n_nodes:,} nodes  span {span_days:.3f} d  capped={capped}", flush=True)

        shutil.rmtree(out, ignore_errors=True)
        cmd = [SAGA, "generate", "--domain", domain, "--nodes", str(n_nodes),
               "--edges", str(n_edges), "--time-span", f"{max(span_days, 1e-3):.6f}",
               "--no-anomalies", "--seed", str(SEED), "--out", out]
        print("gen:", " ".join(cmd), flush=True)
        t0 = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
        if p.returncode != 0:
            raise RuntimeError(f"generate rc={p.returncode}: {p.stderr.strip()[:500]}")
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
            "status": "OK",
        }
    except Exception as e:  # noqa: BLE001
        print(f"!! FAILED: {type(e).__name__}: {e}", flush=True)
        results[name] = {"domain": domain, "status": f"FAILED: {type(e).__name__}: {e}"}
    finally:
        shutil.rmtree(out, ignore_errors=True)
    with open(f"{SP}/advanced_after.json", "w") as fh:
        json.dump(results, fh, indent=2, default=lambda o: None)

print(f"\nwrote {SP}/advanced_after.json")
