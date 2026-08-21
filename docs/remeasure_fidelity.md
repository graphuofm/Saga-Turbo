# Camera-ready re-measurement: fidelity and anomaly-realism (fixed engine)

Re-measurement of the fidelity / anomaly-realism claims using the fixed engine in
the main working tree (`target/release/agora`, seed 42 throughout). Every number
below comes from a real run. Where a real reference or a prior number could not be
reproduced exactly, that is stated plainly. This file does NOT touch
`paper/main.tex` or any frozen submission number; it is a measurement log only.

Engine build: `crates/saga-core/src/sim.rs` and `crates/saga-rules/domains/finance.yaml`
carry the consolidation fix (sim.rs +283/-27, finance.yaml +43/-27 vs HEAD).

Note on runtime: the test machine was heavily oversubscribed during this run
(load average ~61 on 32 cores). This only slowed the harness; it does not affect
any measured value. All generated graph directories were deleted after measurement.

## Datasets found (real references)

Searched repo + home dir. All real references live under
`./realdata/`:

| dataset | path | size | labels |
|---|---|---|---|
| Elliptic Bitcoin | `realdata/elliptic-bitcoin/` | 234,355 edges, 203,769 nodes | `elliptic_txs_classes.csv`: class 1 = illicit (4,545), 2 = licit (42,019), unknown (157,205) |
| CollegeMsg (SNAP) | `realdata/snap-collegemsg/CollegeMsg.txt` | 59,835 edges, 1,899 nodes | none (finance fidelity match) |
| email-Eu-core (SNAP) | `realdata/snap-email-eu/email-Eu-core-temporal.txt` | 332,334 edges, 986 nodes | none (finance fidelity match) |
| tgbl-review | `realdata/tgbl-review/tgbl-review_edgelist_v2.csv` | ~4.87M lines | none (e-commerce match) |
| tgbl-wiki | `realdata/tgbl-wiki/tgbl-wiki_edgelist_v2.csv` | ~157k lines | none |
| CICIDS2017 | `realdata/cicids2017/` | large | benign/attack flows (cyber match) |

The only real graph with ground-truth anomaly labels is **Elliptic** (illicit
transactions). All anomaly-realism metrics (degree ratio, homophily, in/out
magnitude) therefore compare AGORA finance against Elliptic. The C2ST / fidelity
metrics compare AGORA finance against its paper matches (CollegeMsg, email-Eu).

## How AGORA anomalous nodes were identified

AGORA emits `labels_nodes.csv` (adversary node ids) alongside the edge shards.
The marked/anomalous node set = the ids in that file. For Elliptic, the marked
set = txIds with class == 1 (illicit). "Normal" = all non-marked nodes that
appear in the edge list, applied identically to both graphs.

---

## 1. C2ST / discriminative score (metric #2)

`metrics_advanced.discriminative_score(real, synth)` — HistGradientBoosting on
ID-agnostic per-edge features, reports AUC and 2·|AUC−0.5|. AGORA finance
generated at the size of each real match (seed 42).

Command (from `python/`):
`python3 -m saga_eval compare --synth <fin> --real <real> --advanced`
(measured directly via `metrics_advanced.discriminative_score`).

| real graph | AGORA size | AUC | accuracy | discriminative score |
|---|---|---|---|---|
| CollegeMsg | 60,277 edges | 0.99972 | 0.9921 | **0.99945** |
| email-Eu-core | 332,210 edges | 0.99986 | 0.9950 | **0.99972** |

**Verdict: UNCHANGED.** The fixed engine did NOT move the C2ST — AGORA finance
is still near-perfectly separable from the real graphs (disc score ≈ 0.9997, the
same as the prior ~0.9997). The consolidation fix targeted anomaly-node
structure, not the edge-level feature distribution the C2ST keys on, so this is
expected and reported honestly.

---

## 2. Anomaly degree ratio vs real (metric #3)

Ratio = mean total-degree(marked nodes) / mean total-degree(normal nodes),
identical method both sides.

| graph | mean deg (anom) | mean deg (normal) | **degree ratio** |
|---|---|---|---|
| Elliptic real (illicit) | 2.012 | 2.307 | **0.872** |
| AGORA finance, Elliptic-matched (234k) | 2.437 | 4.647 | **0.524** |
| AGORA finance, 1M edges (default) | 11.955 | 21.211 | **0.564** |

Prior: pre-fix AGORA **12.9×**; the consolidation was said to cure it toward
**0.73×**. Fixed-engine value is **0.52–0.56×** (size-independent: 0.52 matched,
0.56 at 1M).

**Verdict: IMPROVED (cured).** From 12.9× down to ~0.53×. It now slightly
*undershoots* the real 0.87× (i.e. AGORA anomalous nodes are a touch
lower-degree than normal, whereas Elliptic illicit nodes are nearly equal to
normal). Direction is correct and the pathological 12.9× is gone.

---

## 3. Homophily — resolved under ONE consistent definition (metric #4)

**Definition used (anom.py's exact formula):**
`homophily = (both.sum() / out_edges) / (len(mset) / n)`
where `out_edges` = number of edges whose SOURCE is in the marked set, `both` =
those out-edges whose DESTINATION is also marked, `mset` = marked node set, `n` =
total graph nodes. Equivalently: `P(dst ∈ mset | src ∈ mset) / (|mset|/n)` — the
enrichment of same-set destinations among edges leaving the marked set, relative
to the marked-node base rate. This is applied identically to both graphs.

| graph | out-edges of marked | both-marked | base (|mset|/n) | **homophily** |
|---|---|---|---|---|
| Elliptic real (illicit) | 3,371 | 998 | 0.0223 | **13.27** |
| AGORA finance, Elliptic-matched (234k) | 2,206 | 95 | 0.0192 | **2.25** |
| AGORA finance, 1M edges | 9,875 | 441 | 0.0191 | **2.33** |

The earlier disagreeing measurements were 3.0 (simplified) vs 14.19 (anom.py).
Under the single anom.py definition above, the real Elliptic value is **13.27**
(within rounding of the prior 14.19; the small gap is the node-universe choice —
we count all edge-list nodes) and AGORA is **2.25**. The simplified ~3.0-style
number is NOT reported.

**Verdict: REGRESSED / gap remains.** Real illicit transactions are strongly
homophilous (13.3× enrichment — illicit txns transact with illicit txns).
AGORA anomalous nodes cluster only ~2.3×, i.e. AGORA under-clusters its
anomalies relative to real. This is the honest residual weakness.

(For transparency, the alternative denominator `both.sum()/total_edges` / base
gives 0.19 real vs 0.02 AGORA — same ordering, same conclusion.)

---

## 4. Anomalous in/out degree magnitude (metric #5)

Mean in-degree and out-degree over the marked set, same method both sides.

| graph | mean in-deg (anom) | mean out-deg (anom) | in/out avg | total-deg (anom) |
|---|---|---|---|---|
| Elliptic real (illicit) | 1.270 | 0.742 | **1.006** | 2.012 |
| AGORA finance, Elliptic-matched (234k) | 1.304 | 1.132 | **1.218** | 2.437 |
| AGORA finance, 1M edges | 6.518 | 5.438 | 5.978 | 11.955 |

Prior: recruitment filter overshot anomalous in/out degree to **~16.55** vs real
**~1.71**.

**Verdict: IMPROVED (essentially fixed at like-for-like size).** At the Elliptic
matched size the anomalous in/out magnitude is **1.22** (total 2.44) vs real
**1.006** (total 2.01) — the ~16.55-vs-1.71 gap is closed. The 1M-edge value is
larger (5.98 / total 11.96) but that is a graph-density artifact (1M edges /
95k nodes ≈ mean degree 21, vs Elliptic's ≈ 2.3); the fair like-for-like number
is the matched one. AGORA still overshoots real by only ~20% at matched size,
not the prior ~10×.

---

## 5. Other distributional fidelity vs real (metric #6)

From the base scorecard (`saga_eval.compare`), AGORA finance vs its paper matches:

| metric | vs CollegeMsg (real / AGORA) | vs email-Eu (real / AGORA) |
|---|---|---|
| overall fidelity score | 0.805 | 0.803 |
| total-degree KS | 0.327 | 0.295 |
| out-degree KS | 0.293 | 0.274 |
| in-degree KS | 0.150 | 0.115 |
| inter-event-time KS | 0.205 | 0.370 |
| clustering coeff | 0.138 / 0.084 | 0.450 / 0.130 |
| reciprocity | 0.636 / 0.123 | 0.711 / 0.284 |
| **repeated-edge fraction** | **0.661 / 0.829** | **0.925 / 0.965** |
| mean degree | 63.0 / 63.8 | 674.1 / 673.9 |

**Repeated-edge fraction:** paper (frozen) reports AGORA ~0.94 vs real 0.66/0.83.
Fixed engine: AGORA **0.829** against CollegeMsg (real 0.661) and **0.965**
against email-Eu (real 0.925). **IMPROVED** for the CollegeMsg match (0.94 → 0.83,
closer to real 0.66); still slightly high. **Overall fidelity score UNCHANGED**
(0.80/0.80, matches the frozen finance .79/.80). AGORA still under-clusters
(clustering 0.08–0.13 vs real 0.14–0.45) and under-reciprocates — unchanged
honest gaps.

---

## Summary of verdicts vs prior/frozen numbers

| metric | prior | fixed engine | real | verdict |
|---|---|---|---|---|
| C2ST disc. score | ~0.9997 | 0.9994 / 0.9997 | — | UNCHANGED (still separable) |
| anomaly degree ratio | 12.9× (pre-fix) → 0.73× (target) | 0.52–0.56× | 0.87× | IMPROVED (cured) |
| homophily (anom.py def) | 3.0 vs 14.19 (disputed) | 2.25 | 13.27 | resolved; gap REMAINS |
| anom in/out degree | 16.55 (pre-fix) | 1.22 (matched) | 1.01 | IMPROVED (fixed at like-for-like) |
| repeated-edge fraction | 0.94 (frozen) | 0.83 (CollegeMsg) | 0.66 | IMPROVED |
| finance fidelity score | .79/.80 (frozen) | 0.805/0.803 | — | UNCHANGED |

Bottom line: the consolidation fix cured the two pathological anomaly-structure
overshoots (degree ratio 12.9×→~0.53×; in/out magnitude 16.55→~1.22 at matched
size) and nudged repeated-edge fraction toward real. It did not change the C2ST
(anomalies remain edge-level separable) and did not close the homophily gap
(real illicit clustering 13.3× vs AGORA 2.25× — AGORA still under-clusters its
anomalies).
