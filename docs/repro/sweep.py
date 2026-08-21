"""P2-b re-measurement: crypto + finance difficulty->detectability sweep.
Generates each graph fresh (seed 42, 500k edges, anomaly-rate 0.05), measures
base rate and detector AP/AUC for attr/struct/all views, deletes the graph."""
import subprocess, sys, shutil, glob, os
from pathlib import Path
sys.path.insert(0, "./python")
import pandas as pd
from saga_eval.detector import detect_auc

AGORA = "./target/release/saga"
SCRATCH = "./scratch/sweepdirs"
DELTAS = [0.0, 0.25, 0.5, 0.75, 1.0]

def run(domain):
    print(f"\n===== {domain} =====")
    rows = []
    for d in DELTAS:
        out = f"{SCRATCH}/{domain}_{d}"
        shutil.rmtree(out, ignore_errors=True)
        subprocess.run([AGORA, "generate", "--domain", domain, "--edges", "500000",
                        "--seed", "42", "--anomaly-rate", "0.05", "--difficulty", str(d),
                        "--format", "csv", "--out", out, "--quiet"], check=True)
        edges = sorted(glob.glob(f"{out}/edges*.csv"))[0]
        df = pd.read_csv(edges)
        y = (df["label"].astype(str) != "normal").astype(int)
        n_anom, n_tot = int(y.sum()), int(y.size)
        base = n_anom / n_tot
        res = {v: detect_auc(out, seed=42, feature_set=v) for v in ("attr", "struct", "all")}
        rows.append((d, n_anom, n_tot, base, res))
        print(f"d={d}: anom={n_anom} tot={n_tot} base={base*100:.3f}%  "
              f"AP attr={res['attr']['ap']:.4f} struct={res['struct']['ap']:.4f} all={res['all']['ap']:.4f}  "
              f"AUC attr={res['attr']['roc_auc']:.4f} struct={res['struct']['roc_auc']:.4f} all={res['all']['roc_auc']:.4f}")
        shutil.rmtree(out, ignore_errors=True)
    # summary tables
    print(f"\n--- {domain} base rate ---")
    print("| delta | anom edges | total | base rate |")
    for d,na,nt,b,_ in rows:
        print(f"| {d} | {na} | {nt} | {b*100:.3f}% |")
    print(f"\n--- {domain} AP ---")
    print("| view | " + " | ".join(str(d) for d in DELTAS) + " |")
    for v in ("attr","struct","all"):
        print(f"| {v} | " + " | ".join(f"{r[4][v]['ap']:.4f}" for r in rows) + " |")
    print(f"\n--- {domain} AUC ---")
    print("| view | " + " | ".join(str(d) for d in DELTAS) + " |")
    for v in ("attr","struct","all"):
        print(f"| {v} | " + " | ".join(f"{r[4][v]['roc_auc']:.4f}" for r in rows) + " |")

if __name__ == "__main__":
    os.makedirs(SCRATCH, exist_ok=True)
    for dom in sys.argv[1:] or ["crypto", "finance"]:
        run(dom)
    shutil.rmtree(SCRATCH, ignore_errors=True)
