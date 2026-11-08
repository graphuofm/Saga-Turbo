#!/usr/bin/env python3
"""Render the JOB 1 tables: tab:downstream + tab:downmulti, BEFORE vs AFTER.
BEFORE for downmulti is read programmatically from before_downstream.json
(the pre-fix j6_down.py run). BEFORE for tab:downstream is the published paper
value AND the pre-fix binary re-measured in this run's own controlled A/B."""
import json

S = "$WORK"
L = []

# ------------------------------------------------------------------ downstream
rows = {r["graph"]: r for r in json.load(open(f"{S}/downstream_after.json"))}
PUB = {"real_CollegeMsg": dict(tgn_ap=0.843, eb_inf_ap=0.764, recurring_edge_frac=0.83),
       "SAGA_fixed": dict(tgn_ap=0.943, eb_inf_ap=0.942, recurring_edge_frac=0.94),
       "BA_random_ts": dict(tgn_ap=0.611, eb_inf_ap=0.500, recurring_edge_frac=0.0)}

L.append("TAB:DOWNSTREAM — future link prediction, test AP (re-measured on the fixed engine)")
L.append("=" * 100)
L.append("NOTE: the exact generate command behind the published /tmp/sagatgn_A is not recorded in the")
L.append("repo, so it CANNOT be replayed byte-for-byte. We instead ran the documented domain-matched")
L.append("convention (finance --nodes 1899 --edges 59835 --time-span 193.71 --seed 42 --anomaly-rate 0.02,")
L.append("identical to fidelity_driver.py) through BOTH the pre-fix binary (/tmp/saga_base) and the fixed")
L.append("binary. The SAGA_prefix column is therefore the controlled BEFORE; the published column is the")
L.append("paper's number under an unknown config and is shown only as a target, not as a measurement.")
L.append("")
order = ["real_CollegeMsg", "SAGA_prefix", "SAGA_fixed", "BA_random_ts"]
L.append(f"{'graph':<18}{'nodes':>8}{'edges':>9}{'TGN AP':>9}{'TGN AUC':>9}{'EB_inf AP':>11}{'EB_tw AP':>10}{'recur.frac':>12}")
for g in order:
    r = rows.get(g)
    if not r:
        L.append(f"{g:<18}  MISSING")
        continue
    if r.get("status") != "OK":
        L.append(f"{g:<18}  {r.get('status')}")
        continue
    L.append(f"{g:<18}{r['n_nodes']:>8}{r['n_edges']:>9}{r['tgn_ap']:>9.3f}{r['tgn_auc']:>9.3f}"
             f"{r['eb_inf_ap']:>11.3f}{r['eb_tw_ap']:>10.3f}{r['recurring_edge_frac']:>12.3f}")
L.append("")
L.append("published paper values (tab:downstream, pre-fix, unknown SAGA config):")
L.append(f"  TGN:      real 0.843 | SAGA 0.943 | BA 0.611")
L.append(f"  EdgeBank: real 0.764 | SAGA 0.942 | BA 0.500")
L.append(f"  recurring-edge fraction: real 0.83 | SAGA 0.94 | BA ~0")
L.append("")

# The decisive quantity: is SAGA EASIER than reality, and did the fix change it?
rr = rows.get("real_CollegeMsg", {})
for tag in ("SAGA_prefix", "SAGA_fixed"):
    r = rows.get(tag, {})
    if r.get("status") == "OK" and rr.get("status") == "OK":
        L.append(f"  {tag:<12} TGN gap (SAGA - real) = {r['tgn_ap']-rr['tgn_ap']:+.3f} | "
                 f"EdgeBank gap = {r['eb_inf_ap']-rr['eb_inf_ap']:+.3f} | "
                 f"recur-frac gap = {r['recurring_edge_frac']-rr['recurring_edge_frac']:+.3f}")
L.append("  (published pre-fix gaps, for reference: TGN +0.100, EdgeBank +0.178)")
L.append("  POSITIVE gap = SAGA is MORE predictable than real data = easier than reality.")
L.append("")
L.append("")

# ------------------------------------------------------------------- downmulti
L.append("TAB:DOWNMULTI — 6 SAGA domains vs size-matched BA, 2e5 events (re-measured)")
L.append("=" * 100)
L.append("BEFORE read programmatically from before_downstream.json (pre-fix j6_down.py run).")
L.append("AFTER = identical protocol, fixed binary (after_downmulti.py).")
L.append("")
before = {r["domain"]: r for r in json.load(open(f"{S}/before_downstream.json"))}
after = {r["domain"]: r for r in json.load(open(f"{S}/downmulti_after.json"))}
L.append(f"{'domain':<11}{'TGN saga':>19}{'TGN ba':>17}{'EB saga':>18}{'EB ba':>16}")
L.append(f"{'':<11}{'before -> after':>19}{'before -> after':>17}{'before -> after':>18}{'before -> after':>16}")


def cell(b, a, k, w):
    bv = b.get(k) if b else None
    av = a.get(k) if a else None
    fb = f"{bv:.3f}" if isinstance(bv, float) else "n/a"
    fa = f"{av:.3f}" if isinstance(av, float) else ("SKIP" if not a else a.get("status", "n/a")[:8])
    return f"{fb}->{fa}".rjust(w)


doms = ["finance", "crypto", "cyber", "transport", "ecommerce", "healthcare"]
acc = {k: ([], []) for k in ("tgn_saga", "tgn_ba", "eb_saga", "eb_ba")}
for d in doms:
    b, a = before.get(d), after.get(d)
    if a and a.get("status") != "OK":
        L.append(f"{d:<11}  {a.get('status')}")
        continue
    if not a:
        L.append(f"{d:<11}  NOT RUN / SKIPPED")
        continue
    L.append(f"{d:<11}" + cell(b, a, "tgn_saga", 19) + cell(b, a, "tgn_ba", 17)
             + cell(b, a, "eb_saga", 18) + cell(b, a, "eb_ba", 16))
    for k in acc:
        if b and isinstance(b.get(k), float):
            acc[k][0].append(b[k])
        if isinstance(a.get(k), float):
            acc[k][1].append(a[k])
L.append("-" * 100)
mean = {k: (sum(v[0]) / len(v[0]) if v[0] else float("nan"),
            sum(v[1]) / len(v[1]) if v[1] else float("nan")) for k, v in acc.items()}
L.append(f"{'MEAN':<11}" + f"{mean['tgn_saga'][0]:.3f}->{mean['tgn_saga'][1]:.3f}".rjust(19)
         + f"{mean['tgn_ba'][0]:.3f}->{mean['tgn_ba'][1]:.3f}".rjust(17)
         + f"{mean['eb_saga'][0]:.3f}->{mean['eb_saga'][1]:.3f}".rjust(18)
         + f"{mean['eb_ba'][0]:.3f}->{mean['eb_ba'][1]:.3f}".rjust(16))
L.append(f"  (means over the {len(acc['tgn_saga'][1])} domains present in BOTH columns)")
L.append("  published pre-fix means: TGN SAGA 0.964 / BA 0.697 ; EdgeBank SAGA 0.842 / BA exactly 0.500")
open(f"{S}/downstream_after.txt", "w").write("\n".join(L) + "\n")
print("\n".join(L))
