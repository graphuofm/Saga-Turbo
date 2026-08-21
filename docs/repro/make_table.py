import json, sys
SP = "./scratch"
d = json.load(open(f"{SP}/advanced_axes.json"))
try:
    fx = json.load(open(f"{SP}/advanced_axes_bipartite_fixed.json"))
except FileNotFoundError:
    fx = {}

L = []
w = L.append
w("JOB 4 — the five advanced evaluation axes, SAGA vs real (ALL NUMBERS FROM REAL RUNS)")
w("")
w("Protocol: match SAGA to each real graph's (nodes, edges, time-span) with --no-anomalies")
w("  --seed 42, per docs/VALIDATION.md ('scale-matched ... anomalies off') and")
w("  docs/EXPERIMENTS.md sec.9 ('Match SAGA to each real graph's (n,m,span); cap 2.5M edges').")
w("  Domain per docs: email-Eu/CollegeMsg -> finance; tgbl-review -> ecommerce.")
w("  tgbl-wiki -> ecommerce (NO doc precedent; realdata/INDEX.txt calls it 'ECOMMERCE-like').")
w("Metric source: python/saga_eval/metrics_advanced.py advanced_report(); delta=3600s (default).")
w("")
w("=" * 108)
w("MAIN TABLE  (energy/W1 reported for BOTH marginals the harness computes: total-degree and inter-event)")
w("=" * 108)
hdr = (f"{'real graph':<16} {'edges r/s':<19} {'energy(deg)':>11} {'W1(deg)':>10} "
       f"{'energy(IET)':>12} {'W1(IET)':>14} {'motifL1':>8} {'C2ST':>7} {'AUC':>7}")
w(hdr); w("-" * len(hdr))
for k, v in d.items():
    a = v["advanced"]; l2 = a["layer2_distribution_distances"]
    dg, ie = l2["total_degree"], l2["inter_event"]
    c = a["layer4_discriminative"]
    sz = f"{v['real']['edges']:,}/{v['synth']['edges']:,}"
    w(f"{k:<16} {sz:<19} {dg['energy']:>11.3f} {dg['wasserstein']:>10.3f} "
      f"{ie['energy']:>12.3f} {ie['wasserstein']:>14.3f} {a['layer3_temporal_motifs']['l1_distance']:>8.3f} "
      f"{c['discriminative_score']:>7.4f} {c['auc']:>7.4f}")
w("")
w("Anderson-Darling (also implemented; higher = more different):")
for k, v in d.items():
    l2 = v["advanced"]["layer2_distribution_distances"]
    w(f"  {k:<16} AD(total-degree)={l2['total_degree']['anderson_darling']:>10.3f}   "
      f"AD(inter-event)={l2['inter_event']['anderson_darling']:>12.3f}")
w("")
w("Per-run sizes / spans actually used:")
for k, v in d.items():
    r = v["real"]
    cap = f" (time-prefix capped to {r['time_prefix_capped_to']:,})" if r["time_prefix_capped_to"] else ""
    w(f"  {k:<16} real {r['edges']:>9,} edges / {r['nodes']:>7,} nodes / span {r['span_days']:>9.3f} d{cap}")
    w(f"  {'':<16} synth {v['synth']['edges']:>8,} edges / {v['synth']['nodes']:>7,} nodes  [domain={v['domain']}]")
w("")
w("2-node temporal-motif fingerprints (8-vector, delta=3600s) — bits are edge directions within a pair:")
for k, v in d.items():
    m = v["advanced"]["layer3_temporal_motifs"]
    w(f"  {k}")
    w(f"    real : {[round(x,4) for x in m['real_fingerprint']]}")
    w(f"    synth: {[round(x,4) for x in m['synth_fingerprint']]}")

if fx:
    w("")
    w("=" * 108)
    w("ADDENDUM — bipartite datasets re-run with the user/item ID-namespace collision FIXED")
    w("=" * 108)
    hdr2 = (f"{'real graph':<16} {'nodes buggy':>12} {'nodes fixed':>12} {'energy(deg)':>11} "
            f"{'W1(deg)':>10} {'motifL1':>8} {'C2ST':>7} {'AUC':>7}")
    w(hdr2); w("-" * len(hdr2))
    for k, v in fx.items():
        if "error" in v:
            w(f"{k:<16} FAILED: {v['error'][:60]}"); continue
        a = v["advanced"]; dg = a["layer2_distribution_distances"]["total_degree"]
        c = a["layer4_discriminative"]
        w(f"{k:<16} {v['nodes_buggy_merged']:>12,} {v['nodes_namespace_fixed']:>12,} "
          f"{dg['energy']:>11.3f} {dg['wasserstein']:>10.3f} "
          f"{a['layer3_temporal_motifs']['l1_distance']:>8.3f} "
          f"{c['discriminative_score']:>7.4f} {c['auc']:>7.4f}")
    w("")
    for k, v in fx.items():
        if "error" in v: continue
        m = v["advanced"]["layer3_temporal_motifs"]
        w(f"  {k} (namespace-fixed) motif fingerprints:")
        w(f"    real : {[round(x,4) for x in m['real_fingerprint']]}")
        w(f"    synth: {[round(x,4) for x in m['synth_fingerprint']]}")
print("\n".join(L))
