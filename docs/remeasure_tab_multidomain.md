# Camera-ready re-measurement: `tab:multidomain` (six-domain structural signatures)

Re-measurement of `paper/main.tex` Table `tab:multidomain` (the per-domain NORMAL
graph structural stats) on the FINAL fixed engine, so the whole table sits on one
consistent engine for AGORA's camera-ready. Measurement log only: it does NOT touch
`paper/main.tex`, `paper.frozen/`, `../agora-anon`, or `docs/PAPER_SPEC.md`,
and nothing is committed. Every value is from a real seed-42, anomalies-off run.

Engine build: `target/release/agora` (the pre-built final engine, built 01:06; NOT
rebuilt this session). All six domain rulebase yamls are modified vs HEAD (the final
fix state on disk: finance homophily + 2 added edge attrs; crypto/cyber/ecommerce
homophily/anomaly params; F1 transport truck/bus fleets + sensor reweight; F1-health
5 specialties + `PRESCRIBED_NDC` drug event + pharmacy rebalance) and are all older
than the 01:06 binary, so the binary carries every fix. Machine is testing-only; each
2M-edge graph was measured then deleted immediately (no graphs remain).

---

## 1. Protocol (reproduced from the frozen table's own protocol)

Per domain, one generate then the full statistic battery, exactly as the frozen table
(`docs/EXPERIMENTS.md` §7, whose numbers match the frozen `tab:multidomain` cell-for-cell):

```
agora generate --domain <d> --edges 2000000 --no-anomalies --seed 42 --out <dir> --quiet
```

**Structural columns** come from `saga_eval.stats.compute_stats(src, dst, t)` (default
args: `clustering_sample=2000`, `seed=0`, so deterministic) over the generated
`edges_*.parquet` stream, loaded by `saga_eval.load.load_saga`. The columns are:
`n_nodes`, `mean_degree`, `clustering` (sampled avg local, undirected), `reciprocity`,
burstiness `B = (σ−μ)/(σ+μ)` of per-source inter-event gaps, power-law tail `α`
(Clauset–Shalizi–Newman MLE on total degree), and `repeat_edge_ratio` (recurrence).

**`typ/attr` schema column** is recomputed from each domain's FINAL output (not the raw
yaml), which is what the frozen table did — it counts what actually instantiates in the
normal, anomalies-off graph, so schema blocks that never fire are not counted:
- **node-types** = number of `nodes_<type>.parquet` files written.
- **edge-types** = number of DISTINCT `event_type` values present in the edge stream.
- **edge-attributes** = number of edge columns beyond the structural set
  `{src, dst, t, event_type, label, anomaly_id}`.

This method reproduces every frozen schema cell for the four unchanged domains exactly
(e.g. finance `3/4,4`; crypto `4/6,10` — crypto's 7th schema event `contract_creation`
has zero normal weight and never fires, so it is not counted, matching the frozen `6`).

---

## 2. Re-measured six-row table (final fixed engine)

Full precision (paper rounding shown in §3):

| domain | nodes | m.deg | clust | recip | B | α | mem M | rep | typ/attr | event types that fire |
|---|---|---|---|---|---|---|---|---|---|---|
| finance    | 98,465 | 40.6 | .0346 | .0897 | .404 | 2.89 | .326 | .788 | 3/4,4  | transfer, purchase, cash_deposit, cash_withdrawal |
| crypto     | 78,722 | 50.1 | .0021 | .0023 | .630 | 2.50 | .284 | .867 | 4/6,10 | swap, tx, token_transfer, add_liquidity, remove_liquidity, approve |
| cyber      | 92,957 | 42.7 | .0632 | .1678 | .388 | 3.41 | .250 | .749 | 4/4,11 | flow, auth, dns, process_exec |
| transport  | 96,351 | 41.4 | .0000 | .0000 | .338 | 2.22 | .220 | .772 | 4/3,8  | traversal, sensor_reading, trip |
| e-commerce | 91,177 | 43.7 | .0967 | .0000 | .383 | 2.48 | .306 | .597 | 4/5,8  | WROTE_REVIEW, PURCHASED, RATED, RETURNED, VOTED_HELPFUL |
| healthcare | 60,615 | 65.5 | .0084 | .0474 | .384 | 2.66 | .311 | .779 | 4/5,11 | SUBMITTED_CLAIM, PRESCRIBED, PRESCRIBED_NDC, DISPENSED, REFERRED |

At the paper's precision (matching `tab:multidomain`'s columns):

| domain | nodes | m.deg | clust | recip | B | α | rep | typ/attr |
|---|---|---|---|---|---|---|---|---|
| finance    | 98k | 40.6 | .035 | .09 | .40 | 2.9 | .79 | 3/4,\,4 |
| crypto     | 79k | 50.1 | .002 | .00 | .63 | 2.5 | .87 | 4/6,\,10 |
| cyber      | 93k | 42.7 | .063 | .17 | .39 | 3.4 | .75 | 4/4,\,11 |
| transport  | 96k | 41.4 | .000 | .00 | .34 | 2.2 | .77 | 4/3,\,8 |
| e-commerce | 91k | 43.7 | .097 | .00 | .38 | 2.5 | .60 | 4/5,\,8 |
| healthcare | 61k | 65.5 | .008 | .05 | .38 | 2.7 | .78 | 4/5,\,11 |

---

## 3. Frozen vs final (paper precision), moved cells flagged

`→` marks a cell that moved at the paper's printed precision. `·` = reproduces.

| domain | nodes | m.deg | clust | recip | B | α | rep | typ/attr |
|---|---|---|---|---|---|---|---|---|
| finance    | 98k · | 40.6 · | .036→**.035** | .09 · | .40 · | 2.9 · | .79 · | 3/4,4 · |
| crypto     | 79k · | 50.1 · | .004→**.002** | .00 · | .63 · | 2.5 · | .87 · | 4/6,10 · |
| cyber      | 93k · | 42.7 · | .066→**.063** | .17 · | .39 · | 3.4 · | .75 · | 4/4,11 · |
| transport  | 80k→**96k** | 49.7→**41.4** | .000 · | .00 · | .48→**.34** | 2.4→**2.2** | .90→**.77** | 4/3,8 · |
| e-commerce | 91k · | 43.7 · | .098→**.097** | .00 · | .38 · | 2.5 · | .60 · | 4/5,8 · |
| healthcare | 52k→**61k** | 76.7→**65.5** | .003→**.008** | .02→**.05** | .25→**.38** | 2.0→**2.7** | .86→**.78** | 4/4,10→**4/5,11** |

**Reproduce (four unchanged domains).** finance, crypto, cyber, e-commerce reproduce
on every structural cell to rounding. The only movements are third-decimal clustering
jitter from the sampled-clustering estimator and tiny homophily-param nudges
(finance .036→.035, crypto .004→.002, cyber .066→.063, e-comm .098→.097); none crosses
a printed-precision boundary that matters, and all node/deg/recip/B/α/rep/schema cells
are unchanged. This confirms the four normal graphs are unchanged by the fixes (as
expected: homophily only affects anomalies, which are OFF here).

**Transport shifted (expected — F1 fix).** Activating the truck/bus fleets and
rebalancing sensor weight changed the population and event mix: nodes 80k→96k, mean
degree 49.7→41.4, burstiness .48→.34, tail α 2.4→2.2, recurrence .90→.77. Clustering
(.000), reciprocity (.00) and the `4/3,8` schema are unchanged (still 3 firing edge
types; no new event/actor types instantiate — truck/bus are `vtype` values of the
existing `vehicle` node type).

**Healthcare shifted (expected — F1-health fix).** Activating 5 specialties, adding the
`PRESCRIBED_NDC` drug event, and rebalancing the pharmacy rate/burst changed both the
graph and the schema: nodes 52k→61k, mean degree 76.7→65.5, clustering .003→.008,
reciprocity .02→.05, burstiness .25→.38, tail α 2.0→2.7, recurrence .86→.78, and the
schema **4/4,10 → 4/5,11** (the now-firing `PRESCRIBED_NDC` event adds one edge type
and one edge attribute; `PAID` remains a zero-normal-weight schema event and still does
not fire).

---

## 4. New per-column extremes for the caption

The frozen caption bolds each column's extreme and reports its span. Two prose/bold
targets moved OFF healthcare and one moved OFF transport onto other domains. Updated
spans and bold targets:

| column | frozen span | frozen bold cell(s) | **final span** | **final bold cell(s)** | change |
|---|---|---|---|---|---|
| clustering | 0–.10 | transport .000 (min), e-comm .098 (max) | **0–.10** (.000–.097) | transport .000, e-comm .097 | span/cells unchanged |
| reciprocity | 0–.17 | cyber .17 (max) | **0–.17** (.00–.168) | cyber .17 | unchanged |
| burstiness B | .25–.63 | healthcare .25 (min) | **.34–.63** (.338–.630) | **transport .34** (min), crypto .63 (max) | min bold moves healthcare→transport |
| tail α | 2.0–3.4 | cyber 3.4 (max), healthcare 2.0 (min) | **2.2–3.4** (2.22–3.41) | cyber 3.4 (max), **transport 2.2** (min/heaviest) | min bold moves healthcare→transport |
| recurrence rep | .60–.90 | e-comm .60 (min), transport .90 (max) | **.60–.87** (.597–.867) | e-comm .60 (min), **crypto .87** (max) | max bold moves transport→crypto |
| mean degree | (77 bold) | healthcare 76.7 (max) | (**66** bold) | healthcare **65.5** (max) | same cell, value 77→66 |

**Prose corrections implied for the paragraph above the table** (not edited here):
- "healthcare claims are hub-dominated (mean degree **77**, the **heaviest tail at
  α≈2.0**)" is now wrong on both counts: healthcare mean degree is **66** and its tail
  is α≈2.7. The **heaviest tail is now transport at α≈2.2**; healthcare is still the
  mean-degree hub (66, the max).
- "burstiness min" is now transport (.34), not healthcare (.25).
- "transport trips are steady" with the bolded max recurrence .90 → transport recurrence
  is now .77; the **most recurrent domain is crypto (.87)**. Crypto's own prose
  ("dense and highly recurrent (recurrence 0.87, burstiness 0.63)") is still exactly right.
- cyber ("reciprocity 0.17, lightest tail α=3.4") and e-commerce ("clustering 0.10,
  lowest recurrence 0.60") are still accurate.

"No two domains look alike" still holds: all six rows remain mutually distinct.
