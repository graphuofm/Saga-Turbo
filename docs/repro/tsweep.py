#!/usr/bin/env python3
"""Generate transport from a given yaml at matrix size, report fidelity + components."""
import sys, os, json, subprocess, shutil
sys.path.insert(0, "./python")
import numpy as np
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare

HOME = os.path.expanduser("~")
BIN = "./target/release/saga"
CAP = 1_500_000
SP = "./scratch"
def cap_t(s, d, t, cap=CAP):
    if s.size <= cap: return s, d, t
    order = np.sort(np.argsort(t, kind="stable")[:cap])
    return s[order], d[order], t[order]

rs, rd, rt = cap_t(*load_real(f"{HOME}/.cache/zord/sx-askubuntu.txt.gz"))
N, M, SPAN = 159316, 964437, 2613.82
real = compute_stats(rs, rd, rt)

def run(yaml_path, tag):
    outdir = os.path.join(SP, f"sw_{tag}")
    shutil.rmtree(outdir, ignore_errors=True)
    subprocess.run([BIN, "generate", "--domain", yaml_path, "--nodes", str(N), "--edges", str(M),
                    "--time-span", str(SPAN), "--seed", "42", "--anomaly-rate", "0.02",
                    "--out", outdir, "--quiet"], check=True, stderr=subprocess.DEVNULL)
    ss, sd, st = cap_t(*load_saga(outdir))
    synth = compute_stats(ss, sd, st)
    res = compare(real, synth)
    shutil.rmtree(outdir, ignore_errors=True)
    ks = res["ks_distances"]; sm = res["scalar_metrics"]
    print(f"\n[{tag}] FIDELITY={res['fidelity_score']:.4f}  nodes={synth.n_nodes}")
    print(f"   ie_KS={ks['inter_event_time']:.3f} in_KS={ks['in_degree']:.3f} tot_KS={ks['total_degree']:.3f} out_KS={ks['out_degree']:.3f}")
    print(f"   repeat={sm['repeat_edge_ratio']['synth']:.3f}(d{sm['repeat_edge_ratio']['distance']:.3f}) "
          f"burst={sm['burstiness_b']['synth']:.3f}(d{sm['burstiness_b']['distance']:.3f}) "
          f"recip={sm['reciprocity']['synth']:.3f}(d{sm['reciprocity']['distance']:.3f}) "
          f"clust={sm['clustering']['synth']:.3f}(d{sm['clustering']['distance']:.3f}) "
          f"mdeg={sm['mean_degree']['synth']:.2f}(d{sm['mean_degree']['distance']:.3f})")
    return res['fidelity_score']

if __name__ == "__main__":
    for arg in sys.argv[1:]:
        yaml_path, tag = arg.split("::")
        run(yaml_path, tag)
