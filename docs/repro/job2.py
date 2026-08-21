import json, shutil, subprocess, time

BIN = "./target/release/saga"
SCR = "./scratch"

rows = []
for dom in ["finance", "cyber"]:
    for E in [200_000, 1_000_000, 2_000_000]:
        out = f"{SCR}/g2_{dom}_{E}"
        shutil.rmtree(out, ignore_errors=True)
        cmd = [BIN, "generate", "--domain", dom, "--edges", str(E),
               "--anomaly-rate", "0.05", "--seed", "42", "--out", out]
        t0 = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
        rec = {"domain": dom, "edges_req": E, "wall_s": round(time.time() - t0, 1)}
        try:
            st = json.load(open(f"{out}/saga_stats.json"))
            tot = st["total_edges"]
            lab = dict(st["label_introspection"])
            anom = sum(v for k, v in lab.items() if k != "normal")
            rec.update(status="OK", total_edges=tot, anom_edges=anom,
                       frac=anom / tot,
                       intents={k: v for k, v in lab.items() if k != "normal"})
        except Exception as e:
            rec.update(status="FAILED", reason=f"{type(e).__name__}: {e}",
                       rc=p.returncode, stderr=p.stderr[-800:])
        shutil.rmtree(out, ignore_errors=True)
        rows.append(rec)
        print(json.dumps(rec), flush=True)
json.dump(rows, open(f"{SCR}/job2_raw.json", "w"), indent=1)
print("DONE")
