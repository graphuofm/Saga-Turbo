import json, os, re, shutil, subprocess, sys, time

SAGA = "./target/release/saga"
SCRATCH = "$WORK"
OUT = os.path.join(SCRATCH, "g_run")
EDGES = 100_000_000
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
PIS = [0.005, 0.01, 0.02, 0.05, 0.10]

res = []
for dom in DOMAINS:
    for pi in PIS:
        shutil.rmtree(OUT, ignore_errors=True)
        cmd = [SAGA, "generate", "--domain", dom, "--edges", str(EDGES),
               "--anomaly-rate", str(pi), "--seed", "42", "--format", "parquet",
               "--out", OUT, "--quiet"]
        t0 = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True)
        wall = time.time() - t0
        cell = {"domain": dom, "pi": pi, "edges_requested": EDGES}
        if p.returncode != 0:
            cell["status"] = "FAILED"
            cell["reason"] = (p.stderr or p.stdout)[-500:]
        else:
            prom = open(os.path.join(OUT, "metrics.prom")).read()
            tot = int(re.search(r"saga_edges_total (\d+)", prom).group(1))
            anom = int(re.search(r"saga_anomalous_edges_total (\d+)", prom).group(1))
            by_intent = dict((m.group(1), int(m.group(2))) for m in
                             re.finditer(r'saga_edges_by_intent\{intent="([^"]+)"\} (\d+)', prom))
            cell.update(status="ok", total_edges=tot, anomalous_edges=anom,
                        fraction=anom / tot if tot else float("nan"),
                        wall_s=round(wall, 1), by_intent=by_intent)
        res.append(cell)
        print(json.dumps({k: v for k, v in cell.items() if k != "by_intent"}), flush=True)
        shutil.rmtree(OUT, ignore_errors=True)

json.dump(res, open(os.path.join(SCRATCH, "gpi_raw.json"), "w"), indent=2)
print("DONE")
