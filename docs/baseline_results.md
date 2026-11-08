# Temporal-generator baselines: measured results

Companion to `docs/baseline_survey.md`, which records provenance and every
source patch. Everything here is a measured run on one machine on 2026-08-24.

Target: `High_School11` (the dataset bundled with ETN-gen), 126 nodes,
28,560 contact records, 272,330 s span, 5,609 distinct timestamps.
Machine: NVIDIA RTX A5000 (24 GB), Python 3.10.12. TagGen and TIGGER used the
GPU; DYMOND, ETN-gen, and SAGA are CPU only. Wall time and peak resident set
size are from `/usr/bin/time -v` around the whole process.

## 1. Cost

| generator | fit | generate | total wall | peak RSS | device |
|---|---:|---:|---:|---:|---|
| SAGA | none | 0.04 s | **0.04 s** | **0.016 GB** | CPU |
| ETN-gen | 4.9 s | 2.7 s | 7.6 s | 0.18 GB | CPU |
| DYMOND | 19.9 s | 161.9 s | 181.8 s | 0.30 GB | CPU |
| TagGen | 74.6 s | 501.2 s | 575.8 s | 1.40 GB | GPU |
| TIGGER | 1390.2 s | 9.9 s | 1400.1 s | 3.74 GB | GPU |

SAGA is 190x faster than the fastest fitted generator here and about 35,000x
faster than the slowest, at 0.016 GB against their 0.18 to 3.74 GB. This
is a fair comparison of end-to-end cost to produce one graph, and an unfair
comparison of method: SAGA does not fit a model, so it does no work the others
must do. The comparison that matters for a benchmark generator is that the
fitted generators cannot be run at all at the scales in Section 6.5, and SAGA
can.

## 2. Fidelity

Every generator's timestamps are rescaled linearly to the real span before
scoring, because the generators emit different time units (seconds, snapshot
indices, window indices). Distances are per-metric distance to real, lower is
closer; the composite is Eq. (4).

| | edges | distinct t | composite fidelity |
|---|---:|---:|---:|
| real | 28,560 | 5,609 | - |
| SAGA | 31,450 | 28,982 | **0.803** |
| DYMOND | 26,163 | 30 | 0.788 |
| TagGen | 51,709 | 12 | 0.684 |
| TIGGER | 6,582 | 14 | 0.546 |
| ETN-gen | 1,713 | 445 | 0.481 |

| metric | ETN-gen | DYMOND | TagGen | TIGGER | SAGA |
|---|---:|---:|---:|---:|---:|
| KS out-degree | 0.698 | 0.198 | 0.397 | 0.484 | **0.159** |
| KS in-degree | 0.794 | **0.151** | 0.365 | 0.548 | 0.175 |
| KS total degree | 0.937 | 0.183 | 0.325 | 0.683 | **0.143** |
| KS inter-event time | 0.699 | 0.774 | 0.880 | 0.772 | **0.558** |
| power-law alpha | **0.230** | 0.373 | 0.238 | 0.332 | 0.235 |
| burstiness B | 0.258 | 0.140 | **0.014** | 0.118 | 0.118 |
| memory M | **0.001** | 0.033 | 0.017 | 0.024 | 0.117 |
| clustering | 0.376 | 0.268 | 0.029 | **0.017** | 0.194 |
| reciprocity | **0.000** | **0.000** | 0.909 | 1.000 | 0.418 |
| repeat-edge ratio | 0.826 | 0.173 | 0.015 | 0.388 | **0.004** |
| mean degree | 0.887 | **0.044** | 0.288 | 0.625 | 0.048 |
| **composite** | 0.481 | 0.788 | 0.684 | 0.546 | **0.803** |

Absolute values, real first:

| metric | real | ETN-gen | DYMOND | TagGen | TIGGER | SAGA |
|---|---:|---:|---:|---:|---:|---:|
| power-law alpha | 3.762 | 6.016 | 8.247 | 6.112 | 7.499 | 2.331 |
| burstiness B | 0.788 | 0.273 | 0.508 | 0.816 | 0.552 | 0.552 |
| memory M | 0.056 | 0.054 | 0.122 | 0.021 | 0.008 | 0.290 |
| clustering | 0.576 | 0.200 | 0.844 | 0.547 | 0.559 | 0.381 |
| reciprocity | 0.000 | 0.000 | 0.000 | 0.909 | 1.000 | 0.418 |
| repeat-edge ratio | 0.940 | 0.114 | 0.768 | 0.925 | 0.552 | 0.945 |
| mean degree | 453.3 | 27.2 | 415.3 | 820.8 | 104.5 | 499.2 |

## 3. What this result does and does not show

**The four learned generators are in Task B and SAGA is in Task A.** DYMOND,
ETN-gen, TagGen, and TIGGER each read the target graph and fit to it. SAGA is
given three scalars, node count, edge count, and span, and never sees the
target. SAGA scoring highest on the composite is therefore a result about what
this target rewards, not a demonstration that SAGA models contact networks
better than a model trained on them.

**Three of the four are discrete-snapshot models, and the inter-event metric
penalizes that directly.** DYMOND produced 30 distinct timestamps, TIGGER 14,
TagGen 12, against 5,609 in the real data. Their inter-event KS distances
(0.774, 0.772, 0.880) largely measure snapshot granularity rather than fit.
This is a property of the model class: these methods target discrete-time
dynamic graphs. Reading their composite scores as an overall verdict on the
methods would be a misreading.

**Reciprocity here measures a data-model mismatch, not fit quality.**
High_School11 is an undirected contact log stored with each contact recorded
once, so its reciprocity is 0 by construction. TagGen and TIGGER emit symmetric
adjacency and score near 1; SAGA emits directed financial transfers and scores
0.418. None of these three numbers says anything about generative quality on
this target.

**ETN-gen's score is dominated by an edge-count shortfall.** With the gap
parameter documented in ETN-gen's own notebook (`gap=299`) it produced 1,713
edges against the target's 28,560, and its mean degree is 27.2 against 453.3.
A smaller gap (19) produced 3,317 edges in 111.4 s at 0.81 GB, still an order
of magnitude short. We report the documented setting as measured and did not
search its parameter space; a tuned run would likely score higher, and its
score should be read as "with the published defaults" rather than as the
method's ceiling.

**No hyperparameter search was run for any baseline.** Each was run with the
example invocation from its own README or notebook. Where a documented default
could not run at all we changed the smallest thing that made it run and said so
(TIGGER's `time_window=1` crashes with an `IndexError` on empty windows for
this target because it has 13,617 time steps over 126 nodes; we used
`time_window=500`, which is within the documented range).

**This is one target, one seed, one machine.** The numbers support the
statement that these generators run at this scale and cost what the table says.
They do not support a general ranking.

## 4. Static generators: the timestamp-protocol audit

Target: CollegeMsg (1,899 nodes, 59,835 edges, 193.7 d) against a
size-matched SAGA graph. `uniform` draws timestamps uniformly over the real
span; `renewal` bootstraps inter-event gaps from the real gap distribution and
rescales to the span, which hands the static baseline the real inter-event
marginal outright. Composite fidelity:

| generator | uniform | renewal | change |
|---|---:|---:|---:|
| SAGA | 0.795 | 0.795 | - |
| config model | 0.816 | **0.836** | +0.020 |
| R-MAT | 0.705 | 0.707 | +0.002 |
| Barabasi-Albert | 0.642 | 0.643 | +0.001 |
| Erdos-Renyi | 0.558 | 0.578 | +0.020 |
| Watts-Strogatz | 0.557 | 0.564 | +0.007 |

The protocol choice is not neutral. Under the naive uniform protocol the
configuration model scores below SAGA on the composite; under the
matched-marginal protocol it scores above it, 0.836 against 0.795. The
configuration model is a Task B generator handed the real degree sequence
exactly, and under `renewal` it is additionally handed the real inter-event
distribution, so it holds the two ingredients the composite weights most. We
report the protocol explicitly and both numbers, rather than the protocol under
which SAGA leads.

What does not change with the protocol: the configuration model produces no
attributes, no labels, and no controllable difficulty, and a classifier
two-sample test separates it from real at 0.915 (uniform) and 0.928 (renewal),
against SAGA's 0.999. On the temporal-motif L1 distance the two are within
0.005 of each other (1.416 against 1.411). SAGA does not lead this table on
marginal similarity; it leads on what a benchmark generator has to supply
beyond marginal similarity.

## 5. Reproduction

Drivers are in `docs/repro/baselines/`. Patched upstream sources are not
redistributed; `docs/baseline_survey.md` lists every change precisely enough to
reapply. The static-generator protocol is
`python/saga_eval/baselines.py::set_time_protocol`.
