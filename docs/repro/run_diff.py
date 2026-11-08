import json, os, re, shutil, subprocess, sys, time

sys.path.insert(0, "./python")
from saga_eval.detector import detect_auc

SAGA = "./target/release/saga"
SCRATCH = "$WORK"
OUT = os.path.join(SCRATCH, "d_run")
EDGES = 10_000_000
PI = 0.05
DELTAS = [0.0, 0.25, 0.5, 0.75, 1.0]

res = []
for d in DELTAS:
    shutil.rmtree(OUT, ignore_errors=True)
    cmd = [SAGA, "generate", "--domain", "finance", "--edges", str(EDGES),
           "--anomaly-rate", str(PI), "--difficulty", str(d), "--seed", "42",
           "--format", "csv", "--shard-size", "8192", "--out", OUT, "--quiet"]
    t0 = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True)
    cell = {"delta": d, "edges_requested": EDGES, "pi": PI}
    if p.returncode != 0:
        cell["status"] = "FAILED"; cell["reason"] = (p.stderr or p.stdout)[-500:]
        res.append(cell); print(json.dumps(cell), flush=True); continue
    prom = open(os.path.join(OUT, "metrics.prom")).read()
    tot = int(re.search(r"saga_edges_total (\d+)", prom).group(1))
    anom = int(re.search(r"saga_anomalous_edges_total (\d+)", prom).group(1))
    ncsv = len([f for f in os.listdir(OUT) if f.startswith("edges") and f.endswith(".csv")])
    cell.update(status="ok", total_edges=tot, anomalous_edges=anom,
                fraction=anom / tot, n_csv_shards=ncsv, gen_wall_s=round(time.time() - t0, 1))
    for fs in ["all", "attr", "struct"]:
        t1 = time.time()
        try:
            r = detect_auc(OUT, seed=0, feature_set=fs)
            cell[fs] = r
            cell[fs]["train_s"] = round(time.time() - t1, 1)
        except Exception as e:
            cell[fs] = {"status": "FAILED", "reason": repr(e)[:300]}
        print(d, fs, cell[fs], flush=True)
    res.append(cell)
    shutil.rmtree(OUT, ignore_errors=True)

json.dump(res, open(os.path.join(SCRATCH, "diff_raw.json"), "w"), indent=2)
print("DONE")
