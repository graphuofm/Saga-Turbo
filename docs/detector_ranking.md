# Detector-ranking agreement between real labeled data and SAGA

Purpose: the advisor review (its sections 12 and 19-Experiment-4) argues that the
convincing benchmark-validity result is not a high detector score on synthetic
data but whether detectors RANK the same way on synthetic data as on real
labeled data. This file reports that measurement. Every number is from a real
run on this host, CPU only; the driver is `docs/repro/detector_ranking/rank.py`
and the raw output is `results.json` in the same directory.

## Protocol

Task: node-level anomaly detection on a temporal graph. Two datasets, matched in
edge count and time-step count:

* Real: the Elliptic Bitcoin transaction graph (`realdata/elliptic-bitcoin`).
  203,769 nodes, 234,356 edge rows, 49 time steps taken from the feature file's
  time-step column. Labels are node classes: class 1 (illicit) is positive,
  class 2 (licit) is negative, and the 157,205 `unknown` nodes are kept in the
  graph for structure but excluded from scoring, which is the usual practice.
  46,564 nodes are scored, of which 4,545 are positive (9.76%).
* Synthetic: SAGA finance, seed 42, `--anomaly-rate 0.02`, generated at the same
  requested node count, edge count and span (49 days). Positives are the nodes
  SAGA records in `labels_nodes.csv`, that is the agents recruited into a
  campaign; every other node is a negative, so SAGA is fully labeled.

Six detector families, deliberately different in kind, each producing one score
per node from the timestamped edge list alone (features are computed by the
driver: in/out degree, distinct partners, repeat ratio, first and last activity,
active time steps, inter-event mean and standard deviation, burstiness, peak
events in one step, and mean neighbour degree):

1. gradient-boosted trees on those features (supervised);
2. truncated-SVD spectral embedding of the symmetrized adjacency plus logistic
   regression (supervised);
3. IsolationForest on the features (unsupervised);
4. an AnomRank-style centrality-shift score, the absolute change in PageRank
   between the first and second half of the time span (unsupervised);
5. a MIDAS-style streaming count-min sketch over (source, destination) pairs per
   time bucket, scored by squared surprise against the running mean and taken as
   the maximum over a node's edges (unsupervised);
6. total-degree z-score, a heuristic floor.

Items 4 and 5 are faithful reimplementations in the spirit of the published
methods rather than the authors' code, and are labeled as such.

Evaluation: three seeds, each a 70/30 stratified split of the scored nodes;
supervised detectors train on the 70% and are scored on the 30%, unsupervised
detectors are scored on the same 30% so every number is comparable. We report
mean average precision, then the Spearman and Kendall correlation between the
detector ranking on Elliptic and on SAGA.

## Average precision per detector

| detector | Elliptic AP | SAGA AP | SAGA d=0 | d=0.5 | d=1.0 |
|---|---:|---:|---:|---:|---:|
| GBM (supervised, features) | 0.6272 | 0.0431 | 0.0445 | 0.0431 | 0.0423 |
| Spectral+LR (supervised) | 0.1104 | 0.0286 | 0.0284 | 0.0286 | 0.0291 |
| IsolationForest (unsupervised) | 0.0658 | 0.0176 | 0.0190 | 0.0176 | 0.0171 |
| AnomRank-style (centrality shift) | 0.0764 | 0.0230 | 0.0229 | 0.0230 | 0.0235 |
| MIDAS-style (streaming sketch) | 0.0740 | 0.0181 | 0.0190 | 0.0181 | 0.0178 |
| Degree z-score (heuristic floor) | 0.0845 | 0.0185 | 0.0192 | 0.0185 | 0.0181 |

## Rank correlation, real versus SAGA

| SAGA configuration | Spearman rho | p | Kendall tau | p |
|---|---:|---:|---:|---:|
| saga | 0.943 | 0.0048 | 0.867 | 0.0167 |
| saga_d0.0 | 0.886 | 0.0188 | 0.733 | 0.0556 |
| saga_d0.5 | 0.943 | 0.0048 | 0.867 | 0.0167 |
| saga_d1.0 | 0.943 | 0.0048 | 0.867 | 0.0167 |

The ranking agrees. On the default configuration Spearman rho is 0.943
(p = 0.0048) and Kendall tau is 0.867 (p = 0.0167) over the six families, and the
agreement is stable across the difficulty knob (rho 0.886 at d=0, 0.943 at
d=0.5 and d=1.0). The two orderings differ by exactly one adjacent swap: the
degree z-score floor and the centrality-shift detector trade places.

Ranking on Elliptic: GBM (supervised, features) > Spectral+LR (supervised) > Degree z-score (heuristic floor) > AnomRank-style (centrality shift) > MIDAS-style (streaming sketch) > IsolationForest (unsupervised)

Ranking on SAGA: GBM (supervised, features) > Spectral+LR (supervised) > AnomRank-style (centrality shift) > Degree z-score (heuristic floor) > MIDAS-style (streaming sketch) > IsolationForest (unsupervised)

## Base rates and lift, which is where the two datasets differ

Absolute average precision is NOT comparable between the two datasets: the
positive base rate is 0.0976 on the scored Elliptic nodes and 0.0221 on SAGA,
and Elliptic is only 23% labeled while SAGA is fully labeled. Dividing average
precision by the base rate gives lift over a random ranker:

| detector | Elliptic lift | SAGA lift |
|---|---:|---:|
| GBM (supervised, features) | 6.43x | 1.95x |
| Spectral+LR (supervised) | 1.13x | 1.29x |
| IsolationForest (unsupervised) | 0.67x | 0.80x |
| AnomRank-style (centrality shift) | 0.78x | 1.04x |
| MIDAS-style (streaming sketch) | 0.76x | 0.82x |
| Degree z-score (heuristic floor) | 0.87x | 0.84x |

The rank correlation is unchanged under this normalization (Spearman
0.943, p = 0.0048).

## What this establishes, and what it does not

It establishes that a benchmark built from SAGA would order these six detector
families essentially the same way the real labeled graph does, which is the
property a benchmark needs if it is to be used to compare detectors.

It does not establish that SAGA is as hard as, or as realistic as, Elliptic.
Two honest observations point the other way. First, the supervised
gradient-boosted detector is much stronger on Elliptic (lift 6.43x) than on SAGA
(1.95x), so SAGA's campaign membership is harder to read off these node features
than Elliptic's illicit class; the common worry that synthetic anomalies are
easier than real ones does not hold on this task. Second, SAGA compresses the
spread between detectors: the weaker families do slightly better on SAGA
(centrality shift 1.04x versus 0.78x, spectral 1.29x versus 1.13x) while the
strongest does much worse, so the dynamic range that separates a good detector
from a poor one is narrower on SAGA than on Elliptic. A benchmark with a
narrower range still orders methods correctly here, but it discriminates less.

Label semantics differ and are bridged, not identical: Elliptic labels a
transaction node as illicit, while SAGA labels an agent as recruited into a
campaign. Both reduce to a binary node label on a temporal graph, which is what
the detectors consume, but they are not the same notion of anomaly.

Realized sizes: SAGA emitted about 102,100 nodes that appear in at least one
edge against Elliptic's 203,769, because the requested node count is an upper
bound on the nodes that actually transact within the span. Edge counts match to
within 1% (233,000 against 234,356).

