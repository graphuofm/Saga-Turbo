# tab:diff re-measurement: Average Precision alongside ROC-AUC (fixed engine)

Camera-ready re-measurement track. Every cell below is from a real seed-42 run on the
fixed engine (`target/release/agora`, built 2026-07-17 18:19). No number is estimated,
interpolated, or copied from the frozen paper table. The sweep was run twice and
reproduced bit-for-bit (deterministic: seed 42 with a fixed stratified train/test split).

This file is a measurement note only. It does not modify `paper/main.tex` or any frozen
number, and nothing here is committed.

## Protocol (reproduces the paper's tab:diff, sweep-re-measure variant)

- Domain: finance.
- Graph size: 500k edges, anomaly-rate pi = 0.05, seed 42 (the size the difficulty
  sweeps re-measure used; the frozen tab:diff is also finance).
- One fresh graph generated per difficulty level; graph deleted immediately after
  measurement (testing-only machine).
- Detector: `HistGradientBoostingClassifier` (GBM), `max_iter=200`, trained on the
  emitted per-edge labels (`python/saga_eval/detector.py::detect_auc`). Features combine
  structure (endpoint log-degrees), attributes (amount, channel, type, cross-border,
  etc.), and timing (hour-of-day, inter-event gap, recurrence). No node IDs, no
  label-derived features.
- Three feature views: attribute-only (`feature_set='attr'`, structural degree feats
  dropped), structure-only (`feature_set='struct'`, degree feats only), full
  (`feature_set='all'`).
- Split: 70/30 stratified train/test, `random_state=42`. Metrics are on the held-out
  test set: test ROC-AUC and test Average Precision (`sklearn.metrics.roc_auc_score`,
  `average_precision_score`).
- Difficulty knob delta in {0, 0.25, 0.5, 0.75, 1.0} (the five columns of tab:diff).

Generation command per delta:

```
agora generate --domain finance --edges 500000 --seed 42 \
  --anomaly-rate 0.05 --difficulty <delta> --format csv --out <dir>
```

Driver script: `scratchpad/diff.sh`. Raw output: `scratchpad/diff_results.csv`.

### Base rate (the point of the exercise)

The emitted per-edge positive rate is roughly 0.4%, not 5%. pi = 0.05 sets the
campaign/entity rate; each campaign lays down only a handful of edges, so the fraction of
anomalous EDGES is far smaller:

| delta | anomalous edges | total edges | base rate |
|------:|----------------:|------------:|----------:|
| 0     | 1787 | 498797 | 0.358% |
| 0.25  | 1915 | 498826 | 0.384% |
| 0.5   | 2038 | 498825 | 0.409% |
| 0.75  | 2278 | 498818 | 0.457% |
| 1.0   | 2633 | 498789 | 0.528% |

At a ~0.4% base rate, ROC-AUC is dominated by the vast negative pool and stays optimistic
even when the detector cannot rank the rare positives; Average Precision is the metric
that reflects performance at the honest prevalence.

## Table 1 — Average Precision (AP), test set

| detector view       | delta=0 | 0.25 | 0.5 | 0.75 | 1.0 |
|---------------------|--------:|-----:|----:|-----:|----:|
| attribute-only      | 0.402 | 0.350 | 0.270 | 0.164 | 0.091 |
| structure-only      | 0.056 | 0.055 | 0.054 | 0.055 | 0.067 |
| full (all features) | 0.532 | 0.509 | 0.388 | 0.308 | 0.381 |

## Table 2 — ROC-AUC, test set

| detector view       | delta=0 | 0.25 | 0.5 | 0.75 | 1.0 |
|---------------------|--------:|-----:|----:|-----:|----:|
| attribute-only      | 0.843 | 0.843 | 0.878 | 0.920 | 0.923 |
| structure-only      | 0.941 | 0.941 | 0.938 | 0.938 | 0.948 |
| full (all features) | 0.868 | 0.914 | 0.916 | 0.905 | 0.914 |

(Four-decimal raw values are in `scratchpad/diff_results.csv`.)

## What the two tables show

- Attribute-only AP declines monotonically across the sweep: 0.402 -> 0.350 -> 0.270 ->
  0.164 -> 0.091, a 4.4x fall. This is the knob doing exactly what it claims: as delta
  rises the anomalies' amounts and timing regress toward ordinary transactions, so an
  attribute-only view loses them.
- The SAME attribute-only signal in ROC-AUC does the opposite: it RISES, 0.843 -> 0.923.
  Under a 0.4% base rate the AUC is measuring the easy bulk of negatives, not the shrinking
  positive-ranking quality, so it moves the wrong way. This is the clearest evidence that
  the frozen AUC table misrepresents the knob's effect.
- Structure-only AUC stays flat near 0.94 across all five levels (0.941, 0.941, 0.938,
  0.938, 0.948), which matches the frozen table's structure-only row (0.941 - 0.942) and
  confirms the claim that the knob does not camouflage structure. In AP terms structure
  alone is near the floor (~0.05-0.07) at this base rate: high AUC, negligible precision,
  the textbook rare-positive divergence.
- Full-view AP declines from 0.532 to 0.308 by delta=0.75, then ticks back up to 0.381 at
  delta=1.0. The non-monotone tail comes from structure re-asserting itself (structure-only
  AP and AUC both rise slightly at delta=1.0 as anomalous edges grow to 0.53% of the graph),
  not from attributes. Full-view AUC is essentially flat-to-rising (0.868 - 0.916), again
  hiding the attribute effect.
- Note on the frozen table: the frozen tab:diff attribute-only and full rows (0.995 ->
  0.968, 0.998 -> 0.991) are from the pre-fix engine and do not reproduce on the fixed
  engine; only the structure-only row (~0.94) carries over. This is expected, the fixed
  engine has the honest ~0.4% edge base rate.

Load note: 1-min load average was ~28 (a concurrent heavy job) during measurement. AP and
AUC are deterministic functions of seed-42 output and the fixed split, so the values are
load-independent; only wall-clock timing (not reported here) would be affected.

## Recommendation

Yes: the AP table is the honest camera-ready replacement for the AUC table. At the fixed
engine's ~0.4% edge base rate, ROC-AUC is the wrong instrument, it is optimistic and even
INVERTS the knob's effect (attribute-only AUC rises 0.843 -> 0.923 while the anomalies are
provably getting harder). Average Precision tells the true story the paper wants to tell:
attribute-only AP declines monotonically 0.402 -> 0.091 (a 4.4x drop), demonstrating that
the difficulty knob genuinely camouflages attribute separability, while structure-only
stays flat (~0.94 AUC, and near-floor ~0.05 AP) because the knob does not touch the
structural footprint. Both of the questions posed are confirmed: structure-only stays
flat at ~0.94 AUC across the sweep, and attribute-only AP declines monotonically. Camera-
ready tab:diff should lead with AP (attribute-only and full rows), optionally keeping the
structure-only AUC row to make the "high AUC can hide a real effect" point explicit.
