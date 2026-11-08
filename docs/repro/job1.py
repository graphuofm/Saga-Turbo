import json, os, shutil, subprocess, sys, time

BIN = "./target/release/saga"
SCR = "$WORK"
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
PIS = [0.005, 0.01, 0.02, 0.05, 0.10]
EDGES = 100_000_000

rows = []
for dom in DOMAINS:
    for pi in PIS:
        out = f"{SCR}/g1_{dom}_{pi}"
        shutil.rmtree(out, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", dom, "--edges", str(EDGES),
               "--anomaly-rate", str(pi), "--seed", "42", "--out", out]
        t0 = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
        wall = time.time() - t0
        rec = {"domain": dom, "pi": pi, "wall_s": round(wall, 1)}
        try:
            st = json.load(open(f"{out}/saga_stats.json"))
            tot = st["total_edges"]
            lab = dict(st["label_introspection"])
            anom = sum(v for k, v in lab.items() if k != "normal")
            rec.update(status="OK", total_edges=tot, anom_edges=anom,
                       frac=anom / tot if tot else None,
                       intents={k: v for k, v in lab.items() if k != "normal"})
        except Exception as e:
            rec.update(status="FAILED", reason=f"{type(e).__name__}: {e}",
                       rc=p.returncode, stderr=p.stderr[-800:])
        shutil.rmtree(out, ignore_errors=True)
        rows.append(rec)
        print(json.dumps(rec), flush=True)
        json.dump(rows, open(f"{SCR}/gpi_after_raw.json", "w"), indent=1)
print("DONE")
