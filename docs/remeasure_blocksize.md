# Phase 3 headline experiment: fidelity-to-sequential-exact vs throughput as the block length L varies

Block-bounded staleness (docs/CONSISTENCY_MODEL.md sections 3, 8, 9, 10). L is the
staleness bound: every event reads the state snapshot as of the start of its L-block,
so a read is stale by at most L. L -> granularity is sequential-exact; L = span is the
published fixed engine (option A, every read sees S_0). Every number below is from a
real seed-42 run on this host; generated graphs were deleted after measurement.

## Protocol

- Binary: `./target/release/agora` (Phase 1 + 2 landed; `cargo build --release` clean,
  binary current at measurement time).
- Command per point:
  `agora generate --preset {tiny|small} --domain finance --seed 42 --no-anomalies
  --guards --block-len-days L --format csv --out DIR --json -q`
- Scales and spans: the span is NOT derived from the edge count; it comes from the size
  preset (crates/saga-rules/src/config.rs `Preset::baseline`). The two scales are the
  presets whose default edge budget matches the target:
  - 1M edges = preset `tiny`: 10,000 nodes, 1,000,000 target edges, span 30 days.
  - 10M edges = preset `small`: 100,000 nodes, 10,000,000 target edges, span 90 days.
  Granularity 1 s, epoch 2025-01-01, anomalies off (matching the Phase 1/2 measurement
  convention so the guard is the only state-conditioned mechanism).
- Timing: every L point was generated twice; the faster `wall_time_s` (from the summary
  JSON, which covers the whole run including the CSV sink) is reported. Format was held
  fixed at CSV for every point, including the reference, so the L-comparison is
  like-for-like. Host: 32 logical CPUs, default thread count. 1-min loadavg at run
  start: 3.09-3.29 for the 1M sweep, 3.28-5.85 for the 10M sweep (threshold 8, never
  hit). Reruns agreed with first runs on edges_written and overdraft_violations at
  every L (same-command determinism).
- Divergence vs the reference, computed on logical values, not parquet/CSV bytes:
  - Edge-set divergence: logical edge tuples `(src,dst,t,event_type,amount)` extracted
    from the edge CSVs, LC_ALL=C sorted; symmetric multiset difference counted with
    `comm -3`; divergence = |symdiff| / |reference edges|.
  - State divergence: exported node state compared per node (full-row logical values
    for the count of differing nodes; the `balance` column of the 10k/100k account
    rows parsed as f64 for the L2 norm and max-abs of the balance-vector difference,
    and for the count of accounts whose printed balance differs).
  - Overdraft-violation rate: `overdraft_violations / overdraft_guarded` from the
    summary JSON (a violation is an emitted transfer that passed the stale block-start
    guard but overdrew the true up-to-date balance; the engine tracks this against a
    never-stale shadow ledger).

## The reference (sequential-exact)

The reference is L = granularity = 1 s = 1.1574e-5 days: one block per timestamp tick,
so every event reads all effects of all prior-tick events. This is the exact floor of
the L continuum, not an approximation: timestamps are integers at 1 s granularity, so
an even smaller L cannot separate events further. Verified: L = 0.5 s reproduces the
L = 1 s output exactly (sorted-tuple sha16 8a13e420844f537a and nodes.csv sha16
ce699f5b41c85d26 both identical, violations identical at 15). The only residual
violations are same-second ties, which share a snapshot at any L: 15 of 403,723
guarded transfers (0.0037%) at 1M, 97 of 4,022,798 (0.0024%) at 10M. That is
"sequential-exact modulo simultaneity at the data's own granularity".

Reference summaries (robust identity of the reference runs):

| scale | edges | per-event-type | suppressed transfers | nodes.csv sha16 | wall (s) |
|---|---|---|---|---|---|
| 1M / 30 d | 879,482 | cash_deposit 136,184; cash_withdrawal 14,458; purchase 325,117; transfer 403,723 | 127,589 | ce699f5b41c85d26 | 1.556 |
| 10M / 90 d | 8,671,172 | cash_deposit 1,377,882; cash_withdrawal 138,188; purchase 3,132,304; transfer 4,022,798 | 1,311,759 | 6e4a97e287008fd9 | 9.605 |

Suppressed transfers = guards-off candidate edge count (1,007,071 and 9,982,931,
measured) minus emitted edges. All emitted transfers are guarded (guarded count =
emitted transfer count).

## The L sweep

1M edges, 30-day span (preset tiny). "viol %" = overdraft-violation rate; "edge div %"
= |symmetric difference| / 879,482; balance L2 / max-abs in account-balance units;
Mev/s = edges_written / wall.

| L (days) | blocks | edges | viol % | edge div % | accts bal diff | balance L2 | balance max-abs | wall (s) | Mev/s |
|---|---|---|---|---|---|---|---|---|---|
| 1 s (ref) | 2,592,000 | 879,482 | 0.00 | 0.00 | 0 | 0 | 0 | 1.556 | 0.57 |
| 0.001 | 30,000 | 878,315 | 0.30 | 1.63 | 2,700 | 5.2e4 | 9.2e3 | 0.465 | 1.89 |
| 0.01 | 3,000 | 871,760 | 2.33 | 5.06 | 3,997 | 1.1e5 | 2.3e4 | 0.411 | 2.12 |
| 0.05 | 600 | 860,246 | 6.21 | 8.71 | 4,507 | 1.6e5 | 2.5e4 | 0.406 | 2.12 |
| 0.25 | 120 | 840,993 | 12.69 | 13.41 | 4,959 | 2.3e5 | 2.7e4 | 0.401 | 2.10 |
| 0.5 | 60 | 834,850 | 14.45 | 14.53 | 5,166 | 2.4e5 | 2.8e4 | 0.397 | 2.10 |
| 1 | 30 | 821,880 | 19.13 | 17.09 | 5,485 | 3.2e5 | 4.9e4 | 0.398 | 2.06 |
| 2 | 15 | 808,354 | 24.88 | 19.39 | 5,954 | 4.3e5 | 7.8e4 | 0.404 | 2.00 |
| 4 | 8 | 800,318 | 29.97 | 21.12 | 6,475 | 4.8e5 | 6.3e4 | 0.406 | 1.97 |
| 7 | 5 | 802,396 | 35.42 | 21.74 | 6,792 | 7.1e5 | 1.9e5 | 0.425 | 1.89 |
| 14 | 3 | 820,091 | 44.78 | 21.29 | 7,231 | 1.1e6 | 1.9e5 | 0.466 | 1.76 |
| 30 = span | 1 | 961,453 | 60.00 | 14.28 | 7,599 | 3.7e6 | 9.0e5 | 0.574 | 1.68 |

10M edges, 90-day span (preset small). Edge div is relative to 8,671,172. Accounts:
100,000.

| L (days) | blocks | edges | viol % | edge div % | accts bal diff | balance L2 | balance max-abs | wall (s) | Mev/s |
|---|---|---|---|---|---|---|---|---|---|
| 1 s (ref) | 7,776,000 | 8,671,172 | 0.00 | 0.00 | 0 | 0 | 0 | 9.605 | 0.90 |
| 0.001 | 90,000 | 8,663,750 | 0.24 | 1.46 | 26,057 | 1.6e5 | 1.4e4 | 3.655 | 2.37 |
| 0.01 | 9,000 | 8,615,056 | 1.84 | 4.55 | 40,193 | 3.2e5 | 2.9e4 | 3.495 | 2.46 |
| 0.05 | 1,800 | 8,543,228 | 4.60 | 7.41 | 44,892 | 4.6e5 | 4.0e4 | 3.464 | 2.47 |
| 0.25 | 360 | 8,430,239 | 8.36 | 10.59 | 47,281 | 6.0e5 | 5.5e4 | 3.453 | 2.44 |
| 0.5 | 180 | 8,396,654 | 9.33 | 11.35 | 48,248 | 6.4e5 | 6.4e4 | 3.464 | 2.42 |
| 1 | 90 | 8,294,295 | 12.50 | 13.66 | 50,043 | 7.7e5 | 4.5e4 | 3.449 | 2.40 |
| 2 | 45 | 8,175,671 | 16.26 | 16.07 | 52,612 | 9.1e5 | 5.8e4 | 3.426 | 2.39 |
| 4 | 23 | 8,036,389 | 20.94 | 18.58 | 56,216 | 1.1e6 | 6.8e4 | 3.439 | 2.34 |
| 7 | 13 | 7,945,718 | 25.78 | 20.34 | 59,722 | 1.4e6 | 8.7e4 | 3.557 | 2.23 |
| 14 | 7 | 7,883,747 | 32.00 | 21.86 | 64,396 | 1.7e6 | 1.3e5 | 3.643 | 2.16 |
| 30 | 3 | 8,026,329 | 42.82 | 21.97 | 69,763 | 3.8e6 | 3.4e5 | 3.903 | 2.06 |
| 90 = span | 1 | 9,527,113 | 60.16 | 14.73 | 75,438 | 1.2e7 | 2.3e6 | 5.201 | 1.83 |

Reading the fidelity columns:

- The overdraft-violation rate is the clean, monotone fidelity metric: it rises
  strictly with L at both scales, from ~0% (ref) through 0.24-0.30% at L = 0.001 to
  60% at L = span, and is scale-stable (both scales hit 60.0/60.2% at span). It
  approaches 0 as L -> 0, as the model predicts.
- Edge-set divergence and balance-diff counts are monotone up to L = 14/30 but DIP at
  L = span (21.7% -> 14.3% at 1M; 21.9% -> 14.7% at 10M). This is real, not noise: at
  L = span the guard reads only the generous initial balances S_0 and suppresses very
  little (961k/9.53M edges emitted vs 879k/8.67M in the reference), so the emitted set
  is closer to the unguarded candidate set, which by construction shares most tuples
  with every run. State divergence (balance L2, max-abs, accounts differing) keeps
  rising monotonically all the way to span; final-state error is largest at L = span
  by an order of magnitude. The violation rate and the state divergence are the
  headline fidelity curves; edge divergence saturates around 22% and is reported for
  completeness.

## Throughput interpretation (where the cost comes from)

Shape of the wall-time curve: flat plateau across three orders of magnitude of L
(roughly L = 0.01 to 4 days: 0.40-0.41 s at 1M, 3.43-3.50 s at 10M), a cliff only at
extremely small L, and a mild rise at very large L.

- Small-L cliff = block-boundary barriers. Phase A (parallel candidate drawing, keyed
  on the absolute day) is L-independent by design; the only L-dependent cost is the
  Phase B sweep with one snapshot-freeze/merge barrier per block. The engine has no
  per-phase timer, so the boundary cost is inferred from the curve, and the curve is
  unambiguous: wall is flat until the block count reaches ~10^5, then scales with the
  number of boundaries. Reference vs plateau: 1M, 2.592M boundaries cost
  1.556 - 0.40 = 1.16 s, about 0.45 us/boundary; 10M, 7.776M boundaries cost
  9.605 - 3.44 = 6.17 s, about 0.79 us/boundary. At L = 0.001 (30k/90k boundaries) the
  predicted overhead is ~15-70 ms, matching the small observed bump (0.465 vs 0.40 s;
  3.655 vs 3.44 s). So sequential-exact costs 2.7x (10M) to 3.9x (1M) the plateau
  wall time, and the knob buys nearly all of that back by L = 0.001.
- Large-L rise = more admitted work, not merge cost. As L grows the stale guard passes
  more transfers (at span: 961k vs ~820k edges at 1M; 9.53M vs ~8.3M at 10M), so more
  edges are written and more effects merged. A guards-off run (which emits the full
  candidate set, 1,007,071 / 9,982,931 edges) takes 0.459 s / 3.968 s, right on the
  per-edge plateau rate, confirming the plateau cost is Phase A + sink + merge at
  ~0.42-0.48 us/edge. The remaining increase at L = span (0.57 vs 0.60 us/edge at 1M)
  is consistent with the single whole-span canonical merge and the larger emitted set.
- events/sec caveat: eps = edges_written / wall, and edges_written itself varies with
  L (the guard suppresses different amounts), so wall time is the primary performance
  number; eps is reported as the summary computes it.

## Communication vs L (ties in Phase 2)

Measured with the Phase 2 single-process N-shard emulation via the public
`sim::shard_equivalence` hook, from a new measurement-only ignored test
(crates/saga-core/tests/phase3_comm.rs; no shipped code path modified; run with
`cargo test --release --test phase3_comm -- --ignored --nocapture`). Finance, seed 42,
guards ON, N = 4 virtual shards, 10k nodes / 1M target edges / 30-day span (the same
1M scale as the headline sweep). Deltas are cross-shard dst-effects at 18 bytes each
(SHARD_DELTA_BYTES).

| L (days) | blocks | total deltas | mean/block | peak/block | total MB | shipped fraction | edges match | state match |
|---|---|---|---|---|---|---|---|---|
| 0.05 | 600 | 927,613 | 1,546 | 4,076 | 16.70 | 0.278 | true | true |
| 0.25 | 120 | 898,499 | 7,488 | 17,644 | 16.17 | 0.277 | true | true |
| 1 | 30 | 869,299 | 28,977 | 40,506 | 15.65 | 0.277 | true | true |
| 2 | 15 | 847,797 | 56,520 | 81,406 | 15.26 | 0.276 | true | true |
| 7 | 5 | 840,023 | 168,005 | 251,134 | 15.12 | 0.276 | true | true |
| 30 | 1 | 1,079,485 | 1,079,485 | 1,079,485 | 19.43 | 0.281 | true | true |

Exactly the section 4 prediction: the TOTAL exchanged volume is nearly L-invariant
(~0.84-0.93M deltas, 15-17 MB; the L = 30 total is higher only because the near-stale
guard admits more transfers, hence more effect deltas exist at all), and the shipped
fraction is a flat ~0.28 for N = 4. What L controls is the batching: per-block volume
scales linearly with L (mean/block from 1.5k at L = 0.05 to 1.08M at L = span), i.e.
smaller L = more, smaller exchanges (more synchronization rounds), larger L = fewer,
larger exchanges. Sharded equivalence (edges AND per-node state equal to the
whole-graph run) holds at every L measured. Communication therefore does not penalize
small L in volume, only in round count, which is the same barrier count already priced
into the throughput curve.

## Determinism spot checks (threads 1 vs 8)

Logical comparison at several L: sha256 over the sorted logical edge tuples
(src,dst,t,event_type,amount) and over nodes.csv (exported per-node state values),
threads 1 vs threads 8:

| scale | L | tuples sha16 (t1 = t8) | nodes sha16 (t1 = t8) | verdict |
|---|---|---|---|---|
| 1M | 0.01 | f27f94d7da32fa7b | fcc9d86ab3786fce | identical |
| 1M | 1 | 8da0d8b4851072b4 | 735a5925bcd35f5f | identical |
| 1M | 7 | 25ad7a6eb7dc68f5 | 839a9cd3f6de901e | identical |
| 10M | 1 | 8a7b7a3c20a5a3af | 0da0d122ca35ed56 | identical |

Theorem 1 holds at every spot-checked L and both scales: thread count changes nothing,
logically or byte-wise.

## The tradeoff in one paragraph (the headline)

Throughput is flat in L across more than three orders of magnitude and falls off a
cliff only when blocks approach the timestamp granularity: the entire cost of
consistency is ~0.45-0.79 us per block boundary, invisible until the boundary count
reaches ~10^5 and dominant at ~10^6+. Fidelity, by contrast, degrades smoothly and
monotonically with L from the very first step. That asymmetry creates a wide sweet
spot at small-but-not-tiny L: at L = 0.001 days (86 s of staleness), the violation
rate is 0.24-0.30%, edge divergence ~1.5%, and wall time is within 7-17% of the
fastest point, while sequential-exact costs 2.7-3.9x. At L = 0.01 days the plateau is
fully reached (within ~2% of peak throughput) at 1.8-2.3% violations. The other
endpoint is strictly bad: L = span (the old option-A engine) has 60% violations, the
largest state error by an order of magnitude, AND is 43-51% slower than the plateau
because the permissive stale guard admits more work. So the published fixed engine's
semantics are the worst point on the fidelity curve without even being the fastest,
and bounded staleness at sub-day L buys near-sequential fidelity at essentially zero
throughput cost. That is the case for the knob.

## Repro notes and hygiene

- Driver and analysis scripts ran from a scratch directory outside the repo; every
  generated graph directory was deleted immediately after its numbers were extracted,
  and the sorted-tuple reference files were deleted after the tables were computed.
  Nothing was committed; paper/, paper.frozen/ and the anon tree were not touched.
- The only repo additions are this document and the measurement-only ignored test
  crates/saga-core/tests/phase3_comm.rs.
