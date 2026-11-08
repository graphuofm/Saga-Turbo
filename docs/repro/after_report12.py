#!/usr/bin/env python3
"""Render JOB1/JOB2 after-fix tables + MATCHES/DRIFTED vs the pre-fix baselines."""
import json

SP = "$WORK"
after = json.load(open(f"{SP}/fidelity_after.json"))
before = json.load(open(f"{SP}/before_snapshot/before_fidelity.json"))
bmap = {r["real"]: r for r in before}
L = []
P = L.append


def d(x, n=3):
    return "n/a" if x is None else f"{x:.{n}f}"


P("=" * 118)
P("JOB 1 — tab:fidelity RE-MEASURED (after fix).  Protocol: match SAGA to real (n, m, span); seed 42;")
P("        --anomaly-rate 0.02; 2.5M-edge time-prefix cap.  Identical to pre-fix driver except the binary.")
P("=" * 118)
P("")
P(f"{'real graph':17s} {'domain':10s} {'edges':>9s} {'nodes':>8s} {'span_d':>7s} {'FID':>6s} {'Δfid':>7s}")
P("-" * 118)
for r in after:
    b = bmap.get(r["real"], {})
    df = r["fidelity"] - b.get("fidelity", float("nan"))
    P(f"{r['real']:17s} {r['domain']:10s} {r['real_edges']:>9,} {r['real_nodes']:>8,} "
      f"{r['span_days']:>7.1f} {r['fidelity']:>6.3f} {df:>+7.3f}")

P("")
P("RECIPROCITY and REPEAT RATE — two SEPARATE metrics (the paper's 'recip. r/s' column holds REPEAT RATE)")
P("-" * 118)
P(f"{'real graph':17s} | {'RECIPROCITY real':>16s} {'synth':>9s} {'dist':>6s} | {'REPEAT real':>11s} {'synth':>9s} {'dist':>6s}")
P("-" * 118)
for r in after:
    sd = r["scalar_distances"]
    P(f"{r['real']:17s} | {r['real_reciprocity']:>16.4f} {r['saga_reciprocity']:>9.4f} "
      f"{sd['reciprocity']:>6.3f} | {r['real_repeat_rate']:>11.3f} {r['saga_repeat_rate']:>9.3f} {sd['repeat_edge_ratio']:>6.3f}")

P("")
P("BURSTINESS B, POWER-LAW ALPHA, CLUSTERING, MEAN DEGREE (real / synth)")
P("-" * 118)
P(f"{'real graph':17s} | {'B real':>7s} {'synth':>7s} | {'alpha real':>10s} {'synth':>7s} | {'clus real':>9s} {'synth':>7s} | {'meandeg real':>12s} {'synth':>10s}")
P("-" * 118)
for r in after:
    P(f"{r['real']:17s} | {r['real_B']:>7.3f} {r['saga_B']:>7.3f} | {d(r['real_alpha']):>10s} {d(r['saga_alpha']):>7s} | "
      f"{d(r['real_clustering']):>9s} {d(r['saga_clustering']):>7s} | {r['real_mean_deg']:>12.2f} {r['saga_mean_deg']:>10.2f}")

P("")
P("EVERY KS TERM (two-sample KS, lower = closer)")
P("-" * 118)
P(f"{'real graph':17s} | {'out_degree':>10s} {'in_degree':>10s} {'total_degree':>12s} {'inter_event_time':>16s}")
P("-" * 118)
for r in after:
    k = r["ks"]
    P(f"{r['real']:17s} | {k['out_degree']:>10.3f} {k['in_degree']:>10.3f} {k['total_degree']:>12.3f} {k['inter_event_time']:>16.3f}")

P("")
P("SCALAR DISTANCE COMPONENTS (the other terms averaged into FID)")
P("-" * 118)
keys = ["powerlaw_alpha", "burstiness_b", "memory_m", "clustering", "reciprocity", "repeat_edge_ratio", "mean_degree"]
P(f"{'real graph':17s} | " + " ".join(f"{k[:12]:>12s}" for k in keys))
P("-" * 118)
for r in after:
    sd = r["scalar_distances"]
    P(f"{r['real']:17s} | " + " ".join(f"{d(sd[k]):>12s}" for k in keys))

P("")
P("MATCHES / DRIFTED vs pre-fix (before_snapshot/before_fidelity.json)")
P("-" * 118)
P(f"{'real graph':17s} | {'fid b→a':>16s} | {'recip b→a':>20s} | {'repeat b→a':>20s} | {'B b→a':>18s} | verdict")
P("-" * 118)
for r in after:
    b = bmap.get(r["real"])
    if not b:
        P(f"{r['real']:17s} | NO PRE-FIX BASELINE")
        continue
    ch = []
    if abs(r["fidelity"] - b["fidelity"]) > 0.002:
        ch.append("fid")
    for lbl, ka, kb in [("recip", "saga_reciprocity", "saga_reciprocity"),
                        ("repeat", "saga_repeat_rate", "saga_repeat_rate"),
                        ("B", "saga_B", "saga_B")]:
        if abs(r[ka] - b[kb]) > 0.005 * max(1.0, abs(b[kb])):
            ch.append(lbl)
    verdict = "MATCHES" if not ch else "DRIFTED (" + ",".join(ch) + ")"
    P(f"{r['real']:17s} | {b['fidelity']:>7.3f}→{r['fidelity']:<8.3f} | "
      f"{b['saga_reciprocity']:>9.4f}→{r['saga_reciprocity']:<10.4f} | "
      f"{b['saga_repeat_rate']:>9.3f}→{r['saga_repeat_rate']:<10.3f} | "
      f"{b['saga_B']:>8.3f}→{r['saga_B']:<9.3f} | {verdict}")

# ---------------- JOB 2 ----------------
sa = json.load(open(f"{SP}/signatures_after.json"))
sb = json.load(open(f"{SP}/before_snapshot/before_signatures.json"))
sbm = {r["domain"]: r for r in sb}
P("")
P("=" * 118)
P("JOB 2 — tab:multidomain RE-MEASURED (after fix).  --edges 2000000 --seed 42 --no-anomalies")
P("=" * 118)
P("")
P(f"{'domain':11s} {'nodes':>8s} {'edges':>9s} {'meandeg':>8s} {'clust':>7s} {'recip':>7s} {'B':>6s} {'M':>7s} {'alpha':>6s} {'repeat':>7s} {'span_d':>7s}")
P("-" * 118)
for r in sa:
    P(f"{r['domain']:11s} {r['n_nodes']:>8,} {r['n_edges']:>9,} {r['mean_deg']:>8.2f} {r['clustering']:>7.4f} "
      f"{r['reciprocity']:>7.4f} {r['burstiness_b']:>6.3f} {r['memory_m']:>7.3f} {r['powerlaw_alpha']:>6.2f} "
      f"{r['repeat_edge']:>7.3f} {r['span_days']:>7.1f}")
P("")
P("TYPED SCHEMA COUNTS")
P("-" * 118)
P(f"{'domain':11s} {'node types':>11s} {'edge types':>11s} {'attributes':>11s}")
P("-" * 118)
for r in sa:
    P(f"{r['domain']:11s} {r['n_node_types']:>11d} {r['n_edge_types']:>11d} {r['n_attributes']:>11d}")
P("")
P("MATCHES / DRIFTED vs pre-fix (before_snapshot/before_signatures.json)")
P("-" * 118)
FIELDS = ["n_nodes", "mean_deg", "clustering", "reciprocity", "burstiness_b", "powerlaw_alpha", "repeat_edge",
          "n_node_types", "n_edge_types", "n_attributes"]
for r in sa:
    b = sbm.get(r["domain"])
    if not b:
        P(f"{r['domain']:11s} NO PRE-FIX BASELINE")
        continue
    ch = [f for f in FIELDS
          if b.get(f) is not None and r.get(f) is not None
          and abs(r[f] - b[f]) > 0.005 * max(1.0, abs(b[f]))]
    P(f"{r['domain']:11s} {'MATCHES' if not ch else 'DRIFTED: ' + ', '.join(f'{f} {b[f]}→{r[f]}' for f in ch)}")

txt = "\n".join(L)
print(txt)
open(f"{SP}/fidelity_after.txt", "w").write(txt + "\n")
