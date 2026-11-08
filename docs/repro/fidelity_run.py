import os
os.environ["OMP_NUM_THREADS"] = "6"
os.environ["OPENBLAS_NUM_THREADS"] = "6"
import sys, time, json
sys.path.insert(0, "./python")
from saga_eval import load, stats as st, compare as cmp, metrics_advanced as adv

SC = "$WORK"
RD = "./realdata"

JOBS = [
    ("finance_vs_CollegeMsg", f"{SC}/fin_college", f"{RD}/snap-collegemsg/CollegeMsg.txt"),
    ("finance_vs_email-Eu",   f"{SC}/fin_email",   f"{RD}/snap-email-eu/email-Eu-core-temporal.txt"),
]

out = {}
for name, synth_dir, real_path in JOBS:
    r = {}
    t = time.time()
    ss, sd, stt = load.load_saga(synth_dir)
    rs, rd, rt = load.load_real(real_path)
    r["synth_edges"] = int(ss.size); r["real_edges"] = int(rs.size)
    r["t_load"] = round(time.time() - t, 1)
    t = time.time()
    synth = st.compute_stats(ss, sd, stt)
    real = st.compute_stats(rs, rd, rt)
    r["t_stats"] = round(time.time() - t, 1)
    res = cmp.compare(real, synth)
    r["fidelity_score"] = res["fidelity_score"]
    r["ks"] = res["ks_distances"]
    r["scalars"] = {k: {"real": v["real"], "synth": v["synth"]} for k, v in res["scalar_metrics"].items()}
    t = time.time()
    disc = adv.discriminative_score((rs, rd, rt), (ss, sd, stt))
    r["C2ST"] = disc
    r["t_c2st"] = round(time.time() - t, 1)
    out[name] = r
    with open(f"{SC}/fidelity_results.json", "w") as fh:
        json.dump(out, fh, indent=2)
    print(f"[done] {name}", flush=True)

print("ALL DONE", flush=True)
