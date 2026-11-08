#!/usr/bin/env python3
"""Reconstruct the tab:fidelity 7-row driver on the FINAL fixed engine.
Protocol = matrix protocol (docs/remeasure_matrix_agora.md) with CAP=2.5M and
the tab:fidelity domain mapping. Every AGORA row generated seed 42, deleted after.
"""
import os, sys, json, shutil, subprocess, tempfile
import numpy as np

REPO = "."
sys.path.insert(0, os.path.join(REPO, "python"))
from saga_eval import load, stats as st, compare as cmp

BIN = os.path.join(REPO, "target/release/saga")
CAP = 2_500_000
SEED = 42
Z = os.path.expanduser("~/.cache/zord")
RD = os.path.join(REPO, "realdata")
SCRATCH = "$WORK"

# (row-label, domain, real loader spec)
# loader spec: ("snap", path) or ("csv", path, src,dst,t)
ROWS = [
    ("finance/email-Eu-core",  "finance",   ("snap", os.path.join(RD, "snap-email-eu/email-Eu-core-temporal.txt"))),
    ("finance/CollegeMsg",     "finance",   ("snap", os.path.join(RD, "snap-collegemsg/CollegeMsg.txt"))),
    ("crypto/wiki-talk",       "crypto",    ("snap", os.path.join(Z, "wiki-talk-temporal.txt.gz"))),
    ("crypto/sx-mathoverflow", "crypto",    ("snap", os.path.join(Z, "sx-mathoverflow.txt.gz"))),
    ("cyber/sx-superuser",     "cyber",     ("snap", os.path.join(Z, "sx-superuser.txt.gz"))),
    ("cyber/sx-askubuntu",     "cyber",     ("snap", os.path.join(Z, "sx-askubuntu.txt.gz"))),
    ("e-comm/tgbl-review",     "ecommerce", ("csv",  os.path.join(RD, "tgbl-review/tgbl-review_edgelist_v2.csv"), "source", "target", "ts")),
]

def load_real(spec):
    if spec[0] == "snap":
        return load.load_real(spec[1])
    _, path, sc, dc, tc = spec
    return load.load_real(path, src_col=sc, dst_col=dc, t_col=tc, has_header=True)

def cap_earliest(src, dst, t, n):
    if src.size <= n:
        return src, dst, t
    order = np.argsort(t, kind="stable")[:n]
    order = np.sort(order)
    return src[order], dst[order], t[order]

def run_row(label, domain, spec, only=None):
    rsrc, rdst, rt = load_real(spec)
    rsrc, rdst, rt = cap_earliest(rsrc, rdst, rt, CAP)
    m = int(rsrc.size)
    n = int(np.unique(np.concatenate([rsrc, rdst])).size)
    span_s = float(rt.max() - rt.min())
    span_days = round(span_s / 86400.0, 2)
    outdir = tempfile.mkdtemp(prefix="fidgen_", dir=SCRATCH)
    try:
        cmd = [BIN, "generate", "--domain", domain, "--nodes", str(n), "--edges", str(m),
               "--time-span", str(span_days), "--seed", str(SEED), "--anomaly-rate", "0.02",
               "--out", outdir, "--quiet"]
        subprocess.run(cmd, check=True, capture_output=True)
        ssrc, sdst, stt = load.load_saga(outdir)
        # cap AGORA to the same edge budget (earliest chronologically)
        ssrc, sdst, stt = cap_earliest(ssrc, sdst, stt, m)
        real = st.compute_stats(rsrc, rdst, rt)
        synth = st.compute_stats(ssrc, sdst, stt)
        res = cmp.compare(real, synth)
        fid = res["fidelity_score"]
        B = res["scalar_metrics"]["burstiness_b"]
        rec = res["scalar_metrics"]["reciprocity"]
        return {
            "label": label, "domain": domain,
            "real_edges": m, "real_nodes": n, "span_days": span_days,
            "synth_edges": int(ssrc.size),
            "fid": fid,
            "B_real": B["real"], "B_synth": B["synth"],
            "recip_real": rec["real"], "recip_synth": rec["synth"],
        }
    finally:
        shutil.rmtree(outdir, ignore_errors=True)

if __name__ == "__main__":
    only = sys.argv[1] if len(sys.argv) > 1 else None
    out = []
    for label, domain, spec in ROWS:
        if only and only not in label:
            continue
        r = run_row(label, domain, spec)
        out.append(r)
        print(json.dumps(r), flush=True)
    with open(os.path.join(SCRATCH, "fid_results.json"), "w") as fh:
        json.dump(out, fh, indent=2)
