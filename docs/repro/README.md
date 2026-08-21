# Reproduction scripts (audit trail for the fixed-engine re-measurements)

These are the measurement scripts and summary result files that produced the
numbers in the camera-ready paper (the fixed engine). They are preserved here for
audit and re-run; the authoritative protocol and before/after numbers live in
`docs/PAPER_SPEC.md`, `docs/ENGINE_ISSUES.md`, and the `docs/remeasure_*.md`
files. Multi-megabyte raw JSON dumps from the original session scratchpad were
not copied; each script regenerates its own outputs from the fixed-engine binary
(`./target/release/saga`) at seed 42.

Naming: `*_after*` / `*_clean*` / `*_final*` are the fixed-engine runs that back
the camera-ready; earlier bare/`before_*` variants are the pre-fix baselines kept
for the side-by-side.

## Key drivers -> what they produced

| script(s) | metric / table | doc |
|---|---|---|
| `anom.py` | homophily = (both/out_edges)/(M/N), degree ratio, in/out (the anomaly-realism metric definitions) | remeasure_fidelity.md; ENGINE_ISSUES P1-a |
| `measure_p1a.py` | P1-a homophily calibration sweep (2.25 -> 12.99 vs Elliptic 13.27) | ENGINE_ISSUES P1-a; PAPER_SPEC F3 |
| `fid_final.py`, `fidelity_driver.py`, `fidelity_after.json` | 7-graph fidelity + B + recurrence | remeasure_fidelity.md, remeasure_tab_fidelity.md; tab:fidelity |
| `remeasure_agora_row.py`, `job2_matrix_after.py` | fig_matrix AGORA row (final engine) | remeasure_matrix_agora.md; fig_matrix |
| `diff.sh`, `run_diff.py`, `diff_results.csv` | difficulty AP + AUC sweep (finance; crypto for P2-b) | remeasure_diff_ap.md; tab:diff |
| `job3_learned_clean.py`, `job3_finance_fix.py` | learned-generator contest (toy scale, finance nan fix) | remeasure_downstream_learned.md; tab:learned |
| `after_downmulti_clean.py`, `downmulti_clean.json` | downstream multi-domain (TGN/EdgeBank AP) | remeasure_sweeps.md; tab:downmulti |
| `after_downstream_college.py` | downstream single (CollegeMsg-matched) | tab:downstream |
| `run_gpi_after.py`, `j1_gpi.py`, `fit_gpi.py` | g(pi) elasticity sweep (6 domains x 5 pi) | remeasure_sweeps.md; controllable-rate |
| `transport_fid.py`, `run_transport.py`, `job4_bipartite_fix.py` | transport fidelity fix 0.694 -> 0.743 | ENGINE_ISSUES P3-a(F1); tab:multidomain |
| `health_probe.py`, `health_types.py` | healthcare fidelity fix 0.696 -> 0.754 | ENGINE_ISSUES P3-a(F1-health) |
| `thrpt.sh`, `scalecurve.sh`, `scaleout.sh`, `bench.py` | throughput / scaling (1e7..1e9) | remeasure_sweeps.md; tab:scale |
| `union_hash.py` | shard-union bit-identity check | S2 determinism |
| `build.py`, `aggregate.py`, `mkreport*.py`, `make_table.py` | driver/aggregation/report helpers | (glue) |

The `docs/remeasure_*.md` files are the source of truth for the exact numbers;
this directory is the code that produced them.
