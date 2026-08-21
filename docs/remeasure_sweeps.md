# Camera-ready re-measurement of the mechanical-sweep tables

Re-measured with the FIXED engine in the main working tree
(`./target/release/agora`), seed 42 throughout, every number from a real run.
This file does not touch `paper/main.tex` or any frozen submission number; it is a
side-by-side check of the fixed engine against the frozen tables.

Machine: 13th Gen Intel Core i9-13900K, 32 threads (8P+16E), Linux.
Determinism: fixed seed 42 for generation; detector/TGN/EdgeBank use their harness
default seed 0. Every generated graph was deleted immediately after measurement.

Note on host state during this run: the host was under heavy concurrent external
load from processes outside this task (three unrelated Python jobs each pinning
~10 cores, loadavg ~65-106 on a 32-core box) for the entire session. This does
NOT affect the fidelity/AUC/AP numbers (those are deterministic functions of the
data, independent of wall-clock), but it does contaminate the throughput table
(Table 4), which is wall-clock-bound. See that table's note.

---

## Table 1 - g(pi) elasticity: realized anomalous-edge fraction

Setup: `agora generate --domain <d> --edges 1000000 --seed 42 --anomaly-rate <pi>`
for each of the six domains and pi in {0.005, 0.01, 0.02, 0.05, 0.10}. Value is
`anomalous_edges / edges_written` from the run's own JSON summary (percent of edges).

| domain      | pi=0.005 | 0.01   | 0.02   | 0.05   | 0.10   | fit (frac ~ k*pi) |
|-------------|---------:|-------:|-------:|-------:|-------:|------------------:|
| finance     |  0.036%  | 0.077% | 0.157% | 0.408% | 0.819% | k ~ 0.082 |
| crypto      |  0.006%  | 0.010% | 0.024% | 0.062% | 0.126% | k ~ 0.013 |
| cyber       |  0.006%  | 0.017% | 0.028% | 0.067% | 0.133% | k ~ 0.013 |
| ecommerce   |  0.201%  | 0.414% | 0.861% | 2.137% | 4.338% | k ~ 0.434 |
| healthcare  |  0.221%  | 0.393% | 0.820% | 2.069% | 4.414% | k ~ 0.441 |
| transport   |  0.010%  | 0.022% | 0.033% | 0.090% | 0.191% | k ~ 0.019 |

Monotone in pi for all six domains (verified). The 0.30 (30%) cap never binds at
pi<=0.10: the largest realized fraction anywhere in the table is 4.4% (healthcare,
pi=0.10).

**Diff vs frozen submission: structurally different (much lower).** The frozen paper
states g(pi) ~ 3*pi (finance: pi=0.01/0.02/0.05/0.10 -> 3.1%/6.8%/15.9%/30.0%,
R^2~1, cap binds at pi=0.10). The fixed engine's realized fraction is far smaller and
now clearly domain-dependent: finance is g(pi) ~ 0.082*pi (about 37x lower than the
frozen 3*pi), the lightest domains (crypto/cyber) ~0.013*pi, and the heaviest
(ecommerce/healthcare) ~0.43*pi. The qualitative claims (monotone, controllable, cap
does not bind at pi<=0.10) still hold; the amplification magnitude does not. The
frozen "~3*pi, cap binds at 0.10" numbers are no longer reproducible.

---

## Table 2 - Difficulty controls detectability (tab:diff)

Setup: finance, `--edges 500000 --seed 42 --anomaly-rate 0.05 --difficulty <delta>`,
delta in {0, 0.25, 0.5, 0.75, 1.0}. Detector: `saga_eval.detector.detect_auc`
(HistGradientBoosting, 200 iters, 70/30 stratified split, harness seed 0), read in
three feature views: attribute-only, structure-only (endpoint degrees), full.
Same three views as the frozen table. ROC-AUC and AP both reported.

ROC-AUC by view:

| view              | delta=0 | 0.25  | 0.5   | 0.75  | 1.0   |
|-------------------|--------:|------:|------:|------:|------:|
| attribute-only    | 0.843   | 0.843 | 0.878 | 0.920 | 0.923 |
| structure-only    | 0.941   | 0.941 | 0.938 | 0.938 | 0.948 |
| full (all feats)  | 0.868   | 0.914 | 0.916 | 0.905 | 0.914 |

Average precision (AP) by view:

| view              | delta=0 | 0.25  | 0.5   | 0.75  | 1.0   |
|-------------------|--------:|------:|------:|------:|------:|
| attribute-only    | 0.402   | 0.350 | 0.270 | 0.164 | 0.091 |
| structure-only    | 0.056   | 0.055 | 0.054 | 0.055 | 0.067 |
| full (all feats)  | 0.532   | 0.509 | 0.388 | 0.308 | 0.381 |

Anomalous-edge count in the training graph: 1787, 1915, 2038, 2278, 2633 of ~498,800
edges (0.36% -> 0.53%), i.e. the honest base rate is well under 1% at pi=0.05 (a direct
consequence of Table 1's lower g(pi)).

**Diff vs frozen submission: structurally different / non-monotone on AUC, but the
paper's core narrative holds on AP.** The frozen table has attribute-only AUC falling
0.995->0.968, structure-only flat ~0.941, full ~0.998->0.991. Re-measured with the
fixed engine: structure-only AUC is still flat at ~0.94 (matches the frozen story that
the knob does not camouflage structure). But because the realized anomaly base rate is
now ~0.4% (not the frozen 15.9%), positives are scarce and ROC-AUC is both lower and
non-monotone: attribute-only AUC actually *rises* with delta (0.843 -> 0.923) and full
AUC wobbles ~0.87-0.92 rather than sitting at ~0.99. The real camouflage effect shows
up cleanly in AP, exactly as the paper argues ("ROC-AUC understates it; average
precision shows it"): attribute-only AP falls monotonically 0.402 -> 0.091 (a 4.4x
drop) as delta rises. So the mechanism (knob hits attributes, not structure) reproduces
on AP; the specific high, monotone AUC magnitudes in the frozen table do not.

---

## Table 3 - Downstream utility for future link prediction (tab:downstream)

Setup: real = CollegeMsg (1,899 nodes, 59,835 edges). SAGA = finance scale-matched to
CollegeMsg (`--nodes 1899 --edges 59835 --time-span 194 --seed 42 --no-anomalies`,
59,046 edges emitted). BA = Barabasi-Albert with random timestamps sized to the real
graph (`saga_eval.baselines.gen_ba`, seed 42). Models: PyG TGN (6 epochs, msg zeroed,
seed 0) via `tgn_eval.run_tgn`; EdgeBank (inf + time-window) via
`downstream_extra.run_edgebank`; both on the same chronological 70/15/15 split with
1 random-node negative per positive. Repeated-edge fraction = 1 - distinct_pairs/edges.

| downstream model            | real  | SAGA  | BA    |
|-----------------------------|------:|------:|------:|
| TGN (learned) test AP       | 0.841 | 0.927 | 0.622 |
| TGN test ROC-AUC            | 0.848 | 0.930 | 0.678 |
| EdgeBank_inf test AP        | 0.764 | 0.942 | 0.500 |
| EdgeBank_inf test ROC-AUC   | 0.776 | 0.944 | 0.496 |
| EdgeBank_tw test AP         | 0.742 | 0.836 | 0.500 |
| repeated-edge fraction      | 0.661 | 0.829 | 0.000 |

**Diff vs frozen submission: reproduces the frozen table, with two small honest
shifts.** The frozen tab:downstream has TGN 0.843/0.943/0.611, EdgeBank_inf
0.764/0.942/0.500, repeated-edge 0.66/0.94/~0. Re-measured: TGN real 0.841 (frozen
0.843) and BA 0.622 (frozen 0.611) match; EdgeBank_inf reproduces exactly
(0.764/0.942/0.500). Two differences: SAGA TGN AP is 0.927 vs the frozen 0.943 (slightly
less predictable), and the SAGA repeated-edge fraction is 0.829, not the frozen table's
0.94 (it matches the 0.827 recorded in docs/VALIDATION.md for the same scale-matched
finance-vs-CollegeMsg graph; the frozen table's 0.94 came from a higher-recurrence SAGA
config). The headline ordering SAGA ~ real >> BA, and BA hitting exactly chance (0.500)
under EdgeBank because it has no repeated edges, both reproduce.

---

## Table 4 - Throughput / scaling (tab:scale a, size curve, finance)

Setup: `agora generate --domain finance --edges <N> --seed 42`, wall clock and peak RSS
via `/usr/bin/time -v`; each graph deleted immediately after timing. `engine M/s` is the
run's own `events_per_sec` (edges / in-process wall time).

CPU: 13th Gen Intel Core i9-13900K, 32 threads.

| edges | wall  | throughput (measured) | peak RAM | host loadavg |
|-------|------:|----------------------:|---------:|-------------:|
| 1e6   | 6.28s | 0.16 M/s | 0.11 GB | 66.7 |
| 1e7   | 13.25s| 0.76 M/s | 0.25 GB | 67.8 |
| 1e8   | 48.35s| 2.07 M/s | 1.13 GB | 66.3 |

**Diff vs frozen submission: NOT a clean re-measurement (host was saturated by
external load); reported as a contention-degraded lower bound, not an engine
regression.** The frozen tab:scale (single-tenant, same i9-13900K) reports 1e7 in 2.0s
(4.9 M/s), 1e8 in 16.8s (5.9 M/s), peak RAM 0.26/1.34 GB. Two things to read carefully:

1. Peak RAM tracks the frozen table (0.11/0.25/1.13 GB here vs 0.26/-/1.34 GB frozen).
   RAM is independent of CPU contention, so this part is a genuine confirmation.
2. Wall-clock throughput is ~3x below frozen (1e8 in 48.35s vs 16.8s), but the host was
   under ~2x CPU oversubscription throughout (loadavg ~66 on a 32-core box, from three
   unrelated external Python jobs each pinning ~10 cores, none of them part of this
   task). Throughput also *climbs* with size here (0.16 -> 0.76 -> 2.07 M/s) because the
   fixed startup/scheduling overhead under contention amortizes over the larger run.
   Both signatures (RAM matches, throughput ~2-3x down under ~2x oversubscription, size
   scaling still linear) are consistent with CPU starvation, not an engine slowdown. A
   clean single-tenant re-measurement of the "1e8 in ~17s, 5-6 M/s" claim was not
   possible on this host during this session because it was continuously saturated by
   external processes outside this task's control. This is reported honestly rather than
   presenting a contaminated number as a validated throughput.

---

## Clean re-measurement (single-tenant host) - Throughput / scaling (tab:scale a, finance)

This section supersedes the contention-degraded Table 4 above. The earlier 1e8=48.35s
number was measured while three unrelated external Python jobs saturated the box
(loadavg ~66 on 32 cores). The host is now quiet. Before each run the 1-minute loadavg
was read from /proc/loadavg and confirmed well under the single-tenant guard (~8).

Setup: `./target/release/agora generate --domain finance --edges <N> --seed 42 --json`,
single process, nothing else running. `wall (engine)` is the run's own `wall_time_s`;
`throughput` is the run's own `events_per_sec` (= edges / in-process wall time);
`peak RAM` is Maximum resident set size from `/usr/bin/time -v`. Each graph was deleted
immediately after timing.

CPU: 13th Gen Intel Core i9-13900K (8P+16E), 32 threads. Fixed seed 42.

| edges | wall (engine) | throughput (measured) | peak RAM | loadavg (1-min, at run start) |
|-------|--------------:|----------------------:|---------:|------------------------------:|
| 1e6   |  0.380 s | 2.63 M/s | 0.105 GB | 4.13 |
| 1e7   |  2.022 s | 4.94 M/s | 0.275 GB | 4.19 |
| 1e8   | 18.999 s | 5.25 M/s | 1.352 GB | 4.24 |

(edges_written: 998,532 / 9,983,276 / 99,807,834. Peak RAM converted from
107,644 / 287,844 / 1,417,584 KiB.)

**Diff vs frozen submission: clean re-measurement now REPRODUCES the frozen tab:scale.**
The frozen paper states 1e7 in ~2 s and 1e8 in ~17 s at a measured 5-6 M labeled
edges/s. On the quiet host: 1e7 = 2.022 s (4.94 M/s) and 1e8 = 18.999 s (5.25 M/s),
both squarely in the frozen range; the 5.25 M/s at 1e8 matches the paper's "5-6 M/s"
headline. Peak RAM (0.105 / 0.275 / 1.352 GB) also matches frozen (~0.26 / 1.34 GB at
1e7 / 1e8). The 1e6 point runs at 2.63 M/s because fixed per-process startup is a larger
fraction of a 0.38 s run; throughput rises to its plateau by 1e7. This replaces the
contended 48.35 s / 2.07 M/s at 1e8: that slowdown was CPU starvation from external load,
not an engine regression, as the RAM (which is contention-independent and matched even
then) already indicated.
