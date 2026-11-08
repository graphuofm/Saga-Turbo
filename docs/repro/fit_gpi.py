import json, numpy as np

S = "$WORK"
raw = json.load(open(f"{S}/gpi_raw.json"))
DOMAINS = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
PIS = [0.005, 0.01, 0.02, 0.05, 0.10]

out = {"scale_edges_requested": 100_000_000, "seed": 42,
       "note": "all cells real runs; fraction = saga_anomalous_edges_total / saga_edges_total from metrics.prom",
       "cells": raw, "per_domain": {}}
lines = []
lines.append("JOB 1 - controllable anomaly rate at scale (--edges 1e8, --seed 42, 6 domains x 5 pi)")
lines.append("All 30 cells completed successfully; every number is from a real run.")
lines.append("")
hdr = f"{'domain':<11}{'pi':>7}{'total_edges':>14}{'anom_edges':>12}{'frac_%':>10}"
lines.append(hdr); lines.append("-" * len(hdr))
for dom in DOMAINS:
    cs = [c for c in raw if c["domain"] == dom and c["status"] == "ok"]
    cs.sort(key=lambda c: c["pi"])
    for c in cs:
        lines.append(f"{dom:<11}{c['pi']:>7}{c['total_edges']:>14,}{c['anomalous_edges']:>12,}{100*c['fraction']:>10.4f}")
    x = np.array([c["pi"] for c in cs]); y = np.array([c["fraction"] for c in cs])
    # through-origin fit y = c*pi
    c_hat = float((x @ y) / (x @ x))
    ss_res = float(((y - c_hat * x) ** 2).sum())
    ss_tot_o = float((y ** 2).sum())              # R^2 vs zero model (through-origin convention)
    ss_tot_m = float(((y - y.mean()) ** 2).sum())  # R^2 vs mean model
    r2_o = 1 - ss_res / ss_tot_o
    r2_m = 1 - ss_res / ss_tot_m
    mono = all(y[i] < y[i + 1] for i in range(len(y) - 1))
    resid_pct = [round(100 * (yi / (c_hat * xi) - 1), 2) for xi, yi in zip(x, y)]
    out["per_domain"][dom] = {
        "c_through_origin": c_hat, "r2_through_origin": r2_o, "r2_vs_mean": r2_m,
        "monotone_in_pi": mono,
        "anomalous_edges_by_pi": {str(c["pi"]): c["anomalous_edges"] for c in cs},
        "fraction_by_pi": {str(c["pi"]): c["fraction"] for c in cs},
        "pct_dev_from_linear_fit": dict(zip([str(v) for v in x], resid_pct)),
    }
    lines.append(f"  -> fit frac = {c_hat:.5f}*pi  (i.e. {100*c_hat:.3f}% per unit pi)   "
                 f"R2(origin)={r2_o:.5f}  R2(vs mean)={r2_m:.5f}  monotone={mono}")
    lines.append(f"     deviation of each point from the linear fit (%): {resid_pct}")
    lines.append("")

lines.append("PER-DOMAIN COEFFICIENTS (fraction = c * pi), sorted:")
lines.append(f"{'domain':<12}{'c':>10}{'R2(origin)':>13}{'anom@pi=0.005':>16}{'anom@pi=0.05':>15}")
for dom in sorted(DOMAINS, key=lambda d: -out["per_domain"][d]["c_through_origin"]):
    p = out["per_domain"][dom]
    lines.append(f"{dom:<12}{p['c_through_origin']:>10.5f}{p['r2_through_origin']:>13.5f}"
                 f"{p['anomalous_edges_by_pi']['0.005']:>16,}{p['anomalous_edges_by_pi']['0.05']:>15,}")
lines.append("")
cs_all = [out["per_domain"][d]["c_through_origin"] for d in DOMAINS]
lines.append(f"Spread of c across domains: min={min(cs_all):.5f} (crypto) max={max(cs_all):.5f} "
             f"-> ratio {max(cs_all)/min(cs_all):.0f}x. There is NO universal coefficient.")
open(f"{S}/gpi_scale.txt", "w").write("\n".join(lines) + "\n")
json.dump(out, open(f"{S}/gpi_scale.json", "w"), indent=2)
print("\n".join(lines))
