import json, shutil, subprocess, sys, time
sys.path.insert(0, "./python")
from saga_eval.detector import detect_auc

BIN = "./target/release/saga"
SCR = "./scratch"

# Sizing rationale: JOB1 gives finance frac ~= 0.081*pi. At pi=0.10 -> ~0.008.
# 5M edges -> ~41k positives at delta=0, leaving >=20k even if delta=1 halves it.
DOM, PI, EDGES = "finance", 0.10, 5_000_000
DELTAS = [0.0, 0.25, 0.5, 0.75, 1.0]
VIEWS = ["attr", "struct", "all"]
GEN_SEEDS = [42, 43]

rows = []
for gs in GEN_SEEDS:
    for delta in DELTAS:
        out = f"{SCR}/g3_{gs}_{delta}"
        shutil.rmtree(out, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", DOM, "--edges", str(EDGES),
               "--anomaly-rate", str(PI), "--difficulty", str(delta),
               "--seed", str(gs), "--format", "csv", "--shard-size", "4096",
               "-q", "--out", out]
        t0 = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
        if p.returncode != 0:
            rows.append({"gen_seed": gs, "delta": delta, "status": "FAILED",
                         "reason": p.stderr[-500:]})
            shutil.rmtree(out, ignore_errors=True)
            continue
        st = json.load(open(f"{out}/saga_stats.json"))
        lab = dict(st["label_introspection"])
        anom = sum(v for k, v in lab.items() if k != "normal")
        for view in VIEWS:
            t1 = time.time()
            try:
                r = detect_auc(out, seed=0, feature_set=view)
                rec = {"gen_seed": gs, "delta": delta, "view": view,
                       "status": "OK", "roc_auc": r["roc_auc"], "ap": r["ap"],
                       "n_anom": r["n_anom"], "n_total": r["n_total"],
                       "total_edges": st["total_edges"], "anom_edges": anom,
                       "frac": anom / st["total_edges"],
                       "fit_s": round(time.time() - t1, 1)}
            except Exception as e:
                rec = {"gen_seed": gs, "delta": delta, "view": view,
                       "status": "FAILED", "reason": f"{type(e).__name__}: {e}"}
            rows.append(rec)
            print(json.dumps(rec), flush=True)
            json.dump(rows, open(f"{SCR}/diff_after_raw.json", "w"), indent=1)
        shutil.rmtree(out, ignore_errors=True)
print("DONE")
