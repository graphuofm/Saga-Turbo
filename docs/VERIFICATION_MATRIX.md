# Verification matrix: randomized determinism + performance suite (streaming block-bounded-staleness engine)

Measurement-only run against the prebuilt binary `./target/release/agora`
(saga 0.1.0, binary mtime 2026-08-15 00:48; NOT rebuilt for this run).
Host: 13th Gen Intel Core i9-13900K (24 physical / 32 logical cores), 62 GiB RAM,
Ubuntu 22.04, kernel 6.5.0-1027-oem. Date: 2026-08-15.

Every number below is from a real run on this host. Every generated graph
directory was deleted immediately after its numbers were extracted (verified
empty at the end). 1-min loadavg was checked before every timing run
(threshold 8, never hit; observed 4.6 to 6.6). Correctness (Track 1) runs are
load-independent and ran at loadavg 3.3 to 6.

## Track 1: randomized determinism matrix

Protocol. 45 randomized combos over: all 6 domains; seeds {42, 7, 20260815}
plus two RNG-drawn seeds {124515117, 376346595} (matrix RNG seeded 20260815 for
reproducibility of the sample itself); sizes {200k, 400k, 600k, 1M} edges;
L = --block-len-days in {0.01, 0.25, 1, 4}; thread pairs drawn from {1, 2, 8, 32}.
Finance got 10 combos, 5 with --guards ON (guards are only meaningful for
finance); the other five domains got 7 combos each, guards off. 12 guards-off
combos additionally ran sharded with --shard-count in {2, 4}.

Checks per combo (all on logical values, never raw parquet bytes):

- T-ident: threads T1 vs T2 produce identical edge multisets. Comparison is a
  sha256 over the sorted per-row hashes of the tuple (src, dst, t, event_type,
  label, amount-if-present) across all edge parquet shards, plus a row-count
  equality check.
- rerun-ident: the exact same command run twice produces the identical multiset.
- shard-union: the union of the --shard-count disjoint shard outputs equals the
  whole-graph edge multiset (same digest, same count).
- overdraft ctrs: for finance guards-on, the (overdraft_guarded,
  overdraft_violations) counters from the run JSON are identical across the two
  thread counts.

Result: 45/45 combos PASS. 45/45 thread-identity, 45/45 rerun-identity,
12/12 shard-union, 5/5 overdraft-counter checks. Zero failures.

| # | domain | seed | edges req | edges written | L (days) | guards | threads A vs B | rerun | shards | T-ident | rerun-ident | shard-union | overdraft ctrs | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 7 | crypto | 42 | 200,000 | 195,524 | 4 | off | 8 vs 1 | x2 | 4 | ok | ok | ok | - | PASS |
| 17 | crypto | 42 | 400,000 | 390,496 | 4 | off | 2 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 3 | crypto | 42 | 1,000,000 | 981,636 | 4 | off | 8 vs 1 | x2 | 4 | ok | ok | ok | - | PASS |
| 34 | crypto | 42 | 1,000,000 | 981,636 | 4 | off | 8 vs 1 | x2 | - | ok | ok | - | - | PASS |
| 44 | crypto | 124515117 | 400,000 | 391,471 | 4 | off | 32 vs 2 | x2 | - | ok | ok | - | - | PASS |
| 26 | crypto | 124515117 | 1,000,000 | 979,733 | 1 | off | 8 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 38 | crypto | 376346595 | 1,000,000 | 985,040 | 0.25 | off | 1 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 9 | cyber | 42 | 400,000 | 397,696 | 0.01 | off | 8 vs 32 | x2 | 4 | ok | ok | ok | - | PASS |
| 23 | cyber | 42 | 600,000 | 595,740 | 1 | off | 8 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 31 | cyber | 42 | 1,000,000 | 993,400 | 0.01 | off | 1 vs 2 | x2 | - | ok | ok | - | - | PASS |
| 36 | cyber | 20260815 | 200,000 | 198,607 | 4 | off | 1 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 28 | cyber | 376346595 | 400,000 | 399,245 | 4 | off | 8 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 20 | cyber | 376346595 | 1,000,000 | 996,506 | 0.25 | off | 8 vs 2 | x2 | - | ok | ok | - | - | PASS |
| 39 | cyber | 376346595 | 1,000,000 | 996,506 | 0.25 | off | 32 vs 1 | x2 | - | ok | ok | - | - | PASS |
| 30 | ecommerce | 42 | 400,000 | 395,789 | 0.25 | off | 1 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 27 | ecommerce | 42 | 1,000,000 | 997,103 | 1 | off | 1 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 14 | ecommerce | 20260815 | 400,000 | 397,395 | 1 | off | 32 vs 2 | x2 | - | ok | ok | - | - | PASS |
| 6 | ecommerce | 20260815 | 600,000 | 594,433 | 4 | off | 1 vs 8 | x2 | 4 | ok | ok | ok | - | PASS |
| 35 | ecommerce | 124515117 | 200,000 | 198,667 | 1 | off | 2 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 5 | ecommerce | 376346595 | 200,000 | 199,366 | 1 | off | 1 vs 8 | x2 | 2 | ok | ok | ok | - | PASS |
| 10 | ecommerce | 376346595 | 1,000,000 | 996,744 | 1 | off | 32 vs 8 | x2 | 2 | ok | ok | ok | - | PASS |
| 11 | finance | 7 | 1,000,000 | 998,644 | 0.01 | off | 32 vs 8 | x2 | 2 | ok | ok | ok | - | PASS |
| 2 | finance | 42 | 600,000 | 598,506 | 1 | off | 8 vs 2 | x2 | 4 | ok | ok | ok | - | PASS |
| 12 | finance | 42 | 1,000,000 | 887,156 | 1 | ON | 2 vs 1 | x2 | - | ok | ok | - | ok (g=421982,v=20437) | PASS |
| 45 | finance | 20260815 | 200,000 | 200,207 | 0.25 | off | 2 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 37 | finance | 20260815 | 200,000 | 200,207 | 4 | off | 1 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 41 | finance | 124515117 | 200,000 | 184,046 | 0.01 | ON | 32 vs 8 | x2 | - | ok | ok | - | ok (g=91487,v=1062) | PASS |
| 33 | finance | 124515117 | 200,000 | 184,083 | 4 | ON | 1 vs 32 | x2 | - | ok | ok | - | ok (g=91524,v=3810) | PASS |
| 43 | finance | 124515117 | 1,000,000 | 888,451 | 1 | ON | 1 vs 8 | x2 | - | ok | ok | - | ok (g=426967,v=20596) | PASS |
| 22 | finance | 376346595 | 200,000 | 183,230 | 0.25 | ON | 2 vs 8 | x2 | - | ok | ok | - | ok (g=90922,v=3140) | PASS |
| 16 | finance | 376346595 | 600,000 | 597,902 | 4 | off | 1 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 42 | healthcare | 20260815 | 200,000 | 198,188 | 1 | off | 2 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 21 | healthcare | 20260815 | 600,000 | 595,547 | 0.25 | off | 1 vs 2 | x2 | - | ok | ok | - | - | PASS |
| 25 | healthcare | 20260815 | 1,000,000 | 992,012 | 0.25 | off | 8 vs 32 | x2 | - | ok | ok | - | - | PASS |
| 32 | healthcare | 124515117 | 400,000 | 396,709 | 0.01 | off | 32 vs 1 | x2 | - | ok | ok | - | - | PASS |
| 15 | healthcare | 124515117 | 600,000 | 594,306 | 0.01 | off | 1 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 24 | healthcare | 124515117 | 600,000 | 594,306 | 0.25 | off | 1 vs 8 | x2 | - | ok | ok | - | - | PASS |
| 40 | healthcare | 376346595 | 200,000 | 197,317 | 0.01 | off | 8 vs 1 | x2 | - | ok | ok | - | - | PASS |
| 13 | transport | 42 | 400,000 | 399,614 | 1 | off | 32 vs 2 | x2 | 4 | ok | ok | ok | - | PASS |
| 4 | transport | 20260815 | 600,000 | 599,066 | 0.01 | off | 8 vs 1 | x2 | 4 | ok | ok | ok | - | PASS |
| 18 | transport | 20260815 | 600,000 | 599,066 | 0.01 | off | 32 vs 1 | x2 | - | ok | ok | - | - | PASS |
| 8 | transport | 20260815 | 1,000,000 | 998,007 | 0.01 | off | 32 vs 2 | x2 | 2 | ok | ok | ok | - | PASS |
| 19 | transport | 124515117 | 200,000 | 199,614 | 0.25 | off | 32 vs 2 | x2 | - | ok | ok | - | - | PASS |
| 1 | transport | 124515117 | 1,000,000 | 998,695 | 0.25 | off | 1 vs 2 | x2 | 2 | ok | ok | ok | - | PASS |
| 29 | transport | 376346595 | 600,000 | 598,150 | 0.25 | off | 1 vs 2 | x2 | - | ok | ok | - | - | PASS |

## Track 2: performance suite (finance, seed 42, parquet, anomalies on, default nodes = 100,000)

Method for every timing point: wait for 1-min loadavg < 8, run the point twice,
keep the faster run (engine-reported wall_time_s, which covers the whole run
including the sink). Peak RSS from /usr/bin/time -v (Maximum resident set size)
of the kept run. Reps agreed closely everywhere (worst-case rep spread ~3%).
Threads are the engine default (auto on 32 logical cores) unless stated.
events/s = edges_written / wall as the engine reports it.

### 2.1 Throughput and RAM vs scale, guards OFF

| edges requested | edges written | wall (s) | events/s | peak RSS | loadavg |
|---|---|---|---|---|---|
| 1e6 | 998,371 | 0.37 | 2.68 M | 105 MiB | 5.03 |
| 1e7 | 9,983,083 | 1.96 | 5.10 M | 275 MiB | 5.03 |
| 1e8 | 99,809,412 | 17.08 | 5.84 M | 1.32 GiB | 4.94 |
| 1e9 | (prior measurement, not rerun) | 182.1 | ~5.5 M | 7.92 GB | - |

The 1e9 row is the previously recorded measurement of this same binary and was
explicitly not rerun. Scaling from 1e7 to 1e9 is near-linear in wall
(1.96 -> 17.08 -> 182.1 s per decade) at ~5 to 5.8 M events/s; 1e6 is
startup-dominated. RSS grows sublinearly-to-linearly (275 MiB -> 1.32 GiB ->
7.92 GB).

### 2.2 Cost of consistency vs scale, guards ON, L = 1 day

| edges requested | edges written | wall (s) | events/s | peak RSS | guarded transfers | violations | violation rate | wall vs guards-off |
|---|---|---|---|---|---|---|---|---|
| 1e6 | 887,156 | 0.41 | 2.18 M | 108 MiB | 421,982 | 20,437 | 4.84% | +11% |
| 1e7 | 8,293,981 | 2.18 | 3.80 M | 257 MiB | 3,647,983 | 455,343 | 12.48% | +11% |
| 1e8 | 75,208,475 | 16.19 | 4.65 M | 1.22 GiB | 28,787,432 | 9,021,302 | 31.34% | -5% |

Violation rate = overdraft_violations / overdraft_guarded (an emitted transfer
that passed the stale block-start guard but overdrew the true balance).

Two honest observations:

- At 1e8 the guards-ON wall is LOWER than guards-off (16.19 vs 17.08 s) because
  the guard suppresses 24.6% of candidate edges, so less is written. Per emitted
  edge, guards cost more: 0.215 us/edge (on) vs 0.171 us/edge (off) at 1e8,
  about +26% per edge. At 1e6/1e7 the suppressed fraction is smaller and the
  net wall cost of guards is ~11%.
- The violation rate at fixed L = 1 grows with scale here (4.8% -> 12.5% ->
  31.3%) because the default node count is fixed at 100,000 while the 90-day
  span is also fixed, so per-account event density grows 100x from 1e6 to 1e8.
  This is a density effect, not an engine change; the 1e7 point (12.48%)
  reproduces the recorded 10M sweep value (12.50%, measured with anomalies off)
  almost exactly.

### 2.3 Thread scaling at 1e7 (explicit --threads)

Guards OFF:

| threads | wall (s) | events/s |
|---|---|---|
| 1 | 2.63 | 3.80 M |
| 2 | 2.15 | 4.63 M |
| 4 | 1.95 | 5.12 M |
| 8 | 1.96 | 5.10 M |
| 16 | 1.94 | 5.14 M |
| 32 | 1.98 | 5.04 M |

Guards ON, L = 1:

| threads | wall (s) | events/s |
|---|---|---|
| 1 | 2.97 | 2.79 M |
| 2 | 2.53 | 3.28 M |
| 4 | 2.25 | 3.69 M |
| 8 | 2.21 | 3.76 M |
| 16 | 2.17 | 3.82 M |
| 32 | 2.23 | 3.73 M |

Plateau points, measured honestly: guards OFF plateaus at 4 threads (8/16/32
are within noise of 4; total speedup over 1 thread is only 1.36x). Guards ON
is effectively flat from 4 threads on (best point 16 threads is 3.7% faster
than 4; speedup over 1 thread 1.37x). The old path was described as
writer/sort-bound around 8 threads; the streaming path saturates even earlier,
at ~4 threads, because a single thread already runs at ~3.8 M events/s and the
run is sink/merge-bound, not compute-bound, at this scale.

### 2.3b Thread scaling at 1e7 after parallel Parquet encoding

(The tables above are the pre-change record and are kept as measured. This
subsection was added with docs/CONSISTENCY_MODEL.md section 14: per-column
streaming Parquet encoders appended in row-group order, byte-identical
output, plus the glibc malloc policy in the CLI. "old" below is the pre-change
writer rebuilt from HEAD's parquet.rs in a scratch tree and run interleaved
with "new" in the same session, best of two, loadavg 5 to 6 at start.)

Guards OFF:

| threads | old wall (s) | old events/s | new wall (s) | new events/s |
|---|---|---|---|---|
| 1 | 2.67 | 3.74 M | 2.62 | 3.80 M |
| 2 | 2.19 | 4.56 M | 2.17 | 4.59 M |
| 4 | 1.97 | 5.07 M | 1.94 | 5.15 M |
| 8 | 1.97 | 5.08 M | 1.91 | 5.24 M |
| 16 | 1.95 | 5.12 M | 1.84 | 5.41 M |
| 32 | 1.98 | 5.04 M | 1.89 | 5.28 M |

Guards ON, L = 1:

| threads | old wall (s) | old events/s | new wall (s) | new events/s |
|---|---|---|---|---|
| 1 | 3.01 | 2.76 M | 2.97 | 2.79 M |
| 2 | 2.59 | 3.21 M | 2.55 | 3.25 M |
| 4 | 2.30 | 3.61 M | 2.27 | 3.65 M |
| 8 | 2.24 | 3.70 M | 2.22 | 3.74 M |
| 16 | 2.24 | 3.70 M | 2.23 | 3.72 M |
| 32 | 2.29 | 3.62 M | 2.27 | 3.65 M |

Reading: at 1e7 the plateau is still at ~4 threads (new best 1.84 s vs old
1.95 s, 1.42x over 1 thread vs 1.36x). The single writer thread was
confirmed at 99.9% CPU before the change, but at this scale it was only just
co-bottlenecked with the engine's serial main-thread path (a discard sink
gives 1.85 s; skipping the stats collector as well gives 1.11 s), so removing
it exposes that path rather than a new plateau. Where the writer WAS the
binding bound, the gain is real: 1e8 guards off 17.17 s / 1.29 GiB -> 13.77 s
/ 0.61 GiB (5.81 -> 7.25 M events/s), guards on L = 1 16.85 s / 1.23 GiB ->
15.25 s / 0.74 GiB, 1e9 guards off 182.1 s / 7.92 GB (recorded) -> 167.1 s /
4.38 GiB. Byte-compat hashes for all six domains, the multi-row-group 1e7
file, a 4-shard run and the guards-on path are identical to the old writer;
sweep spot-points exact; 95 tests green.

### 2.3c Thread scaling at 1e7 after the pipelined day loop

(The 2.3 and 2.3b tables are kept as measured. This subsection was added
with docs/CONSISTENCY_MODEL.md section 15: the StatsCollector on its own
thread, Phase A of day w+1 overlapped with day w's sweep/handoff via a
one-day-ahead producer thread, and batches moved rather than copied into
the sink chain. No semantic change: all six domains byte-identical guards
off including saga_stats.json; guards-on edge stream, node state, counters
and stats identical at L = 0.01 / 1 / span and threads 1 vs 8; spot-points
exact; 95 tests green. "old" = this tree before the edits, built in a
scratch copy and run interleaved with "new" in the same session, best of
two, /usr/bin/time wall, loadavg 4 to 7 at start.)

Guards OFF:

| threads | old wall (s) | old events/s | new wall (s) | new events/s |
|---|---|---|---|---|
| 1 | 2.63 | 3.83 M | 1.74 | 5.80 M |
| 2 | 2.16 | 4.68 M | 1.25 | 8.15 M |
| 4 | 1.93 | 5.23 M | 1.00 | 10.27 M |
| 8 | 1.94 | 5.22 M | 0.94 | 10.94 M |
| 16 | 1.88 | 5.36 M | 0.94 | 10.94 M |
| 32 | 1.91 | 5.28 M | 0.93 | 10.94 M |

Guards ON, L = 1:

| threads | old wall (s) | old events/s | new wall (s) | new events/s |
|---|---|---|---|---|
| 1 | 2.99 | 2.80 M | 1.79 | 4.71 M |
| 2 | 2.60 | 3.21 M | 1.31 | 6.48 M |
| 4 | 2.31 | 3.63 M | 1.15 | 7.37 M |
| 8 | 2.31 | 3.63 M | 1.15 | 7.41 M |
| 16 | 2.27 | 3.69 M | 1.13 | 7.53 M |
| 32 | 2.29 | 3.66 M | 1.14 | 7.45 M |

(events/s is the engine's own figure from saga_meta.json, edges written over
engine wall; the wall column is the whole process.)

Reading: the plateau moved, 1.9 -> 0.94 s guards off and 2.3 -> 1.14 s
guards on (2.0x both), and it now sets in at 8 threads instead of 4.
Speedup over 1 worker is 1.87x guards off (was 1.38x); the 1-worker point
itself is faster because the stats, writer, producer and consumer threads
are busy concurrently even with a one-worker pool. Scale: 1e8 guards off
13.88 s / 0.65 GiB -> 7.99 s / 1.13 GiB (7.2 -> 12.6 M events/s), guards on
L = 1 15.33 s / 0.75 GiB -> 7.54 s / 0.92 GiB (4.9 -> 10.1 M events/s).
1e9 guards off 160.8 s / 4.28 GiB -> 89.4 s / 7.62 GiB (6.2 -> 11.2 M
events/s), guards on L = 1 174.9 s / 4.66 GiB -> 89.1 s / 6.27 GiB, one run
each, identical edge and violation counts old vs new.
RSS rose by a bounded constant (a few day batches held in flight by the
pipeline; see section 15 for the accounting). New bound, decomposed at 1e7
and 32 threads: full 0.92 s; Phase A alone (days drawn and dropped) 0.67 s,
and Phase A itself scales only 2.4x from 1 to 32 workers; the single stats
thread runs at 93% CPU and saturates at ~12 M rows/s, which is where 1e8
guards off now sits. Both are documented as the next ceiling; neither is a
semantics-free change to the JSON bytes in the stats case.

### 2.3d Size curve 1e6 / 1e7 and six-domain 1e8 on the pipelined binary

(Measured 2026-08-21 with the frozen `./target/release/agora` as is, so that
tab:scale in paper-vldb/main.tex is one engine throughout: the 1e8/1e9 rows
are CM section 15's. Finance, seed 42, `--anomaly-rate 0.02` as in section
15, parquet, threads auto = 32, best of two, `/usr/bin/time -v` wall and
peak RSS, loadavg 3.1 to 3.2 at start. Edge and violation counts equal the
2.1/2.2 rows exactly: 998,371 / 9,983,083 guards off; 887,156 (421,982
guarded, 20,437 violations) and 8,293,981 (3,647,983 / 455,343) guards on.)

| edges | guards off wall (s) | events/s (engine) | peak RSS | guards on L = 1 wall (s) | events/s | peak RSS | wall on vs off |
|---|---|---|---|---|---|---|---|
| 1e6 | 0.28 (0.28 / 0.28) | 3.81 M | 93 MiB (95,488 KB) | 0.29 (0.29 / 0.30) | 3.29 M | 106 MiB (108,800 KB) | +4% |
| 1e7 | 0.97 (0.99 / 0.97) | 10.57 M | 211 MiB (215,808 KB) | 1.13 (1.16 / 1.13) | 7.51 M | 224 MiB (229,376 KB) | +16% |

The 1e7 guards-off point (0.97 s) sits next to section 2.3c's 32-thread
0.93 s, a separate session; the 1.96 s / 2.18 s rows in 2.1/2.2 are the
pre-pipelining binary and are kept there as measured.

Six domains at 1e8, guards off, default anomalies, seed 42, one run each
(loadavg 6 to 11 at the start of each run, the preceding run's own threads
included):

| domain | edges written | wall (s) | events/s (engine) | peak RSS |
|---|---|---|---|---|
| finance | 99,809,412 | 7.95 | 12.64 M | 0.99 GiB (1,034,128 KB) |
| crypto | 98,209,576 | 11.13 | 8.87 M | 1.11 GiB (1,168,040 KB) |
| cyber | 99,429,075 | 10.70 | 9.35 M | 1.62 GiB (1,701,868 KB) |
| transport | 99,815,509 | 8.64 | 11.64 M | 1.40 GiB (1,466,064 KB) |
| ecommerce | 99,759,065 | 8.37 | 12.01 M | 1.44 GiB (1,505,692 KB) |
| healthcare | 99,246,428 | 13.31 | 7.49 M | 1.21 GiB (1,264,568 KB) |

Every domain reaches 1e8 labeled edges in 8 to 13 s within 1.7 GiB; this
replaces the serial-writer table (17 to 27 s, 1.3 to 2.2 GB) of
docs/EXPERIMENTS.md in tab:scale (b).

### 2.4 L sensitivity of throughput at 1e7, guards ON

| L (days) | blocks (90 d span) | edges written | wall (s) | events/s | violation rate |
|---|---|---|---|---|---|
| 0.01 | 9,000 | 8,613,782 | 2.66 | 3.24 M | 1.84% |
| 0.1 | 900 | 8,501,179 | 2.34 | 3.63 M | 5.93% |
| 1 | 90 | 8,293,981 | 2.17 | 3.83 M | 12.48% |
| 7 | 13 | 7,944,621 | 2.03 | 3.91 M | 25.84% |
| 90 (= span) | 1 | 9,524,713 | 3.85 | 2.48 M | 60.13% |

Comparison with the recorded 10M sweep (docs/remeasure_blocksize.md; that sweep
used CSV format and --no-anomalies): the qualitative shape reproduces (no cliff
at L = 0.01, wall decreasing toward moderate L, span is the slowest point at
+77% over L = 1 with the guard admitting more work), and the fidelity column
reproduces almost exactly (sweep: 1.84% / 12.50% / 25.78% / 60.16% at
L = 0.01 / 1 / 7 / span). One deviation to flag: the small-L overhead is
relatively larger here than in the recorded sweep. The sweep saw L = 0.01
within ~2% of its plateau; this parquet run has L = 0.01 at +23% over L = 1
(2.66 vs 2.17 s), i.e. roughly 45 to 55 us per block boundary at 9,000 blocks
after netting out the ~320k extra emitted edges, vs the sweep's inferred
sub-us-per-boundary cost at millions of boundaries. Part of the relative gap
is that the parquet sink baseline is ~1.3 s faster than the CSV sink, so the
same absolute boundary cost is a larger fraction of wall; the remaining
absolute gap suggests the per-boundary cost is not constant across block-count
regimes and is worth a footnote rather than a flat per-boundary figure.

### 2.5 Memory boundedness at 1e7, guards ON

| L (days) | peak RSS |
|---|---|
| 1 | 260 MiB |
| 7 | 453 MiB |
| 90 (= span) | 2.05 GiB |

RSS is NOT strictly flat between L = 1 and L = 7: it grows 1.7x (260 -> 453
MiB), consistent with per-block buffering proportional to events per block
(L = 7 blocks hold 7x the events). It stays bounded and small at any sub-span
L. L = span is the documented degeneracy: one block buffers the whole run,
2.05 GiB, 7.9x the L = 1 footprint at this scale.

## Summary

- Determinism: 45/45 randomized combos pass all applicable checks (thread
  invariance, rerun determinism, shard-union equivalence, guard-counter
  invariance) across all 6 domains, 5 seeds, 200k to 1M edges, L from 0.01 to
  4 days, threads 1 to 32, shards 1/2/4. Zero failures.
- Scale: ~5.8 M events/s and 1.32 GiB peak RSS at 1e8 guards-off; near-linear
  wall per decade through the prior 1e9 point (182.1 s / 7.92 GB, not rerun).
- Consistency cost: guards ON at L = 1 costs ~11% wall at 1e6/1e7 and is
  net-faster at 1e8 (because it suppresses 25% of edges); per-edge cost is
  ~+26%. Violation rate at fixed L rises with per-account density
  (4.8% / 12.5% / 31.3% at 1e6/1e7/1e8 with 100k fixed nodes).
- Surprise 1: the streaming path saturates at ~4 threads (not ~8), with only
  ~1.35x total parallel speedup at 1e7; it is sink/merge-bound.
- Surprise 2: small-L boundary cost at 1e7/parquet is a larger wall fraction
  than the recorded CSV sweep implied (L = 0.01 is +23% over L = 1 here vs ~2%
  there); no cliff, but the flat-plateau claim should be stated per-sink.
- Span degeneracy confirmed: L = span is simultaneously the slowest point
  (+77% wall), the worst fidelity (60% violations), and 7.9x the RSS (2.05 GiB
  vs 260 MiB at L = 1).
- Hygiene: no source/test/doc modified except this file; nothing committed; all
  generated graphs deleted.

## Regression suite (pinned invariants)

The invariants above that used to be re-checked by hand are pinned as tests
in `crates/saga-cli/tests/regress.rs` (they drive the real `agora` binary
through subprocesses, parse its `--json` summary, and read the output back
with `saga_io::read`). Every pin was computed from the binary on 2026-08-21.
Generated graphs go to unique temp directories the tests delete themselves.

Entry point: `scripts/regress.sh` builds release, runs the full default
suite, then the heavy pins, and prints a PASS/FAIL table (`--fast` skips the
heavy pins, `--heavy` runs only them). Manually:

    cargo test --release                                   # default suite
    cargo test --release -p saga-cli --test regress -- --ignored --nocapture   # heavy pins

Output identity is pinned at two layers so a failure says which one moved:
file sha256 (exact bytes: any change, including a writer change that keeps
content) and a writer-independent logical digest (sha256 over the sorted
lines `src,dst,t,event_type,label,amount-or-first-numeric-attr` read back
from the edge shards). "PARQUET BYTES" failing alone is a writer-only
change; "LOGICAL" failing is a semantic change.

| # | invariant | test | suite | pinned values |
|---|---|---|---|---|
| 1 | Guards-off default output, 1M edges, seed 42, threads 8, all 6 domains: full sha256 of edges_00000.parquet / labels_nodes.csv / ground_truth.json plus logical digest and edges_written | `bytecompat_1m_all_domains` | heavy (ignored) | parquet finance 5893b7aa..., crypto 9cb42c18..., cyber 28847983..., ecommerce e6550616..., healthcare efe4e66c..., transport 78f81253... (full 64-hex values in the test); logical finance a94e44c6..., crypto 072cbc88..., cyber ca00c2e6..., ecommerce fe868373..., healthcare c3c1f93f..., transport a44e8f60...; edges 998371 / 981636 / 993400 / 997103 / 993791 / 998236 |
| 1 fast | Same command at 100k edges: logical digest and edges_written | `bytecompat_100k_logical_all_domains` | default | logical finance 7707b2e3..., crypto d8841318..., cyber b2acf855..., ecommerce 813a2219..., healthcare ca9b318a..., transport cfa8a0b5...; edges 100195 / 98618 / 99427 / 99435 / 99224 / 100153 |
| 2 | Sweep spot-points: finance preset tiny, --no-anomalies, --guards, seed 42, threads 8, L in {0.001, 1, 30} | `sweep_spot_points_finance_tiny` | default | overdraft_violations 1214 / 66205 / 291420; edges_written 878315 / 821880 / 961453 |
| 3 | Guards-on determinism, all 6 domains, 20k nodes / 200k edges / 14 days, seed 42, L=1: threads 1 vs 8 equal logical digest and counters, and the counters match the pins | `guards_on_thread_determinism_all_domains` | default | (edges, anomalous, violations) finance 175768/1187/7332, crypto 58954/34/9782, cyber 161014/109/7803, ecommerce 168165/8231/4060, healthcare 189520/6103/863, transport 183982/139/1149 |
| 4 | Calibration tolerance: ecommerce 1M, L=1, --calibrate-under-guards 1: realized anomalous fraction within 2% relative of planned | `calibration_ecommerce_1m_within_tolerance` | default | planned 0.8608%, realized 0.8596% (7139 / 830459), gap 0.13% |
| 5 | Exchange equivalence: N in {2,4} real shard processes, small guarded finance, union == single machine == emulation (edges, node state) | `exchange_cli::real_processes_equal_single_machine_and_emulation` | default | equality checks |
| 5 heavy | N=2 real processes at the 1M exchange config (100k nodes, 1M edges, --guards, L=1, threads 8): union logical digest == single, edge count and overdraft counters pinned | `exchange_1m_two_processes_equal_single` | heavy (ignored) | edges 887,156; overdraft_guarded 421,982; overdraft_violations 20,437; logical d821f1cf... |

Measured 2026-08-21 on the i9-13900K: `cargo test --release` (whole
workspace) 107 passed / 0 failed / 4 ignored in 26 s wall (102 pre-existing
tests plus the 5 new default pins; the regress.rs default pins themselves
take 2.8 s). Heavy pins: 2 passed in 6.0 s of test time (16 s wall including
the cargo relink). `scripts/regress.sh` end to end: 31 s, all rows PASS.
Every 1M generation is about 0.3 s, so the "heavy" pins are ignored only to
keep the default suite's subprocess count down, not because they are slow.

## Post-hoc Phi coverage (`agora validate`)

Every constraint declared in a domain's Phi list is now accounted for by
`agora validate` (`crates/saga-cli/src/commands/validate.rs`):

| check kind | evaluated how |
|---|---|
| `attr_range`, `rate_limit`, `sub_threshold_count` | streamed over the edge shards (as before) |
| `state_non_negative` | post hoc over the exported node tables (`nodes_<entity>.parquet` or `nodes.csv`): per predicate the number and fraction of nodes whose FINAL state variable is below zero, the minimum value, and a PASS / VIOLATED verdict (SKIP when the run exported no node table) |
| `documented` | listed as `documented (not machine-checked)` so the report is complete over Phi rather than silent |

Guards (`--guards`) enforce a subset of the `state_non_negative` predicates
at generation time against the block-start snapshot; the validator
cross-references the run summary in `saga_meta.json` (`overdraft_guarded`,
`overdraft_violations`) and states that final-state violations on a guarded
run are the staleness leakage plus unguarded subtracting events (finance
purchases/withdrawals, for example). With guards off nothing is enforced and
the validator reports the unguarded dynamics. Phi results stay informational:
the hard verdict (`ok`) is unchanged. Tests: `crates/saga-cli/tests/validate_cli.rs`
(guards-off counts equal an independent CSV recount; guards-on at L = 1 s
equals the recount and is far below guards-off; documented constraints and
every declared state predicate appear, parquet and CSV paths agree).

Measured (2026-08-21, the frozen binary `./target/release/agora` as is; finance,
`--edges 200000 --seed 42`, default 100,000-node scaffold, 90-day span,
anomalies on, parquet; `agora generate ... --out D` then `agora validate D`):

| run | edges written | violating nodes (`account.balance` < 0) | fraction | min balance | `overdraft_guarded` | `overdraft_violations` (staleness leaks) |
|---|---|---|---|---|---|---|
| guards off | 199,203 | 4,821 of 90,000 | 5.3567% | -82,172.13 | 0 | 0 |
| `--guards --block-len-days 0.001` | 183,120 | 817 of 90,000 | 0.9078% | -3,468.80 | 90,140 | 130 |

The guards-on run's 817 final-state violations are the 130 staleness leaks
plus unguarded subtracting events (purchases and withdrawals). These are the
numbers quoted in paper-vldb/main.tex (sec:impl, guards paragraph).

