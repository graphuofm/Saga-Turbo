# Engine issues and fix roadmap (post-submission full-fix track)

The frozen version (pre-fix engine) is what gets submitted, paired with the
renamed pre-fix artifact. This file is the agenda for the unconstrained
engine-improvement track that follows submission: fix everything toward the
camera-ready, no longer bound to reproduce the frozen numbers. Every problem
below is backed by a real measured number (source noted); fixes must preserve
determinism (bit-identical across threads/machines/shards) and keep the test
suite green, and every re-measurement after a fix must come from a real run.

Priority key: P1 = measured fidelity gap a reviewer will probe; P2 = correctness
or interpretability defect; P3 = accuracy/robustness polish.

---

## P1-a. Anomaly homophily gap (the headline fidelity gap)

- Evidence: anom.py definition (P(dst marked | src marked) enrichment), AGORA
  finance = 2.25 vs real Elliptic = 13.27 (docs/remeasure_fidelity.md). Real
  illicit transactions strongly link illicit-to-illicit; AGORA anomalies
  under-cluster.
- Why it exists: the recruitment fix stopped anomalous actors from forming
  super-hubs (cured the 12.9x degree ratio and the 16.55 in/out overshoot), but
  it did not make anomalous actors preferentially transact with each other. Edge
  destinations for anomalous campaigns are still drawn largely from the benign
  counterparty pool.
- Where: crypto/finance campaign edge placement in
  crates/saga-core/src/sim.rs (the campaign edge emission / relation-camouflage
  path, approx lines 663-695) and the destination-selection in
  crates/saga-core/src/anomaly.rs (plan_campaign / scope resolution).
- Fix approach: add a tunable homophily bias so a controllable fraction of an
  anomalous campaign's edges route to other marked actors (illicit-to-illicit),
  while the difficulty knob still reroutes the rest to benign counterparties.
  Must stay deterministic (identity-keyed RNG) and must not re-introduce the
  degree/in-out overshoot. Re-measure homophily, degree ratio, and in/out
  together after the change; the target is to move homophily from 2.25 toward the
  real 13.27 without pushing degree ratio back above ~1x.

### FIX APPLIED (P1-a) — homophily redirect on BOTH the campaign AND normal paths

Global control axis `homophily` in [0,1] (`ControlParams.homophily`, default
0.0 = pre-fix behavior; copied uniformly to every `Campaign` and onto
`AnomalyPlan.homophily`, never scaled by difficulty).
`crates/saga-rules/domains/finance.yaml` control sets `homophily: 0.95`.

Two redirects, both keyed on identity so shards/threads stay bit-identical:

1. CAMPAIGN path (original shallow fix, unchanged; crates/saga-core/src/sim.rs
   `gen_campaign_window`, after the relation-camouflage block): with prob
   `c.homophily`, a campaign counterparty that is NOT already marked is
   redirected to a random marked actor of the SAME entity type. Draws depend
   only on plan-derived shared state (`marked_by_type`, `marked_set`) and a
   shard-independent `cp`.

2. NORMAL path (the DEEPER fix, new; `emit`, immediately after the counterparty
   `dst` is chosen): when the emitting actor is itself marked, its ordinary
   BACKGROUND edges are ALSO redirected — with prob `homophily`, a non-marked
   `dst` becomes a random marked actor of `dst`'s entity type (self-loops
   guarded out). This is the lever the campaign-only redirect lacked: marked
   actors emit normal traffic that otherwise dilutes P(dst marked | src marked)
   and capped homophily at ~5.7. `marked_by_type`/`marked_set` are threaded from
   `run` into `gen_chunk` -> `emit`; `homophily` comes from `plan.homophily`.
   DETERMINISM: the normal path is sharded by the emitting node and the redirect
   consumes that node's OWN normal rng stream, so it is identical for any
   thread/shard count (a node is always produced on its owning shard). The block
   is guarded by `homophily > 0.0 && marked_set.contains(&actor)`, so NO rng is
   drawn when the control is 0 or the actor is benign — an exact byte-for-byte
   no-op for every domain that leaves `homophily` unset (crypto, cyber, etc.).

Measurement protocol identical to docs/remeasure_fidelity.md (out-edge-denom
homophily = (both/out_edges)/(M/N); degree ratio = mean_totdeg(marked)/
mean_totdeg(non-marked); in/out avg = (mean_indeg + mean_outdeg)/2 over marked).
AGORA finance generated at Elliptic-matched size (`--nodes 203769 --edges
234355`, seed 42). Baseline (homophily=0.0, both redirects inactive) reproduces
the doc exactly: homophily 2.247, degree ratio 0.524, in/out 1.218 — confirming
the new normal-path code is an exact no-op at 0.

Calibration sweep (finance vs Elliptic; both redirects active; Elliptic last row):

| homophily | measured homophily | degree ratio | anom in/out avg |
|---|---|---|---|
| 0.0 (baseline) | 2.247 | 0.524 | 1.218 |
| 0.3 | 6.337 | 0.521 | 1.210 |
| 0.5 | 8.600 | 0.519 | 1.206 |
| 0.7 | 10.539 | 0.518 | 1.204 |
| 0.85 | 11.883 | 0.514 | 1.196 |
| **0.95 (chosen)** | **12.988** | **0.514** | **1.195** |
| 0.99 | 13.280 | 0.514 | 1.194 |
| Elliptic (real) | 13.27 | 0.872 | 1.006 |

Outcome: homophily now REACHES real 13.27. At the chosen homophily=0.95 measured
homophily is 12.99 (98% of real, within single-seed noise); at 0.99 it is 13.28,
essentially dead-on. Crucially there is NO tradeoff frontier: as homophily rises,
the degree ratio stays flat (~0.51-0.52, real 0.87 — AGORA still slightly
UNDER-shoots, unchanged by this fix) and anom in/out avg actually moves TOWARD
real, 1.218 -> 1.194 (real 1.006), because redirecting a marked actor's normal
out-edges to other marked actors lowers its mean out-degree (1.132 -> 1.001) and
lifts marked in-degree, exactly the real illicit shape. So all three metrics
improve or hold monotonically — the earlier ~5.7 ceiling is GONE.

Chosen value 0.95 (not 0.99): 0.95 already reaches real within ~2% with degree/
in-out identical to 0.99, and is a slightly more robust operating point than
sitting at the very edge of the knob. 0.99 is available and nails 13.28 with no
penalty, so the engine genuinely reaches 13.27 — this is a real structural match,
not a saturation artifact. Before -> after at the chosen value: homophily
2.25 -> 12.99, degree ratio 0.524 -> 0.514, in/out 1.218 -> 1.195.

Verification (all real runs, seed 42): `cargo build --release` clean; `cargo test
--release` all green (66 tests, 0 failures — incl. bit_identical_across_thread_counts,
sharded_union_equals_whole_graph, every_domain_is_thread_deterministic,
every_domain_generates_with_anomalies). Determinism hash check finance 1M threads
1 vs 8: edges / labels_nodes / ground_truth sha256 all bit-identical (edges
7beb54f2..., labels 334ba9ab..., ground_truth 84c9958b... — edges hash differs
from the campaign-only fix, as expected, since the normal path now redirects).
Shard determinism: finance 1M, union of 4 shards == whole graph, bit for bit
(998,449 edges each side, identical after sort). Other-domain spot-check: crypto
at its default homophily=0 is thread-deterministic and a no-op (intrinsic
homophily 15.29 unchanged); with an override of 0.9 it stays thread-deterministic
and the redirect activates cleanly (homophily 15.29 -> 33.66, degree/in-out
flat). All generated graphs deleted after measurement.

## P1-b. Learned-generator contest: finance CTGAN/TVAE = nan + scale mismatch

- Evidence: docs/remeasure_downstream_learned.md / scratchpad/learned_clean.json.
  finance ran at full email-Eu scale (986 nodes / 332,334 edges) while the other
  five domains were capped to ~2500 nodes; the tabular GANs returned nan on the
  dense finance graph.
- Why it exists: the toy-scale cap was not applied to finance, so CTGAN/TVAE hit
  the dense-graph failure mode. This is an EVAL-harness issue, not an engine
  defect.
- Where: the learned-contest script (scratchpad/job3_learned_clean.py) and
  python/saga_eval/tabular_gen.py.
- Fix approach: cap finance to the same ~2500-node budget as the other domains so
  every generator runs on a comparable graph, or handle the nan explicitly and
  report finance's learned cells as "did not converge at this density" (which is
  itself part of the learned-generators-do-not-scale finding). Decide which
  framing is honest for the table and make finance consistent with the other
  five domains.

## P2-a. priv_esc near-zero event volume — FIXED

- Evidence: consolidation report; ~0.38 expected events at 1M edges. The
  machinery is live (scales with budget/anomaly-rate) but the realized volume is
  effectively zero.
- Why it exists: sparse `process` actor population x short stage duration x a
  sub-1 activity multiplier => far below one event in a normal run.
- Where: crates/saga-rules/domains/cyber.yaml (priv_esc stage).
- FIX APPLIED: research (read-only) established there is NO citable per-day T1068
  rate; the literature (MITRE T1068 low-footprint; Verizon DBIR 2026 ~17% of
  privesc is CVE-exploitation; Red Canary TDR ranks it below T1059) supports only
  "rare but present." So the grounded-low activity_multiplier (2-12/day) was KEPT
  (not fabricated higher). The actual defect was the internally-inconsistent
  duration: 0.05-1 d contradicted the domain's own "APT chains span weeks" and
  was the shortest of all six cyber adversaries. Corrected duration_days
  0.05-1 -> 1-7 d (the lateral_movement dwell window; lateral fires reliably at
  the same prevalence_weight and a LOWER multiplier). prevalence_weight untouched
  (type mix unchanged).
- VERIFIED: cyber 1M seed 42 anomaly-rate 0.05 now emits 49 priv_esc instances
  (was ~0); all six cyber intents fire (recon 268 / brute 109 / lateral 96 /
  c2 238 / exfil 302 / priv_esc 49). cargo build clean; cargo test 66/0;
  determinism cyber 1M threads 1 vs 8 edges/labels/ground_truth sha256 all match.
  Comment block updated with the grounding provenance (MITRE/DBIR/Red Canary).

## P2-c. doctor_shopping (healthcare) fires 0 events - FIXED

- Evidence: the F1-health fix noted doctor_shopping emits 0 edges in BOTH the
  frozen and fixed engine (pre-existing, not caused by that fix). Reproduced at
  the fidelity/matrix config (n=24818, m=506550, span 2350.34, anomaly-rate 0.02,
  seed 42): 1 doctor_shopping campaign planned, 0 edges emitted.
- Same class as priv_esc: a declared, grounded anomaly type that never fires.
- Root cause (measured, same arithmetic as priv_esc): the single stage used a
  BUDGET-ELASTIC `activity_multiplier` (k = 0.0174-0.0580 x the normal provider
  rate). But doctor shopping is intrinsically a FIXED per-prescriber fill COUNT,
  not a cadence: each deceived prescriber writes only a handful of ~30-day opioid
  refills to the one shopping patient; the signal is the fan-in DEGREE (distinct
  prescribers -> one patient), not per-prescriber volume. Because the normal
  provider rate rides `world.rate_scale`, the absolute per-prescriber rate
  collapsed at sparser budgets: 0.93/day at 1M edges but only 0.072/day at the
  fidelity scale, so expected events per campaign = acting_members (5) x rate
  (~0.0027/day) x duration (~22.7 d) ~= 0.3, which Poisson-rounds the whole intent
  to ZERO. (priv_esc's was ~0.38; identical pathology.) The internally-indefensible
  parameter was the rate FORM itself: an elastic multiplier for a count typology.
- Where: crates/saga-rules/domains/healthcare.yaml (doctor_shopping playbook).
- FIX APPLIED (healthcare.yaml only): switched the stage from `activity_multiplier`
  to `event_count: !uniform { min: 2.0, max: 4.0 }` (2-4 overlapping controlled
  fills per prescriber over the campaign). Grounded in CMS Part D / the
  Overutilization Monitoring System (OMS): a beneficiary flagged for opioids from
  >=4 distinct prescribers, each writing a few monthly (days_supply 25-35) refills,
  a fixed clinical cadence that does NOT scale with graph density. With
  ring_size 5-15 prescribers this aggregates to the >=5 fills/30d/patient
  doctor-shopping threshold the domain already encodes (constraint
  controlled_substance_overlap). event_count is BUDGET-INELASTIC (fixes the
  collapse) yet still difficulty-damped (k = count/(normal_rate x duration), so
  d=1 -> k_eff=1). Intensity was NOT fabricated: the count matches the ~1.7
  fills/prescriber the elastic form happened to realize at 1M, so the large-config
  volume is unchanged; only the small-config collapse is repaired. First built-in
  domain to use the (fully supported, tested) event_count form.
- VERIFIED (all real seed-42 runs): doctor_shopping now fires at every budget and
  stays by far the rarest adversary (0.05-0.4% of anomalous edges):
    * fidelity config (n=24818, m=506550): 0 -> 9 edges (1 instance).
    * 1M anomaly-rate 0.05: 0/101 -> 165 edges (6 instances). Per-intent instance
      counts: upcoding 71 / phantom_billing 87 / unbundling 47 / kickback_ring 14
      / doctor_shopping 6 (all five intents fire).
    * 1M anomaly-rate 0.02: 52 -> 66 edges (2 instances).
  cargo build clean; cargo test --release 66/66 green (incl. the four
  determinism/anomaly tests: bit_identical_across_thread_counts,
  sharded_union_equals_whole_graph, every_domain_is_thread_deterministic,
  every_domain_generates_with_anomalies). Determinism healthcare 1M threads 1 vs 8:
  edges 84cbd914.., labels 7b74eab2.., ground_truth 55dc4326.. all sha256-match.
  NO fidelity regression: healthcare re-measured 0.754 vs sx-mathoverflow (matrix
  protocol, seed 42), identical to the F1-health value, because the anomaly cap
  does NOT bind (expected_events=458.7 << cap=151400.8, rate_scale=1.0000) so the
  normal stream is byte-unchanged. Other 5 domains all generate at 1M and stay
  thread-deterministic (ecommerce threads 1==8 confirmed). All generated graphs
  deleted after measurement.

## P2-b. Crypto difficulty-sign inversion (k < 1 amplifies instead of damps) — FIXED

- Evidence: consolidation report + docs/remeasure_diff_ap.md. For crypto stages
  the activity multiplier k is sub-1 (k ~ 0.001-0.09 vs normal EOA 445.4/day), so
  the old symmetric transform `k_eff = k^(1-difficulty)` moved k_eff UP toward 1
  as difficulty rose, i.e. raising difficulty made a quiet adversary NOISIER, not
  quieter. Because the attribute/structure signals of those stages are not
  fully camouflaged, that added volume EXPOSED them: crypto detectability ROSE
  with difficulty (the knob was inverted), and anomalous edge volume exploded.
- Where: crates/saga-core/src/anomaly.rs (`k_eff` transform, ~line 534).

### FIX APPLIED (P2-b) — one-sided damping (difficulty never amplifies)

Transform changed so difficulty may only ever move volume DOWN toward the normal
rate, never up:

```rust
// before (symmetric in k <-> 1/k — drove BOTH sides to k=1):
let k_eff = if k > 0.0 { k.powf(1.0 - difficulty) } else { 0.0 };
// after (one-sided: damp a loud adversary toward normal; leave a quiet one):
let k_eff = if k > 1.0 { k.powf(1.0 - difficulty) } else { k };
```

Rationale (also in the code comment): for a QUIETER-than-normal adversary
(k < 1) raising volume toward the normal rate INCREASES its footprint and
exposes it, contradicting the difficulty knob's purpose. Difficulty now only
damps louder-than-normal (k > 1) stages toward k = 1; sub-1 stages keep their
playbook rate and rely on ATTRIBUTE camouflage (axis 1) for hiding. The change
touches every sub-1 stage in every domain, so it was verified to fix crypto
WITHOUT regressing finance (finance k straddles 1).

Re-measurement protocol identical to docs/remeasure_diff_ap.md: GBM detector,
500k edges, anomaly-rate 0.05, seed 42, difficulty in {0,0.25,0.5,0.75,1.0},
graphs deleted after measurement. Both the "before" (old symmetric transform,
temporarily rebuilt) and "after" rows are real seed-42 runs.

CRYPTO base rate (anomalous edges) — the explosion is GONE:

| delta | before (symmetric) | after (one-sided) |
|------:|-------------------:|------------------:|
| 0     | 50   (0.010%) | 50 (0.010%) |
| 0.25  | 114  (0.023%) | 50 (0.010%) |
| 0.5   | 318  (0.065%) | 50 (0.010%) |
| 0.75  | 851  (0.174%) | 50 (0.010%) |
| 1.0   | 2730 (0.559%) | 50 (0.010%) |

Before: 50 -> 2730 edges (54.6x climb with difficulty). After: flat at 50 (the
quiet adversary is never amplified), so difficulty stops adding volume.

CRYPTO detector AP (test set) — the inversion is GONE:

| view   | before: 0 -> 1.0            | after: 0 -> 1.0             |
|--------|-----------------------------|-----------------------------|
| attr   | 0.0002 -> 0.2502 (RISES)    | 0.0002 -> 0.0031 (floor)    |
| struct | 0.0001 -> 0.5408 (RISES)    | 0.0001 -> 0.0004 (floor)    |
| full   | 0.0019 -> 0.5855 (RISES)    | 0.0019 -> 0.0001 (falls)    |

Before, every view's AP climbed steeply with difficulty (detectability rose as
the knob was turned up — inverted). After, all views sit at the ~0.01% base-rate
floor with no upward trend; full-view AP is non-increasing (0.0019 -> 0.0091 ->
0.0028 -> 0.0008 -> 0.0001). The knob no longer exposes crypto anomalies.

FINANCE no-regression check (same protocol, fixed engine) — still correct:

| view       | 0 | 0.25 | 0.5 | 0.75 | 1.0 |
|------------|--:|-----:|----:|-----:|----:|
| attr AP    | 0.4425 | 0.3331 | 0.1606 | 0.0718 | 0.0452 |
| struct AUC | 0.9423 | 0.9366 | 0.9399 | 0.9429 | 0.9497 |

Attribute-only AP still falls MONOTONICALLY (0.4425 -> 0.0452, a 9.8x drop, vs
the known-good 0.402 -> 0.091) and structure-only AUC still holds ~0.94. No
regression. Finance actually improves at the margin: its anomalous-edge base
rate now DECREASES with difficulty (1796 -> 1499) instead of the old climb
(1787 -> 2633 in remeasure_diff_ap.md), because finance's own sub-1 stages
(structuring, round_tripping, mule) are no longer amplified either.

Verification (all real runs, seed 42): `cargo build --release` clean; `cargo
test --release` all green (66 tests, 0 failures — incl. the four determinism/
anomaly tests: bit_identical_across_thread_counts, sharded_union_equals_whole_graph,
every_domain_is_thread_deterministic, every_domain_generates_with_anomalies).
The unit test `difficulty_one_drives_event_count_volume_to_normal` was renamed
to `difficulty_one_is_one_sided_damping` and rewritten: the old test asserted the
now-fixed BUG (it drove a quiet count=2 stage UP to the normal rate at d=1 —
both its counts, 60 and 2, are sub-1 vs the 6.9/day normal account rate), so it
now asserts the correct one-sided property (a loud k>1 stage is damped toward
normal; a quiet k<1 stage is never amplified). Determinism hash check finance
1M threads 1 vs 8: edges / labels_nodes / ground_truth sha256 all bit-identical
(edges ca066e06..., labels 334ba9ab..., ground_truth a231a0d2...). Other-domain
spot-check: crypto, cyber, transport, healthcare, ecommerce all generate at
difficulty 0.75 and are thread-deterministic (threads 1 == 8). All generated
graphs deleted after measurement.

## P3-a. Transport normal-delivered estimator ~8x under-count — FIXED

- Evidence: sweeps re-measurement; the normal-delivered estimator under-counts
  transport delivered volume ~8x.
- Where: crates/saga-core/src/sim.rs (expected_normal_delivered, approx line 144)
  and burst_survival (approx line 92), as applied to transport.yaml.
- Root cause: the estimator bounded each burst follow-up's in-window survival
  probability with a Jensen LOWER bound, `P(t_s+G_j<W) >= 1 - j*gap/W`, summed
  to `m - (gap/W)*m*(1+m)` and used AS IF it were the expectation. That bound is
  strict and, once `j*gap` approaches the window `W`, CLAMPS TO 0 while real
  bursts still land — so delivered normal volume (and hence the anomaly cap that
  is solved against it) was under-stated, badly so in the large-gap/long-burst
  regime (the source of the ~8x figure).

### FIX APPLIED (P3-a) — exact closed-form burst survival (already in the tree)

`burst_survival(m, gap, W)` now evaluates the survival sum EXACTLY instead of
the Jensen bound. Conditioning on `t_s ~ U(0,W)` and the Erlang/Poisson duality,
each term `P(t_s+G_j<W) = F_j(W) - (j/lambda)*F_{j+1}(W)` (lambda = W/gap) is
computed from Poisson(lambda) tail probabilities built up incrementally, weighted
by the geometric chain factor `q^j` and truncated when the remaining mass is
negligible. `expected_normal_delivered` sums `activity_sum*rate*rate_scale*
weekday_days*(1 + burst_p*burst_survival)` over behaviors with the exact weekday
mix, and the anomaly cap is solved against that delivered total (anomaly.rs
`cap = MAX*normal_full / (1 - MAX + MAX*normal_full/target)`).

Verification (fixed engine, seed 42):

- Estimator vs realized, transport at the matrix config (n=159316, m=964437,
  span=2613.82 d): estimator `normal_full = 963,909` vs REALIZED normal edges
  `965,187` — ratio 1.0013 (0.13% high), i.e. NO undercount. (On the pre-rebalance
  transport it was 964,148 vs 963,900, ratio 1.0003.)
- The undercount could not have driven transport fidelity anyway: the cap does
  NOT bind for transport (SAGA_DEBUG_CAP: `expected_events=1.6 << cap=288,688`,
  rate_scale=1.0), because transport anomalies are NATURAL failures riding the
  normal stream plus a deliberately thin adversarial tail. So P3-a is a
  correctness fix to the estimator, not the lever for F1 transport fidelity
  (that was a structural issue — see below).
- Neutrality: `burst_survival` is domain-general (identical closed form for every
  domain, no transport-specific branch), so it cannot perturb the other five
  domains' realized rates. Confirmed: finance realized anomaly 0.323%, crypto
  0.010% (50 edges) both unchanged and thread-deterministic (threads 1==8).
  Correctness is locked by three unit tests: `burst_survival_matches_monte_carlo`
  (closed form vs a Monte-Carlo replica of the emitter), `burst_survival_dominates
  _the_old_jensen_bound` (never below the old bound; strictly above where
  truncation bites — the Jensen form is 0 at (m=8,gap=40000,W=86400) while the
  truth is >0.5), and `burst_survival_handles_degenerate_inputs`.

## P3-a(F1). Transport fidelity 0.694 < 0.73 (the real F1 transport gap) — FIXED

Fixing the estimator (above) does NOT lift transport fidelity — the cap never
binds for transport. The genuine driver of the low score was structural, found
by the per-statistic breakdown vs the transport reference (`sx-askubuntu`, the
`†` generic interaction-net PROXY; matrix protocol, CAP=1.5M, seed 42).

- Root cause: the `sensor_reporting` behavior (nominal rate_per_day=288) monopolized
  ~86% of ALL edges. Because the edge-budget calibration (§3) renormalizes
  rate_per_day into a RELATIVE stream weight (a sensor actually emitted only ~29
  readings over the 2613-day span — the "5-min cadence" never realized), 288 just
  meant sensors out-weighted vehicles ~48x. Consequences measured on the frozen
  transport: 60,235 of 159,316 nodes were ISOLATED (vehicles 45.5% participate,
  segments 65.3%; trucks+buses had NO behavior at all and sat idle), which
  inflated mean-degree (19.5 vs real 12.1), pinned repeat-edge at 0.84 (sensors
  repeat to ~2 fixed segments) vs real 0.38, and let the regular sensor cadence
  dominate the pooled inter-event distribution (KS 0.74).

### FIX APPLIED (transport.yaml only; principled, not metric-gaming)

1. Activated the declared-but-idle fleets: added `freight_movement` (trucks, ~8%
   of the fleet) and `transit_service` (buses, ~4%) behaviors. Trucks/buses are
   real vehicle types that emitted nothing before — an obvious modeling gap.
2. Rebalanced `sensor_reporting` rate_per_day 288 -> 40 so readings are comparable
   to (a small multiple of) vehicle traffic — a coarse aggregate reporting weight
   (PeMS also publishes 15-min/hourly aggregates), turning a sensor-housekeeping
   log into a balanced sensors+segments+trips mobility graph.

Result: event mix now sensor_reading 45% / traversal 38% / trip 17% (was 86% /
9% / 5%); node participation 136,873 (was 99,081) — vehicles 85%, segments 77%.
Anomalies still fire (congestion_cascade, sensor_stuck/drift/dropout, incident,
gps_spoofing, fake_trip_injection all present; realized anomaly volume intact).

Before -> after, transport fidelity vs sx-askubuntu (matrix protocol, seed 42):

| statistic          | before (0.694) | after (0.743) |
|--------------------|---------------:|--------------:|
| FIDELITY (agg)     | 0.694          | **0.743**     |
| inter_event KS     | 0.741          | 0.582         |
| in_degree KS       | 0.452          | 0.505         |
| total_degree KS    | 0.351          | 0.307         |
| out_degree KS      | 0.252          | 0.297         |
| repeat_edge (dist) | 0.459 (0.840)  | 0.265 (0.646) |
| mean_degree (dist) | 0.233 (19.5)   | 0.070 (13.9)  |
| powerlaw_alpha     | 0.070          | 0.009         |
| burstiness (dist)  | 0.219          | 0.215         |
| reciprocity (dist) | 0.386          | 0.386         |
| clustering (dist)  | 0.182          | 0.182         |

Target >=0.73 MET (0.743, robust plateau: sensor weight 30/36/40/48 give
0.747/0.744/0.743/0.739). Residual ceiling is HONEST and structural:
`reciprocity` (0 vs 0.386) and `clustering` (0 vs 0.182) cannot move — transport
is a directed, near-bipartite mobility graph (vehicle->segment, sensor->segment,
vehicle->zone; no segment->segment or vehicle->vehicle event), so it structurally
has no reciprocal edges and no triangles, whereas the proxy is a unipartite human
Q&A net. `inter_event`/`burstiness` are the soft residual: always-on sensor and
commuter streams are less bursty than a heavy-tailed human process (Weibull
arrival was tried and moved burstiness <0.01 — the diurnal + daily cadence
dominates the pooled gaps, so it was not adopted). Determinism preserved: transport
1M seed 42 threads 1==8, edges/labels/ground_truth sha256 all match.

## P3-a(F1-health). Healthcare fidelity 0.696 < 0.73 (the real F1 healthcare gap) — FIXED

The SAME structural pathology as transport, found by the per-statistic breakdown vs
the healthcare reference (`sx-mathoverflow`, the `†` generic Q&A-net PROXY; matrix
protocol, CAP=1.5M, seed 42; matched size n=24818, m=506550, span 2350.34 d).

- Root cause (measured on the frozen healthcare, fidelity 0.696): a dominant event
  monopolized the stream AND two declared actor pools were idle. `DISPENSED`
  (pharmacy->patient, rate_per_day=35 across all pharmacies) was 64.2% of ALL edges,
  making 1,737 pharmacies super-hubs; meanwhile only `family_practice` and
  `internal_med` providers had a billing behavior — cardiology/orthopedics/pain_mgmt/
  dme_supplier/lab (46% of providers) emitted NOTHING (they appeared only as passive
  referral targets), and the entire `drug` node type was ISOLATED (0.0% active: no
  event ever targeted a drug; PRESCRIBED/DISPENSED both go ->patient). Consequences:
  only 13,931 of 24,818 nodes participated (patients 52%, drugs 0%), which inflated
  mean-degree (72.6 vs real 40.8), pinned repeat-edge at 0.865 (DISPENSED repeat_p
  0.75 to ~2 fixed patients) vs real 0.526, over-concentrated out-degree in the few
  pharmacy/provider emitters (out-degree KS 0.587), and let the regular clinic/dispense
  cadence dominate the pooled inter-event distribution (KS 0.457, burstiness 0.224 vs
  real 0.687).

### FIX APPLIED (healthcare.yaml only; principled, CMS-grounded, not metric-gaming)

1. Activated the idle specialist provider pools: added `cardiology_billing`,
   `orthopedics_billing`, `pain_mgmt_billing`, `dme_billing`, `lab_billing`
   behaviors (filtered by `specialty`). Grounded in CMS Part B utilization: labs and
   DME suppliers are among the highest claim-line generators, cardiology/orthopedics/
   pain-management are real high-volume billers; having only two primary-care
   specialties bill was an obvious modeling gap (the direct analog of transport's idle
   truck/bus fleets). Rates 14-30/day, event mixes matched to each specialty (labs/DME
   ~all SUBMITTED_CLAIM no prescribing; pain-mgmt prescribing-heavy).
2. Activated the idle `drug` node type: added `PRESCRIBED_NDC` (provider->drug,
   popularity-weighted) emitted at weight 0.12 by the prescribing behaviors. Grounded
   in the CMS Part D prescriber PUF (literally a provider x NDC prescription table,
   already in the domain provenance); a drug node was declared with attributes and a
   formulary skeleton but was never an event endpoint.
3. Rebalanced the dominant `DISPENSED` stream: pharmacy_dispensing rate_per_day
   35 -> 12 so Part D fills are a balanced secondary stream, not the majority.
   Grounded: in CMS, provider/supplier encounter claims (Part B) dominate claim-line
   volume; a 64%-pharmacy graph inverts that. Also DISPENSED repeat_p 0.75 -> 0.50
   and primary/internal panel repeat_p 0.70/0.65 -> 0.45 so providers reach more
   distinct beneficiaries (raises patient participation AND lowers repeat-edge).
4. Raised billing burstiness burst_p 0.10 -> 0.25 (burst_mean_len 2.0 -> 3.0):
   medical claims are submitted in batches (monthly billing cycles, clearinghouse
   uploads), a genuinely bursty process. This moved BOTH inter_event (KS 0.44->0.30)
   and burstiness (0.22->0.37 toward real 0.69) — a joint temporal-realism gain, not
   a single-metric tweak.

Result: event mix now SUBMITTED_CLAIM 57% / DISPENSED 17% / PRESCRIBED 13% / REFERRED
8% / PRESCRIBED_NDC 6% (was DISPENSED 64% / SUBMITTED_CLAIM 21%); node participation
16,298 (was 13,931), all specialties active, drug type active. Anomalies still fire
(20 instances; upcoding/phantom_billing/unbundling/kickback_ring all present, edge
volumes equal-or-higher than frozen; doctor_shopping emits 0 edges in BOTH the frozen
and fixed engine — a pre-existing sub-1 collector-scope volume issue, not caused by
this fix).

Before -> after, healthcare fidelity vs sx-mathoverflow (matrix protocol, seed 42):

| statistic          | before (0.696) | after (0.754) |
|--------------------|---------------:|--------------:|
| FIDELITY (agg)     | 0.696          | **0.754**     |
| out_degree KS      | 0.587          | 0.538         |
| inter_event KS     | 0.457          | 0.296         |
| total_degree KS    | 0.237          | 0.172         |
| in_degree KS       | 0.152          | 0.066         |
| clustering (dist)  | 0.411 (0.003)  | 0.405 (0.010) |
| reciprocity (dist) | 0.364 (0.019)  | 0.335 (0.049) |
| repeat_edge (dist) | 0.339 (0.865)  | 0.256 (0.783) |
| mean_degree (dist) | 0.280 (72.6)   | 0.204 (61.7)  |
| burstiness (dist)  | 0.232 (0.224)  | 0.161 (0.365) |
| powerlaw_alpha     | 0.230          | 0.185         |
| memory_m (dist)    | 0.058          | 0.088         |

Target >=0.73 MET (0.754, robust plateau: pharmacy rate 10/12/14 give 0.754/0.754/
0.754; burst_p 0.25/0.35 give 0.754/0.755). Residual ceiling is HONEST and structural,
the SAME bipartite-vs-unipartite mismatch as transport: `clustering` (0.010 vs 0.415)
and `reciprocity` (0.049 vs 0.383) cannot move much — healthcare is a directed,
near-bipartite claims graph (provider->patient, pharmacy->patient, provider->drug; the
only same-type edge is provider->provider referral, which supplies the small nonzero
reciprocity), so it structurally has few triangles and little reciprocity, whereas the
proxy is a unipartite human Q&A net. `out_degree` is the third structural residual:
every patient and drug node has out-degree 0 (they only receive), so the out-degree
distribution has a large zero-mass the unipartite proxy lacks. These three account for
essentially the entire remaining gap. Determinism preserved: healthcare 1M seed 42
threads 1==8, edges/labels/ground_truth sha256 all match (edges 75d8d97f..., labels
557d14f8..., ground_truth 5a70ee95...). No regression: transport re-measured 0.743
(unchanged, its yaml untouched); all six domains generate and are thread-deterministic
(threads 1==8) with anomalies firing.

## Known hard limitation (not a quick fix; record, do not over-promise)

## P?-C2ST. Edges remain trivially separable from real

- Evidence: discriminative_score / C2ST ~ 0.9994-0.9997 vs CollegeMsg/email-Eu
  (docs/remeasure_fidelity.md), unchanged by the fix. AGORA reproduces low-order
  marginals, not the full joint distribution.
- This is a fundamental fidelity ceiling of a rule-driven simulator, not a bug.
  Closing it would require modelling higher-order joint structure. Keep it as a
  stated limitation; only attempt if the other P1/P2 items are done and there is
  time.

---

## Suggested order

1. P1-a homophily (highest reviewer-visible fidelity payoff; touches sim.rs +
   anomaly.rs; re-measure homophily + degree ratio + in/out together).
2. P2-b crypto difficulty-sign (correctness/interpretability; small, localized). DONE — one-sided damping.
3. P2-a priv_esc grounding (needs a citable rate; data task).
4. P3-a transport estimator (localized accuracy).
5. P1-b learned-contest finance nan (eval-harness only; quick).
6. C2ST only if time remains.

After each fix: cargo build + cargo test (must stay green), determinism hash
check, and a targeted re-measurement of the affected metric. Update this file
with the before/after number per item as it is fixed.
