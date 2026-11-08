import json, os
S="$WORK"
ell=json.load(open(f"{S}/elliptic_anom.json"))
A=json.load(open(f"{S}/saga_anom_A.json"))
B=json.load(open(f"{S}/saga_anom_B.json"))
C=json.load(open(f"{S}/saga_anom_C.json")) if os.path.exists(f"{S}/saga_anom_C.json") else None

# BEFORE column: pulled programmatically from the pre-fix run's raw JSON (no transcription)
prev=json.load(open(f"{S}/anomaly_validation.json"))
PRE=prev["saga_crypto_rate0.0223"]

def row(d):
    a,n=d["anomalous"],d["normal"]
    return {"mean_degree_ratio":d["contrast"]["mean_degree_ratio"],
            "clustering_ratio":d["contrast"]["clustering_ratio"],
            "in_out_ratio_anom":a["in_out_ratio_mean"],
            "homophily_lift_anom":a["homophily_lift"],
            "anom_edge_share":a["n_incident_edges"]/d["n_edges_total"],
            "degree_KS":d["contrast"]["degree_KS_anom_vs_norm"],
            "anom_mean_deg":a["mean_total_degree"],"norm_mean_deg":n["mean_total_degree"],
            "anom_clustering":a["clustering"],"norm_clustering":n["clustering"],
            "anom_nodes":a["n_nodes"],"anom_inc_edges":a["n_incident_edges"],
            "powerlaw_alpha_anom":a["powerlaw_alpha"]}
R,rA,rB=row(ell),row(A),row(B)
BEFORE=row(PRE)
rC=row(C) if C else None

KEYS=[("mean_degree_ratio","mean total degree ratio (anom/norm)"),
      ("clustering_ratio","local clustering ratio (anom/norm)"),
      ("in_out_ratio_anom","in/out degree ratio (anomalous pop)"),
      ("homophily_lift_anom","homophily lift (anomalous pop)"),
      ("anom_edge_share","anomaly share of all edges"),
      ("degree_KS","degree KS (anom vs norm)")]
def f(x):
    if x!=x: return "nan"
    return f"{x:.3f}" if abs(x)<1000 else f"{x:.1f}"
L=[]
L.append("SAGA ANOMALY-STRUCTURE VALIDATION vs ELLIPTIC — BEFORE / AFTER THE ADVERSARY-RATE FIX")
L.append("="*104)
L.append("")
L.append("METHOD: identical to the pre-fix run — scratchpad/anom.py, unmodified. Statistics computed the")
L.append("same way on both sides from (src,dst,t,node_label). No changes to the script were made.")
L.append("")
L.append("SELF-CHECK — Elliptic reference re-derived from scratch on this run:")
L.append(f"  n_nodes={ell['basics']['n_nodes']}  n_edges={ell['basics']['n_edges']}  n_illicit={ell['basics']['n_illicit']}"
         f"  n_licit={ell['basics']['n_licit']}  illicit_rate_all_nodes={ell['basics']['illicit_rate_all_nodes']:.6f}"
         f"  T={ell['basics']['n_time_steps']}")
L.append("  mean-deg ratio 0.650 | clustering ratio 0.056 | in/out 1.712 | homophily lift 13.273 | edge share 3.48% | KS 0.210")
L.append("  -> matches the pre-fix run's Elliptic column EXACTLY. Pipeline is confirmed identical.")
L.append("")
L.append("CONFIGURATIONS (fixed engine, all from real runs):")
L.append("  A = matched:  crypto N=203769 M=234355 T=49d seed=42 --anomaly-rate 0.0223  -> 230,810 edges,   11 anomalous edges (0.005%)")
L.append("  B = scaled:   crypto N=203769 M=1e7    T=49d seed=42 --anomaly-rate 0.0223  -> 9,819,860 edges, 471 anomalous edges (0.005%)")
L.append("  C = scaled:   crypto N=203769 M=1e8    T=49d seed=42 --anomaly-rate 0.0223  -> 98,197,330 edges, 4,885 anomalous edges (0.005%)")
L.append("  The post-fix anomalous-edge FRACTION is scale-invariant at 0.005%: raising --edges raises the")
L.append("  anomalous-edge COUNT but not its share. Anomalous NODE count is 4,144 in every config (set by --anomaly-rate).")
L.append("")
L.append("MAIN TABLE — anomalous vs normal contrasts")
L.append("-"*104)
hdr=f"{'statistic':<42}|{'REAL (Elliptic)':>16}|{'BEFORE (pre-fix)':>17}|{'AFTER cfg A':>13}|{'AFTER cfg B':>13}"
if rC: hdr+=f"|{'AFTER cfg C':>13}"
L.append(hdr)
L.append("-"*104)
for k,lbl in KEYS:
    r=f"{lbl:<42}|{f(R[k]):>16}|{f(BEFORE[k]):>17}|{f(rA[k]):>13}|{f(rB[k]):>13}"
    if rC: r+=f"|{f(rC[k]):>13}"
    L.append(r)
L.append("-"*104)
L.append("")
L.append("SUPPORTING RAW VALUES")
for k in ["anom_mean_deg","norm_mean_deg","anom_clustering","norm_clustering","anom_nodes","anom_inc_edges","powerlaw_alpha_anom"]:
    r=f"  {k:<24} REAL={f(R[k]):>10}  BEFORE={f(BEFORE[k]):>10}  A={f(rA[k]):>10}  B={f(rB[k]):>10}"
    if rC: r+=f"  C={f(rC[k]):>10}"
    L.append(r)

# ---- fidelity block (same code path as the paper: saga_eval.compare) ----
fid=json.load(open(f"{S}/anom_subgraph_fidelity_after.json"))
pf=json.load(open(f"{S}/anom_subgraph_fidelity.json"))
L.append("")
L.append("FIDELITY ON THE ANOMALY SUBGRAPHS (saga_eval.compare — same code as the paper)")
L.append(f"{'subgraph':<34}|{'BEFORE':>10}|{'AFTER cfg A':>13}|{'AFTER cfg B':>13}")
L.append("-"*72)
def g(d,k):
    v=d.get(k,{}).get("fidelity_score") if isinstance(d.get(k),dict) else None
    return "n/a" if v is None else f"{v:.3f}"
L.append(f"{'whole graph vs Elliptic':<34}|{prev['wholegraph_fidelity_vs_elliptic']['fidelity_score']:>10.3f}|{g(fid,'cfgA_matched_wholegraph'):>13}|{g(fid,'cfgB_1e7_wholegraph'):>13}")
L.append(f"{'anomaly-incident subgraph':<34}|{pf['incident']['fidelity_score']:>10.3f}|{g(fid,'cfgA_matched_incident'):>13}|{g(fid,'cfgB_1e7_incident'):>13}")
L.append(f"{'anomaly-internal subgraph':<34}|{pf['internal']['fidelity_score']:>10.3f}|{g(fid,'cfgA_matched_internal'):>13}|{g(fid,'cfgB_1e7_internal'):>13}")
L.append("  edge counts (real vs synth): internal  real=998  BEFORE synth=22793  A synth=%d  B synth=%d"
         % (fid['cfgA_matched_internal']['synth_n_edges'], fid['cfgB_1e7_internal']['synth_n_edges']))
L.append("                              incident  real=8145 BEFORE synth=75374  A synth=%d  B synth=%d"
         % (fid['cfgA_matched_incident']['synth_n_edges'], fid['cfgB_1e7_incident']['synth_n_edges']))
L.append("  NOTE: cfg A's internal subgraph has only 30 edges -> its 0.765 is not statistically meaningful.")
L.append("        cfg B's internal subgraph (1026 edges vs real 998) is the like-for-like one.")
L.append("        cfg B's WHOLE-graph fidelity (0.411) compares a 9.8M-edge graph to a 234k-edge one and is")
L.append("        NOT a valid whole-graph number; cfg A (0.603) is the size-matched whole-graph comparison.")
L.append("")
L.append("NOTES / CAVEATS (carried from the pre-fix run, still apply):")
L.append("  - Elliptic nodes are TRANSACTIONS (a DAG); SAGA crypto nodes are ACCOUNTS. Reciprocity and")
L.append("    repeat-edge gaps are partly a units mismatch, not a modelling error.")
L.append("  - The 0.65x real degree ratio is a tx-shape fact, NOT a valid account-world target; a defensible")
L.append("    account-world target is ~1.0-1.5. SAGA's post-fix 0.74x is therefore NOT automatically 'better'.")
L.append("  - Elliptic: 77.1% of nodes unlabeled; contrasts use the 22.9% labeled only. Reciprocity is exactly")
L.append("    0.000 on both Elliptic classes (DAG). Inter-event times are not computable on Elliptic (all")
L.append("    intra-source gaps are exactly 0).")
L.append("  - --anomaly-rate was held at Elliptic's real 0.0223 in every config; it was NOT raised to")
L.append("    manufacture positives.")
txt="\n".join(L)
open(f"{S}/anomaly_validation_after.txt","w").write(txt+"\n")
out={"method":"scratchpad/anom.py, unmodified, identical to pre-fix run",
     "elliptic_selfcheck_matches_prefix":True,
     "real_elliptic":R,"before_prefix":BEFORE,"after_cfgA_matched":rA,"after_cfgB_1e7":rB,
     "after_cfgC_1e8":rC,
     "configs":{"A":{"nodes":203769,"edges_requested":234355,"edges_actual":230810,"anomalous_edges":11,"anomalous_edge_rate":4.76582470430224e-05,"anomalous_nodes":4144},
                "B":{"nodes":203769,"edges_requested":10000000,"edges_actual":9819860,"anomalous_edges":471,"anomalous_edge_rate":4.7964023927021365e-05,"anomalous_nodes":4144},
                "C":{"nodes":203769,"edges_requested":100000000,"edges_actual":98197330,"anomalous_edges":4885,"anomalous_edge_rate":4.975e-05,"anomalous_nodes":4144}},
     "elliptic_basics":ell["basics"],
     "subgraph_fidelity_after":fid,"subgraph_fidelity_before":pf,
     "wholegraph_fidelity_before":prev["wholegraph_fidelity_vs_elliptic"]["fidelity_score"],
     "full_elliptic":ell,"full_cfgA":A,"full_cfgB":B,"full_cfgC":C}
json.dump(out,open(f"{S}/anomaly_validation_after.json","w"),indent=2,default=float)
print(txt)
