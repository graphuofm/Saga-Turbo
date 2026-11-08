#!/usr/bin/env python3
"""JOB 2: Elliptic anomaly-realism re-measured on the STRUCTURALLY-FIXED engine
(AFTER-2). Reuses scratchpad/anom.py UNMODIFIED for every statistic; the BEFORE
and AFTER-1 columns are pulled programmatically from the pre-fix / post-multiplier
JSON, never transcribed.
"""
import json
import os
import shutil
import subprocess
import sys
import time

sys.path.insert(0, "$WORK")
import anom  # UNMODIFIED  # noqa: E402

S = "$WORK"
BIN = "./target/release/saga"

# Elliptic reference, re-derived from scratch (must reproduce the frozen values).
EXPECT = {"mean_degree_ratio": 0.650, "clustering_ratio": 0.056,
          "in_out": 1.712, "homophily_lift": 13.273, "edge_share": 0.0348, "ks": 0.210}

out = {"method": ("scratchpad/anom.py used unmodified; BEFORE from anomaly_validation.json, "
                  "AFTER-1 from anomaly_validation_after.json, both read programmatically"),
       "binary_after2": BIN}

print("[1/4] re-deriving Elliptic reference ...", flush=True)
esrc, edst, et, elab, ets, en = anom.load_elliptic()
ell = anom.analyse("elliptic", esrc, edst, et, elab, en)
ell["basics"] = {"n_nodes": en, "n_edges": int(esrc.size),
                 "n_illicit": int((elab == 1).sum()), "n_licit": int((elab == 0).sum()),
                 "illicit_rate_all_nodes": float((elab == 1).sum() / en),
                 "n_time_steps": int(len(set(ets[ets > 0].tolist())))}
a, c = ell["anomalous"], ell["contrast"]
got = {"mean_degree_ratio": c["mean_degree_ratio"], "clustering_ratio": c["clustering_ratio"],
       "in_out": a["in_out_ratio_mean"], "homophily_lift": a["homophily_lift"],
       "edge_share": a["n_incident_edges"] / ell["n_edges_total"],
       "ks": c["degree_KS_anom_vs_norm"]}
drift = {k: (round(got[k], 4), EXPECT[k]) for k in EXPECT if abs(got[k] - EXPECT[k]) > 0.001}
out["elliptic_selfcheck"] = {"measured": {k: round(v, 4) for k, v in got.items()},
                             "expected": EXPECT, "drifted": drift, "ok": not drift}
print("  measured:", {k: round(v, 4) for k, v in got.items()}, flush=True)
if drift:
    print("  *** PIPELINE DRIFT ***", drift, flush=True)
    json.dump(out, open(f"{S}/anomaly_validation_after2.json", "w"), indent=1, default=float)
    sys.exit("STOP: Elliptic reference drifted")
print("  self-check OK — Elliptic reference reproduces exactly.", flush=True)
out["real_elliptic"] = ell

CFGS = [("A_matched", 234355), ("B_1e7", 10_000_000)]
out["configs"] = {}
for tag, medges in CFGS:
    d = f"{S}/after2_{tag}"
    shutil.rmtree(d, ignore_errors=True)
    cmd = [BIN, "generate", "--domain", "crypto", "--nodes", "203769", "--edges", str(medges),
           "--time-span", "49", "--seed", "42", "--anomaly-rate", "0.0223",
           "--format", "parquet", "--out", d, "--quiet"]
    print(f"[gen {tag}] {' '.join(cmd)}", flush=True)
    t0 = time.time()
    try:
        subprocess.run(cmd, check=True, stderr=subprocess.PIPE, timeout=3600)
        src, dst, t, lab, elab_s, n = anom.load_saga(d)
        res = anom.analyse("saga", src, dst, t, lab, n)
        res["basics"] = {"n_nodes": n, "n_edges": int(src.size),
                         "n_anomalous_nodes": int((lab == 1).sum()),
                         "anomalous_node_rate": float((lab == 1).sum() / n),
                         "n_anomalous_edges": int(elab_s.sum()),
                         "anomalous_edge_rate": float(elab_s.mean())}
        res["cmd"] = " ".join(cmd)
        res["wall_s"] = round(time.time() - t0, 1)
        out[f"after2_cfg{tag}"] = res
        out["configs"][tag] = {"cmd": " ".join(cmd), "edges_requested": medges,
                               "edges_actual": int(src.size),
                               "anomalous_edges": int(elab_s.sum()),
                               "anomalous_edge_rate": float(elab_s.mean()),
                               "anomalous_nodes": int((lab == 1).sum()),
                               "status": "OK"}
        print(f"  [{tag}] edges={src.size} anom_edges={int(elab_s.sum())} "
              f"homophily_lift={res['anomalous']['homophily_lift']:.3f} "
              f"deg_ratio={res['contrast']['mean_degree_ratio']:.3f}", flush=True)
    except Exception as e:  # noqa: BLE001
        out["configs"][tag] = {"cmd": " ".join(cmd), "status": f"FAILED: {type(e).__name__}: {e}"}
        print(f"  [{tag}] FAILED: {e}", flush=True)
    json.dump(out, open(f"{S}/anomaly_validation_after2.json", "w"), indent=1, default=float)

# ---- anomaly-internal fidelity, same code path as subfid.py (saga_eval.compare)
print("[3/4] anomaly-subgraph fidelity ...", flush=True)
sys.path.insert(0, "./python")
import numpy as np  # noqa: E402
from saga_eval.compare import compare  # noqa: E402
from saga_eval.stats import compute_stats  # noqa: E402


def sub(src, dst, t, lab, mode):
    ms = np.isin(src, np.nonzero(lab == 1)[0]); md = np.isin(dst, np.nonzero(lab == 1)[0])
    k = (ms | md) if mode == "incident" else (ms & md)
    return src[k], dst[k], t[k]


fid = {}
for tag, _ in CFGS:
    d = f"{S}/after2_{tag}"
    if not os.path.isdir(d):
        fid[tag] = {"status": "SKIPPED: generation failed"}
        continue
    ss, sd, st_, slab, _, sn = anom.load_saga(d)
    for mode in ("incident", "internal", "whole"):
        if mode == "whole":
            rs, rd, rt = esrc, edst, et
            ys, yd, yt = ss, sd, st_
        else:
            rs, rd, rt = sub(esrc, edst, et, elab, mode)
            ys, yd, yt = sub(ss, sd, st_, slab, mode)
        rec = {"real_n_edges": int(rs.size), "synth_n_edges": int(ys.size)}
        if ys.size < 5 or rs.size < 5:
            rec.update(fidelity_score=None, note="too few edges to compute")
        else:
            cc = compare(compute_stats(rs, rd, rt, seed=0), compute_stats(ys, yd, yt, seed=0))
            rec.update(fidelity_score=cc["fidelity_score"], ks_distances=cc["ks_distances"])
        fid[f"{tag}_{mode}"] = rec
        print(f"  {tag} {mode}: real={rs.size} synth={ys.size} fid={rec.get('fidelity_score')}", flush=True)
out["subgraph_fidelity_after2"] = fid

# ---- prior columns, read programmatically
out["before_prefix"] = json.load(open(f"{S}/anomaly_validation.json"))["saga_crypto_rate0.0223"]
prev = json.load(open(f"{S}/anomaly_validation_after.json"))
out["after1_cfgA_matched"] = prev["after_cfgA_matched"]
out["after1_cfgB_1e7"] = prev["after_cfgB_1e7"]
out["after1_configs"] = prev["configs"]
out["after1_subgraph_fidelity"] = prev["subgraph_fidelity_after"]
out["before_subgraph_fidelity"] = prev["subgraph_fidelity_before"]
out["before_wholegraph_fidelity"] = prev["wholegraph_fidelity_before"]
json.dump(out, open(f"{S}/anomaly_validation_after2.json", "w"), indent=1, default=float)

for tag, _ in CFGS:
    shutil.rmtree(f"{S}/after2_{tag}", ignore_errors=True)
print("[4/4] DONE job2 — wrote anomaly_validation_after2.json; generated dirs deleted", flush=True)
