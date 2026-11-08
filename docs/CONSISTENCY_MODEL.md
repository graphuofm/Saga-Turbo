# Block-bounded staleness: the consistency model for parallel agent-graph generation

This is the design spec for Path C (the DB-legit technical core) AND the draft of
the paper's core section. It turns the engine from "state is a write-only output"
(the current option A) into "state feeds back into behavior under a bounded-staleness
consistency model," making the parallel/distributed determinism a non-trivial,
formalized property rather than an embarrassingly-parallel accident.

## 0. Where we start (the current engine, precisely)

- Time is partitioned into blocks (currently 1-day windows). Block `b` = `[t_b, t_{b+1})`.
- Generation of block `b` is embarrassingly parallel per actor: each actor's events are
  drawn from an identity-keyed counter RNG stream `(seed, actor, block, behavior)` and
  read ONLY the immutable initial state `S_0` (`world.state`).
- Effects (add/sub/set on the src OR dst node's state var) are applied AFTER the whole
  block is generated, single-threaded, in a canonical total order (sort key
  `(t, src, dst, event_type)`), into a write-only accumulator `state`.
- `state` is never read back by generation; it is exported once at the end as final
  node attributes. => behavior does not depend on evolving state. This is option A.
- Distributed: partition by src (`node % N == k`); shard `k` emits only its nodes'
  out-edges. The union of edges is bit-identical (tested). BUT node state that depends
  on IN-edges (`!add_to_dst` on balance/cum_in/revenue/volume, e.g. finance.yaml:106-108)
  is WRONG under sharding: shard `k` only accumulates dst-effects from edges it
  generated (src on shard `k`), missing cross-shard in-edges. Untested (the sharded test
  compares edges only). This is a latent correctness bug to fix as part of C.

## 1. The model

State is versioned by block. Let `S_b` be the global state after block `b` is merged;
`S_0` is the initial state.

- READ: generation of block `b` reads the snapshot `S_{b-1}` (the state as of the end of
  the previous block), immutable for the whole of block `b`'s parallel generation.
- BEHAVIOR: an actor's events in block `b` are a deterministic function
  `beh(a, b) = f(S_{b-1}[reads(a)], R(seed,a,b,·))` of the snapshot restricted to the
  state `a` reads and `a`'s identity-keyed RNG stream. No actor reads state written
  within block `b`. No intra-block read-write dependency exists by construction.
- MERGE: block `b` produces an effect multiset `E_b` (one entry per (event, effect)).
  `S_b = apply(S_{b-1}, E_b)` where effects are applied in the canonical total order
  `key = (t, src, dst, event_type, effect_index)`. Add/Sub/Increment commute; Set is
  order-sensitive but the canonical order makes the result a deterministic function of
  `E_b`, not of production order.

STALENESS: an event at time `t` in block `b` reads state that is current as of `t_b`
(block start), so its read is stale by at most the block length `L = t_{b+1}-t_b`. `L`
is the staleness bound.

## 2. Theorem 1 (schedule-independence / determinism)

For a fixed seed and fixed block partition, the generated edges AND the final state are
identical for any number of threads and any parallel schedule.

Proof (induction on blocks). Base: `S_0` is fixed. Step: assume `S_{b-1}` is fixed and
schedule-independent. (i) Every actor's block-`b` events depend only on `S_{b-1}`
(immutable during block `b`) and its own counter-RNG stream, which is a pure function of
`(seed, actor, behavior, absolute-day, index)`, keyed on the absolute day rather than
the block (section 8), so the candidate draws are independent of L as well as of which
thread runs them or when.
So the edge multiset `E_b` and its edges are schedule-independent. (ii) `S_b =
apply(S_{b-1}, E_b)` is computed by sorting `E_b` by the canonical key and folding; the
sort is a total order (ties broken by `effect_index`), so `S_b` is a pure function of
`S_{b-1}` and the set `E_b`, independent of the order effects were produced in.
Therefore `S_b` and block-`b` edges are schedule-independent. QED.

Note: this is exactly why the CURRENT engine is bit-identical — but there it is trivial
because `reads(a) = {}` (no state read). Under state feedback, Theorem 1 is the property
that makes bounded-staleness parallel simulation deterministic, which is the contribution.

## 3. The block size is the tradeoff knob (and option A is one endpoint)

`L` interpolates a continuum:
- `L -> 0` (block = one event): every event reads all prior effects => sequential-exact
  semantics, maximum fidelity to the "true" sequential simulation, but a synchronization
  barrier per event => no parallelism.
- `L = span` (one block for the whole run): every read sees only `S_0` => this is the
  CURRENT engine (option A), maximum parallelism, maximum staleness.
- intermediate `L`: bounded staleness `<= L`, parallelism within a block, one barrier per
  block.

So the current engine is not "wrong," it is the `L = span` endpoint. The paper's knob is
`L`, and the headline experiment measures FIDELITY (vs the `L->0` sequential-exact
reference) against THROUGHPUT as `L` varies. This requires at least one behavior that
actually reads state (else fidelity is flat in `L`); see section 5.

## 4. Theorem 2 (distributed equivalence) + the communication model

Sharded run: partition nodes by `id % N`; machine `k` generates the out-edges of its
nodes and maintains state for its nodes.

Protocol: after generating block `b`, each machine computes the dst-effects of its
generated edges whose dst is owned by another machine, and sends each such delta to the
owner; it receives the dst-effects for its own nodes from the other machines; it then
merges its local src-effects and all received dst-effects in the canonical order to form
its slice of `S_b`. One exchange per block.

Claim: the sharded run's edges and per-node final state equal the single-machine run's,
provided `reads(a)` is restricted to `a`'s OWN node state (an actor conditions on its own
balance, not a counterparty's). Proof sketch: edges are src-partitioned and each actor's
events depend only on its own `S_{b-1}` slice (available locally after block `b-1`'s
exchange) and its RNG; so `E_b` is the disjoint union of the machines' local
`E_b^k`. The boundary exchange delivers to each owner exactly the dst-effects on its
nodes, so each machine's merge over the canonical order reproduces the global merge
restricted to its nodes. By Theorem 1 the per-node result is identical to single-machine.
QED.

Communication: one all-to-all of cross-shard dst-effect deltas per block. Volume = number
of cross-shard edges with a dst-effect, per block (measurable). "Zero communication" (the
current false claim) becomes "one bounded exchange per block," and node-state becomes
correct under sharding (fixing the latent bug). If `reads(a)` needs a counterparty's
state, the exchange must also ship read-set state; we restrict the model to own-state
reads to keep the exchange to written deltas only (a stated scope decision).

## 5. What must be built (state-conditioned behavior), minimally but really

Staleness only matters if behavior reads state. The grounded demonstrator: an OVERDRAFT
constraint in finance — an account's outgoing transfer is capped at (or suppressed when
below) its current balance. Under `L->0` this is exact (no overdrafts); under larger `L`
some transfers that should have been blocked slip through because the payer's balance is
read stale => a measurable divergence from sequential-exact. This makes fidelity a real,
monotone function of `L` and gives the knob teeth. Grounded: accounts cannot spend money
they do not have; this is a real constraint, not invented.

SECOND GUARDED DOMAIN (crypto, added after Phase 4). The same mechanism now grounds a
second physical constraint in a second domain, answering the "only one state-conditioned
behavior exists" limitation. In crypto, a wallet cannot transfer native value it does not
hold: the EVM enforces `balance >= value` at transaction validity, and the crypto rule
base already states the Phi predicate `balance_non_negative`. Wired YAML-only in
crates/saga-rules/domains/crypto.yaml on the two balance-spending events:
`tx` gets `guard: { src_var: balance, amount_attr: value }` and `swap` gets
`guard: { src_var: balance, amount_attr: amount_in }` (both events carry
`!sub_from_src { var: balance }` effects). `eoa.balance` already existed with a grounded
log-normal holdings distribution, so NO state var was added and no RNG stream moved.
Verification (crypto 1M edges, seed 42, defaults = span 90 d, threads 8):
- Guards OFF byte-compat: edges_00000.parquet sha256 9cb42c18..., nodes_eoa.parquet
  912d1fec..., labels_nodes.csv d8104bf5..., ground_truth.json 82d361e0..., all captured
  BEFORE the YAML change and byte-identical AFTER it (adding the `guard:` field touches
  neither Phase A draws nor init_state salting).
- Violation-vs-L mini-curve (guards ON), "emitted guarded events that overdrew on the
  stale read / all emitted guarded events":
    L = 0.001 d ->    366 / 246,942  (0.15%),  292,796 edges
    L = 1     d -> 23,153 / 232,899  (9.94%),  278,753 edges
    L = span(90) -> 671,059 / 866,933 (77.41%), 912,787 edges
  Monotone in L, near zero as L -> 0, maximal at L = span, exactly like finance.
- Threads 1 vs 8 at L = 1: edge and node CSVs byte-identical.
- Oracle: tests/oracle.rs `crypto_guards_match_block_oracle` cross-validates the engine
  against the independent sequential oracle BITWISE (kept-edge multiset and final node
  state) at L in {1 s, 1 d, span}, proves BOTH guarded event types fire, and re-pins
  threads 1 vs 8 in-process. The oracle needed zero changes: it was already
  domain-generic, which is the point.
- The unguarded-domain no-op test now covers all four remaining domains (cyber,
  transport, ecommerce, healthcare): --guards is bit-identical to guards-off there.
The mechanism is therefore demonstrably generic: two domains, two grounded constraints,
one YAML field each, no engine change.

## 6. Implementation plan (ordered; determinism re-verified after each step)

Phase 1 (single-machine core):
1. Thread the current-block snapshot `&state` into `gen_chunk`/`emit` (it already holds
   `S_{b-1}` at the top of block `b`). Read-only during parallel gen.
2. Add a state-read + guard to the rule language and wire the overdraft guard in
   finance.yaml (suppress/cap transfer when balance < amount). Keep it grounded.
3. Make block length `L` a parameter (currently hardcoded 1-day windows); allow sub-day
   and multi-day blocks. Determinism must hold for any `L`.
4. Re-verify Theorem 1 empirically: bit-identical across threads for several `L`.

Phase 2 (distributed):
5. Implement the per-block dst-effect exchange in the sharded path so per-node state is
   correct; add a test that sharded node-state == single-machine node-state (the missing
   test). Measure communication volume per block.

Phase 3 (experiments, all re-measured on the new semantics):
6. Block-size knob: fidelity (vs `L->0` sequential-exact) and throughput vs `L`.
7. Re-run ALL existing tables on the new engine (numbers change because behavior now
   conditions on state).
8. Distributed: node-state bit-identical + communication cost.

## 8. CRITICAL: decouple the RNG stream from the block size L (or the knob is confounded)

The block size L must affect the output ONLY through state-read staleness, not through
re-keying the RNG. Today the actor RNG is keyed on the block index
(`(window<<8)|behavior`); if L changed the block index, changing L would reshuffle every
RNG draw and the "fidelity vs L" curve would be dominated by RNG churn, not staleness.

Design: separate two granularities.
- RNG granularity (FIXED, L-independent): keep the actor's candidate events (arrival
  times, base amounts, counterparties) drawn from a fixed sub-stream keyed on
  `(seed, actor, behavior, absolute-day)` (or a fixed fine bucket), independent of L. The
  candidate event set is thus identical for all L.
- Staleness/merge granularity (= L, the knob): an event at time t reads the state
  snapshot as of `floor(t / L) * L` (the start of its L-block); effects are merged at
  L-boundaries in canonical order. Guards (overdraft) are evaluated against that snapshot.

Cleanest realization = two-pass per L-block: (1) draw candidate events L-independently
(as today), (2) process time-sorted in L-blocks, each reading the previous L-block's
end snapshot, apply guards (which may suppress/cap an event), then merge the block's
guarded effects. Output edges therefore depend on L only through the guard decisions.
- L -> 0: every event reads all prior effects => sequential-exact reference.
- L = span: every event reads S_0 => the current engine.
Determinism (Theorem 1) still holds per fixed L: candidate draws are schedule-independent
(fixed keying), and the per-L-block guard+merge is over the canonical order.

Implication for Phase 1: block length L is NOT just "change the window seconds"; it is a
separate staleness parameter layered over the fixed daily RNG sub-streams. Implement the
two-pass so the candidate stream is invariant to L; verify by generating with state
guards OFF at two different L and getting bit-identical edges (proves L touches output
only via guards).

## 7. Risks / honest notes

- If we keep behavior reads to own-state only, the model is clean and the exchange is
  written-deltas only. Counterparty-state reads would need read-set shipping (scope out
  for v1, note as extension).
- Set effects are order-sensitive; determinism relies on the canonical sort, which we
  already have. Keep effects add/sub dominant; document Set handling.
- This changes generated data => every published number is re-measured. Expected.
- Distributed v1 may be a single-process emulation of the sharded protocol that PROVES
  node-state equivalence and MEASURES communication volume analytically+empirically, with
  a true multi-process deployment as a follow-on. State this scope honestly.

## 9. Phase 1 implementation notes (what was built)

Landed on the single-machine core (crates/saga-core), the two-pass realization from
section 8. No commit; paper/ and the frozen trees untouched.

WHAT WAS BUILT
- Two-pass `sim::run` (crates/saga-core/src/sim.rs):
  - Phase A (parallel, unchanged draws): generate CANDIDATE events per absolute DAY,
    exactly as the old engine did, keyed on `(seed, actor, day, behavior)`. State is NOT
    read here. Each day is merge-sorted + granularity-rounded and appended into one
    globally time-sorted candidate batch.
  - Phase B (single-threaded): sweep the candidates in L-blocks. `block_of(t) =
    floor((t - epoch) / L_s)`. For the whole of a block, `state` is the frozen block-start
    snapshot; each event's guard reads it; kept events are emitted and their effects merged
    into `state` at the block boundary (canonical order — the slice is already sorted).
- RNG/L decoupling (section 8, the critical part): the candidate stream is keyed on the
  day, never on L, so it is byte-for-byte identical for every L. L enters ONLY through the
  Phase-B guard decision. Verified empirically (see below).
- Block length L as a parameter: `GenParams::block_len_days` (CLI `--block-len-days`),
  in days, allows sub-day and multi-day L. `L >= span` is a single block = option A (every
  read sees S_0). Default 1.0.
- Grounded overdraft guard: `EventType.guard { src_var, amount_attr }` in the rule schema
  (crates/saga-rules), compiled to `CompiledGuard` (world.rs), wired in finance.yaml on
  `transfer` (suppress when payer balance < amount). Own-state read only (src's balance),
  keeping the section-4 distributed model exchange to written deltas. Toggle:
  `GenParams::guards_enabled` (CLI `--guards`); default OFF, so pre-Phase-1 output is
  byte-identical.
- Mechanism telemetry: `GenSummary.overdraft_guarded / overdraft_violations`. A violation
  is an emitted transfer that passed the STALE guard yet overdrew the TRUE up-to-date
  balance (tracked by a never-stale shadow ledger, i.e. the L->0 reference, updated per
  event during Phase B; measurement only, active when guards are on).

VERIFICATION (finance, seed 42, 1M edges, span 14d, anomalies off; crates/saga-core/
tests/phase1.rs and real CLI CSV runs)
- Determinism per fixed L (threads 1 vs 8, bit-identical edges AND node-state):
    L=0.25  edges 0x2f02ddee15b01c69  nodes 0x44b2d3f6b8973a0d
    L=1     edges 0x05b33f6e13e2c623  nodes 0xba7bb1083f390371
    L=4     edges 0xe6f12f0f09472552  nodes 0x305e51c67d4f9288
  CLI CSV, L=1: threads 1 and 8 both give edges sha256 d01210c589a8d794, nodes
  56fc1987d20f94d9.
- Decoupling (guards OFF, different L -> bit-identical edges): L in {0.5, 2, span} all
  hash 0x09450b65017febbd (in-process); CLI CSV L=0.5 == L=2 (70d5ca17fcbc6f36). With
  guards ON the two L differ (b4f9...  vs 95b5...), so the knob has teeth.
- Overdraft-vs-L curve (guard ON), violation rate: L=0.001 -> 0.23%, 0.25 -> 9.43%,
  1 -> 14.68%, 4 -> 24.39%, span(14) -> 45.05%. Monotone in L, -> 0 as L -> 0
  (sequential-exact), maximal at L=span. Confirms L=span reproduces option A (guard reads
  only S_0).

DEVIATIONS / DEFERRED
- Deviation from section 6 step 1 wording ("thread `&state` into gen_chunk/emit"): the
  state read lives in the Phase-B guard, NOT inside `emit`. This is deliberate and is the
  section-8 two-pass: putting the read in per-day generation would tie the snapshot
  granularity to the day and re-couple RNG to L. Candidate draws stay L-independent; the
  only state-conditioned decision (the guard) reads the per-L-block snapshot.
- Memory: the first two-pass landing materialized the whole run's candidates in RAM
  before the sweep (the old engine streamed per day). That was a measured regression at
  scale (24.7 s / 8.19 GB at 1e8 vs the published 17.4 s / 1.35 GB; extrapolated ~80 GB
  at 1e9, an OOM on the 62 GB host) and is FIXED by the streaming rewrite in section 11:
  Phase A now yields one day at a time and Phase B consumes complete L-blocks with an
  O(max(L, 1 day)) pending buffer. The final node state still equals the per-event-folded
  ledger, so exported state is L-correct.
- Guard expressiveness is intentionally minimal (one own-state `>=` suppress). Capping (vs
  suppress) and richer predicates are future work; suppress is enough to make fidelity a
  monotone function of L.
- Distributed per-block dst-effect exchange (section 4 / Phase 2) and re-measuring the
  paper tables (Phase 3) are NOT done here, as instructed.

## 10. Phase 2 implementation notes (distributed equivalence)

Landed on crates/saga-core (sim.rs) as a SINGLE-PROCESS EMULATION of the N-shard
protocol from section 4, per the section-7 scope decision. No commit; paper/ and the
frozen trees untouched. A real multi-process deployment (MPI/sockets) is the documented
follow-on and is NOT built here.

SCOPE (honest). The emulation simulates N virtual shards in ONE process. It PROVES that
the sharded run's edges AND per-node final state equal the single-machine whole-graph
run's (logical/value equality, not raw parquet bytes), and MEASURES the per-block
cross-shard exchange volume. It does not deploy real processes, and it does not change
the real multi-process `run` sharded path (which still needs a cross-process transport —
deferred); the emulation IS the protocol proof.

WHAT WAS BUILT
- Refactor (no behavior change): Phase A (candidate draw) and the world+plan setup were
  factored out of `sim::run` into `phase_a` and `prepare`, so the single-machine engine
  and the emulation observe the identical `World`, `rate_scale` rebalance and marked sets.
  The single-machine path and all Phase-1 properties are unchanged (verified: phase1.rs
  green, including threads-1-vs-8 determinism).
- The N-shard sweep (`emulate_shards`): candidates are drawn ONCE for the whole graph and
  each event is routed to its owning shard by `src % N`. This is exact because Phase A
  partitions strictly by src, so the whole-graph candidate array split by `src % N` IS
  each shard's candidate set (the invariant `sharded_union_equals_whole_graph` already
  proves). Each shard holds its own state copy but reads/writes only the cells of the
  nodes it owns. Per block: every guard reads its shard's OWN slice (own-state read, §4);
  at the block boundary every effect delta is delivered to the OWNER of the touched node
  (src-effects local; dst-effects possibly cross-shard) and folded in canonical order.
  So each owner's slice is complete before the next block — Add/Sub/Increment commute and
  Set keeps the canonical last-writer, so the per-shard merge reproduces the global merge
  restricted to that shard's nodes (Theorem 2). N == 1 reduces to the whole-graph sweep,
  and it is cross-checked bit-for-bit against the real `generate` engine export.
- Communication measurement (`ShardCommStats`): counts, per block and total, the
  cross-shard dst-effect deltas the exchange must ship (a dst-effect whose dst is owned by
  a different shard than the generating src, excluding NaN no-ops), and bytes at
  `SHARD_DELTA_BYTES = 18` (node id u64 + var u8 + kind u8 + value f64; entity derivable
  from the id — a conservative fixed-width upper bound).
- The old (no-exchange) path is emulated too (`exchange = false`: the generating shard
  keeps every effect on its own slice, ships nothing), to demonstrate the bug the exchange
  fixes.
- Public measurement hook `sim::shard_equivalence(params, N) -> ShardEquivalence` returns
  the whole-graph reference, the sharded-with-exchange result, the no-exchange result, and
  the comm stats. Test: crates/saga-core/tests/phase2.rs.

VERIFICATION (finance, seed 42, guards ON, span 14d, anomalies off)
- cargo build --release clean; cargo test --release all green: saga-core 33 tests
  (+1 ignored 1M-scale), whole workspace green. New test `sharded_state_equals_whole_graph`
  (N in {2,4}) passes: edge multiset AND every per-node state var equal the whole-graph
  run's, and the emulation's N==1 reference equals the real `generate` export bit-for-bit.
- Sharded == whole-graph (logical), edges AND node-state:
    200k edges: N=2 and N=4 both -> 176,299 edges identical, all state cells identical.
    1M   edges: N=2 and N=4 both -> 846,982 edges identical, all state cells identical.
- Per-block cross-shard dst-effect exchange volume (the "one bounded exchange per block"
  cost of §4), 14 blocks:
    200k: N=2 total 127,660 deltas (2.30 MB), mean 9,119/block, peak 12,236/block,
          shipped fraction 0.185 of all effect deltas;
          N=4 total 191,961 deltas (3.46 MB), mean 13,712/block, peak 18,593/block,
          shipped fraction 0.277.
    1M:   N=2 total 603,614 deltas (10.87 MB), mean 43,115/block, peak 57,645/block;
          N=4 total 910,615 deltas (16.39 MB), mean 65,044/block, peak 87,000/block.
  Volume grows with N (more shards => more cross-shard in-edges) and is 0 for N=1
  (`exchange_volume_grows_with_shards` asserts strict monotonicity for N in {2,4,8}).
- Exchange is NECESSARY (old path diverges): WITHOUT the exchange, node state is wrong and
  the stale guard even changes which edges are emitted:
    200k: N=2 -> 15,816 state cells wrong, edges 174,890 vs 176,299;
          N=4 -> 20,441 state cells wrong, edges 173,987 vs 176,299.
    1M:   N=2 -> 46,546 state cells wrong; N=4 -> 58,197 state cells wrong; edges diverge.
- Determinism (Phase 1 property preserved through the refactor): whole-graph threads 1 vs
  8 remain bit-identical for edges AND node state (phase1.rs `determinism_across_threads_
  per_l`, L in {0.25, 1, 4}).

DEFERRED
- Real multi-process deployment (a cross-process transport for the per-block exchange) and
  re-measuring the paper's tables on the sharded semantics (Phase 3) are NOT done here.
  [SUPERSEDED: the real multi-process exchange is BUILT and verified in Phase 4, §12.]
- The real `run` sharded path (`--shard-count`) still emits per-node state without the
  exchange (the demonstrated bug); it is left for the multi-process deployment, since a
  single process cannot ship deltas between separate shard processes. The emulation is the
  equivalence proof that the exchange makes that path correct.
  [SUPERSEDED by §12: with `--exchange-dir` the real path runs the exchange.]
- RAIL (added with the section-11 streaming fix): because the real sharded path has no
  exchange, `guards_enabled && shard_count > 1` would read state missing other shards'
  dst-effects and silently produce wrong edges AND wrong node state. Both the engine
  (saga-core sim::run) and the CLI (generate.rs, before any output is created) now
  REFUSE that combination with a hard error pointing at sim::shard_equivalence as the
  verified guarded-sharded path. Guards-off sharding stays allowed (edge output proven
  correct; the node-state export caveat above still documented).
  [UPDATED in §12: the rail now fires only WITHOUT `--exchange-dir`, and its message
  names `--exchange-dir` as the fix.]

## 11. Streaming Phase A/B (section 9/10 addendum): the memory-regression fix

The first two-pass landing (section 9) generated the ENTIRE run's candidates in RAM
(Phase A) before sweeping them (Phase B). Measured cost at scale, finance seed 42,
guards off, defaults: 1e8 edges = 24.7 s wall / 8.19 GB peak RSS vs the published
fixed engine's 17.4 s / 1.35 GB; extrapolated 1e9 needs ~80 GB and OOMs a 62 GB
host, breaking the billion-edge claim. Fixed on crates/saga-core/src/sim.rs by
streaming both phases; no commit, paper/ and frozen trees untouched.

DESIGN (what changed, semantics unchanged)
- Phase A is now per-day: `phase_a_day` draws day w's candidates (same draws, same
  keying `(seed, actor, behavior, absolute-day)`, so the RNG/L decoupling of section
  8 is untouched), merge-sorts and granularity-rounds the day, and hands it to Phase
  B immediately. The whole-run `phase_a` survives ONLY as the Phase-2 emulation's
  input builder (`shard_equivalence` must route one shared candidate array to N
  virtual shards); it is documented as test-scale-only and is not on the production
  path.
- Phase B, guards OFF (the default): the sweep provably reads no state (output is
  L-independent), so each day is counted, its effects folded into the export ledger
  in canonical time order (identical folds under any block partition), written to
  the sink, and freed. This is the pre-Phase-1 fully streaming pipeline and its
  memory profile, and the output is byte-identical (verified below).
- Phase B, guards ON: candidates append to a pending buffer; after day w the sweep
  consumes every COMPLETE L-block, i.e. blocks strictly before the block of the
  earliest timestamp any future day can emit (next-day start, rounded down to the
  output granularity). Each completed block reads the frozen S_{b-1} snapshot for
  its guards, writes kept events, merges its effects at the boundary in canonical
  order, and the consumed prefix is drained in place. Peak buffering is
  O(max(L, 1 day)) of candidates plus one day in flight. The sweep stops only at
  block-run boundaries, so the streamed run decomposition and processing order are
  identical to the old whole-array sweep, event for event.
- Degenerate endpoint, stated honestly: for L >= span the single block completes
  only when the run ends, so guards-on L = span buffers the whole run's candidates.
  That is inherent to the semantics (every guard reads S_0, but nothing can be
  emitted-and-merged before the one block closes), not an implementation limit; for
  any L up to a few days, memory is bounded and flat in run length.
- Correctness rail (silent-wrongness fix, see section 10 RAIL): engine and CLI now
  hard-refuse `--guards` with `--shard-count > 1`; guards-off sharding unchanged.

VERIFICATION (all real runs, seed 42; graphs deleted after measurement)
- cargo build --release clean; cargo test --release ALL green: 72 passed, 0 failed,
  2 ignored (the 1M-scale phase2 equivalence test and the phase3_comm measurement
  hook; the phase2 ignored test was also run explicitly and passes with the exact
  section-10 numbers: 846,982 edges, N=2 603,614 deltas / N=4 910,615 deltas,
  no-exchange divergence 46,546 / 58,197 cells). phase1 / phase2 / phase3_comm
  semantics did not drift.
- Byte-compat with the published fixed engine (guards off, defaults): finance 1M
  seed 42 threads 8 parquet, edges_00000.parquet sha256 5893b7aa..., labels_nodes.csv
  334ba9ab..., ground_truth.json a231a0d2... - all identical to the recorded
  fixed-engine hashes.
- Determinism per fixed L (phase1.rs, threads 1 vs 8, guards on, edges AND node
  state): hashes unchanged from the section-9 record, L=0.25 edges
  0x2f02ddee15b01c69 / nodes 0x44b2d3f6b8973a0d; L=1 0x05b33f6e13e2c623 /
  0xba7bb1083f390371; L=4 0xe6f12f0f09472552 / 0x305e51c67d4f9288. Guards-off
  L-independence hash 0x09450b65017febbd, also unchanged.
- Sweep semantics unchanged (guards on, preset tiny, finance, no-anomalies, CSV):
  overdraft violations at L = 0.001 / 1 / 30(span) are 1,214 / 66,205 / 291,420
  with edges 878,315 / 821,880 / 961,453 - exactly the docs/remeasure_blocksize.md
  record.
- Scale, re-measured on this host (32 cores, 62 GB; 1-min loadavg 4.0-5.9 at run
  start, threshold 8):

  | run (finance, seed 42, defaults) | regressed two-pass | streaming (this fix) | published old engine |
  |---|---|---|---|
  | 1e8, guards off | 24.7 s / 8.19 GB | 17.2 s / 1.39 GB | 17.4 s / 1.35 GB |
  | 1e9, guards off | ~80 GB extrapolated, OOM | 182.1 s / 7.92 GB, 997,703,683 edges | ~183 s / ~7.4 GB |
  | 1e8, guards ON, L=1 | (unmeasured, >= 8 GB) | 16.4 s / 1.23 GB | n/a (no guards) |

  The guards-on 1e8 row is the new datapoint: the cost of consistency at scale is
  bounded-memory and roughly free in wall time at L = 1 day (16.4 s; 75,208,475
  edges emitted from 99.8M candidates, 9,021,302 of 28,787,432 guarded transfers
  violating on the stale read, 31.3%, consistent with the small-scale L=1 curve at
  this 90-day span). Peak RSS is BELOW the guards-off run because the guard
  suppresses ~25% of writes; the pending buffer holds ~2 days of candidates.

PARQUET SMALL-L BOUNDARY COST (addendum, fixed after Phase 4). The verification
matrix found the parquet sink paying +23% wall at L = 0.01 vs L = 1 (finance 1e7
guards on) while CSV moved far less. Root cause, established by measurement: NOT
a row-group flush per block sweep. The ArrowWriter already accumulates delivered
batches into 1M-row groups regardless of batch size (files written at L = 0.01
had 9 row groups of ~1M rows, same as L = 1). The real cost is the fixed
per-encode-call overhead (RecordBatch assembly, per-batch dictionary builders,
writer dispatch) times 9,000 tiny per-block batches: the guarded sweep emits one
batch per L-block (L = 0.01 d over 90 d = 9,000 blocks of ~1k rows at 1e7), and
that fixed cost makes the writer thread the bottleneck.

Fix (crates/saga-io/src/threaded.rs, wired in saga-cli generate.rs): opt-in
coalescing in ThreadedSink accumulates delivered rows into 128k-row chunks on
the GENERATION side of the writer channel (the same one copy per row the plain
path already pays for the channel clone) and sends each full chunk as one
message, so the writer thread encodes ~70 large batches instead of 9,000 tiny
ones and each chunk's encode overlaps generation of the next. Block boundaries
are a semantics concept, not an IO concept: the logical row stream (rows and
their order) is unchanged, only encode-call grouping changes. Two variants were
measured WORSE and rejected, recorded here so they are not retried: coalescing
downstream of the channel (inside ParquetSink) keeps 9,000 tiny messages and
turns the encode into bursts that stall the depth-3 channel (3.9 s vs the 2.7 s
unfixed baseline at L = 0.01); a 1M-row chunk target additionally lumps the
final encode after generation ends (tail latency at every L). Gating: enabled
ONLY for guarded parquet runs. The guards-off default path is byte-frozen and
stays on the historical write path bit for bit; CSV/GraphML are untouched.

Verified: finance AND crypto guards-off 1M byte-compat hashes unchanged
(5893b7aa... and 9cb42c18...); guards-on parquet output logically identical
before vs after the fix at L in {0.01, 1} (pyarrow table equality over the edge
table and every node table, finance 1M, telemetry identical); all 94 tests
green; tiny-preset sweep spot-points exact (violations 1,214 / 66,205 / 291,420
at L = 0.001 / 1 / 30).

Re-measured (finance 1e7 guards ON, parquet, threads 8, 3 reps, same session,
background loadavg ~3.4 of 32 cores):
    L = 0.01: 2.70 s before -> 2.52 s after
    L = 0.1:  2.41 s before -> 2.42 s after
    L = 1:    2.27 s before -> 2.30 s after (day batches were already ~95k rows)
  Small-L penalty vs L = 1: +19% -> +9.6% wall. Per WRITTEN row (L = 0.01 keeps
  8.61M rows vs 8.29M at L = 1, because guard decisions differ with L) the
  penalty drops +15% -> +5.5%, i.e. into the target ~2-5% band.

Honest notes: the residual small-L cost is generation-side per-block sweep work
shared by all formats, not parquet IO. On this host, in the same session, CSV
(uncoalesced) showed +25% at L = 0.01 (3.63 s vs 2.90 s at L = 1); its write
path clones the per-column dictionaries on every delivered batch, so the
matrix's "~2% CSV" did not reproduce here and parquet is now the CHEAPER format
at small L. Extending coalescing to the guarded CSV path (and fixing the CSV
per-batch dictionary clone) is a follow-up, left untouched because the frozen
guards-off CSV path shares that code.

## 12. Phase 4: real multi-process exchange (the section-4 protocol, deployed)

The per-block dst-effect exchange of section 4, previously proven only by the
single-process emulation (section 10), is now implemented on the REAL sharded
run path: N separate `agora` processes (`--shard-index k --shard-count N`,
identical seed/params) produce, together, exactly the single-machine guarded
run - correct edges per shard AND correct, complete per-node state - matching
both the emulation and the whole-graph reference bit for bit. Landed on
crates/saga-core (exchange.rs new, sim.rs, api.rs) and crates/saga-cli
(generate.rs). No commit; paper/ and the frozen trees untouched.

THE PROTOCOL (filesystem-based, cluster-friendly: a shared directory, no
sockets/MPI)
- CLI: `--exchange-dir <path>` (plus `--exchange-timeout-s`, default 300).
  Required with `--guards --shard-count > 1`; the section-10 rail still fires
  without it and its message now names `--exchange-dir` as the fix. The dir
  must be fresh per run and shared by all N processes (e.g. NFS); it is
  refused with `--shard-count 1` (no peers to exchange with).
- Per L-block `b`, shard `k`: (1) generates/guards its block-`b` events
  against its own `S_{b-1}` slice; (2) writes its outbound cross-shard
  dst-effect deltas to `<dir>/b<b>_from<k>_to<j>.bin`, one file per peer,
  possibly empty, written to a hidden temp name and atomically RENAMED so a
  partial file is never observable; (3) polls (2 ms sleep, bounded by the
  timeout) until `b<b>_from<j>_to<k>.bin` exists for every peer, reads each,
  and DELETES it (each file has exactly one reader, so consumed deltas never
  accumulate; a clean run leaves the dir empty and removes it); (4) merges its
  local owned effects and the received deltas in the canonical order to form
  its `S_b` slice, then proceeds. This is one filesystem barrier per L-block,
  and the block SCHEDULE is canonical - derived from params only, empty blocks
  included (a shard with no local events in a block still owes its peers empty
  files and still receives their deltas), so all shards run the same rounds in
  lockstep. A missing/crashed peer becomes a clear timeout error naming the
  missing file and peer, never a hang.
- Wire format: fixed 35-byte little-endian records, `t i64 + src u64 +
  dst u64 + event_type u16 + effect_index u8 + amount f64`. The receiver
  reconstructs the effect (entity, var, kind, src/dst target) from
  `(event_type, effect_index)` in its own compiled world, so only the
  canonical key and the resolved operand travel; non-finite Add/Sub no-ops are
  never shipped. Foreign/stale files fail loudly (record-size check, event and
  effect range checks, ownership check on every received delta).

CANONICAL MERGE ORDER, refined (the correctness core). Section 1's key
`(t, src, dst, event_type, effect_index)` is not total: two DIFFERENT events
can share it (same actor, same second, same counterparty, same type - 330 of
998,371 rows in the frozen finance-1M reference share a full key). The
engine's day-sort is deliberately unstable (frozen bytes), so the mutual order
of such ties is a sort ARTIFACT of the surrounding array - unreproducible from
a shard's smaller array, and float folds do not associate, so per-cell fold
order must not depend on it. Phase 4 therefore appends the delta's CONTENT to
the key: ties fold in `(effect_index, amount-bits)` order, making the fold a
pure function of the kept delta multiset (full-content duplicates are
interchangeable; Set operands are per-(event_type, effect) constants, so
last-writer stays deterministic). This canonical fold (`sim::fold_canonical`)
is now used by EVERY state fold - guards-off streaming, the guarded block
merge, the emulation, and the exchange merge (which sorts local + received
deltas by the full key; keys never collide across shards because equal keys
share their src and hence their generating shard) - and the test oracle
(tests/common) implements it independently. Relatedly, granularity rounding
now happens BEFORE the day-sort, so the stream is ordered by its FINAL
timestamps at any granularity (identity for the default 1 s granularity:
byte-identical stream). Consequence, stated honestly: guards-on node-state
hashes recorded in section 9 drift at the ulp level where full-key ties
reorder (the section-9 determinism PROPERTIES all still hold and phase1.rs is
green); the guards-off single-machine edge output is byte-identical (frozen
hash verified below), and the guards-on sweep spot-points are unchanged.

NODE EXPORT, complete. With the exchange, each shard applies exactly the
deltas that target its OWNED nodes (local src-effects + local and received
dst-effects), so its exported node rows are complete and correct - including
guards-off runs that opt into `--exchange-dir`, which fixes the section-0
caveat (cross-shard in-edge effects missing from sharded node state). The
plain guards-off no-exchange mode is unchanged (edge-only sharding, caveat
still documented). Telemetry: `GenSummary.exchange_rounds / deltas / bytes
(sent+received)`; note the per-shard overdraft-VIOLATION telemetry under
sharding counts against a ledger whose cross-shard in-edges arrive at block
granularity, so shard sums approximate the single-machine count (1M run:
20,533 (N=2) / 20,575 (N=4) vs 20,437 single; the guarded-decision count is
exact: 421,982 in all three). Correctness of edges and state never depends on
that telemetry.

VERIFICATION (all real runs, seed 42; outputs and exchange dirs deleted)
- cargo build --release clean; cargo test --release ALL green: 93 passed
  (the 86 existing, none regressed, + 7 new), 2 ignored. The ignored 1M
  phase2 emulation test was run explicitly and reproduces the section-10
  numbers EXACTLY (846,982 edges; N=2 603,614 deltas / N=4 910,615; without
  exchange 46,546 / 58,197 cells wrong).
- Byte-compat: finance 1M seed 42 threads 8 defaults parquet:
  edges_00000.parquet sha256 5893b7aa..., labels_nodes.csv 334ba9ab...,
  ground_truth.json a231a0d2... - identical to the recorded fixed-engine
  hashes. Sweep spot-points (preset tiny, guards on): L=0.001/1/30 violations
  1,214 / 66,205 / 291,420 with edges 878,315 / 821,880 / 961,453 - exact.
- Real-process equivalence, guards ON, finance 20k nodes / 200k edges /
  span 14 / L=1, N in {2, 4}:
  * engine level (crates/saga-core/tests/phase4.rs): N concurrent REAL
    `sim::run` shard instances (one thread per shard, own rayon pools, shared
    exchange dir - the exact code path of N processes): the union of shard
    edges equals the single-machine guarded run's full-row multiset (attrs by
    f64 bits); every shard's exported node state equals the single-machine
    slice BITWISE and the slices tile the graph; both equal
    `sim::shard_equivalence`'s emulation (edges and state); the exchange dir
    is left empty. Also: guards-off + exchange node state == single-machine
    guards-off, bitwise (the caveat fix).
  * process level (crates/saga-cli/tests/exchange_cli.rs): N REAL `agora`
    subprocesses of the built binary, CSV output: shard edge-CSV union ==
    single-machine edges line-for-line as a multiset; each shard's nodes.csv
    == the single run's owned-id rows line for line; parsed edges and
    rendered state values == the emulation.
- Timeout: a lone shard of an N=2 run fails in ~2 s (tiny timeout) with
  "timed out ... waiting for the block-<b> delta file ... from peer shard 1",
  at both the engine and subprocess level; the rail without `--exchange-dir`
  errors (before any output is created) naming the flag.

MEASUREMENT (guards ON, finance 1M seed 42 defaults = 100k nodes, span 90 d,
L = 1 d => 90 barrier rounds; threads 8 per process, quiet host, loadavg
~3.6 < 8, local disk exchange dir; single-machine guarded baseline 0.42-0.44 s
wall, 887,156 edges):

  | N | per-shard wall | makespan | rounds | cross-shard deltas | delta bytes | union edges |
  |---|---|---|---|---|---|---|
  | 2 | 0.52 / 0.53 s | 0.56 s | 90 | 643,748 | 22.53 MB | 887,156 (== single) |
  | 4 | 0.52 s (all) | 0.55 s | 90 | 970,704 | 33.97 MB | 887,156 (== single) |

  Overhead vs the single-machine guarded run: ~0.10-0.12 s makespan (~25%),
  i.e. ~1.2 ms per barrier round (write + poll + read + merge of ~250 KB
  mean per round at N=2). At this 0.4-second scale the run is barrier-latency
  dominated and sharding buys no wall time - the point here is CORRECT
  distributed semantics with a measured, bounded exchange (volume grows with
  N exactly as the section-10 emulation predicted: the same 22.5/34.0 MB
  totals at ~35 B/delta wire size vs the emulation's 18 B/delta accounting
  bound, which excluded the key fields). On runs long enough for per-block
  compute to dominate the ~ms barrier, the per-shard Phase A/B work (the
  17 s/1e8 scale of section 11) partitions by N while the exchange stays
  proportional to cross-shard dst-effects only.

HONEST NOTES
- The exchange dir must be FRESH per run; leftover files from a different
  run/config are detected only when they are malformed or target unowned
  nodes (loud errors), not when they happen to parse.
- L >= span degenerates to one barrier at the end of the run (inherent to the
  semantics, as in section 11); tiny L multiplies barrier rounds (L=0.001 d at
  span 30 is 30,000 rounds - correct but filesystem-latency bound).
- For granularity > 1 s the output row ORDER within a rounded second changed
  (round-before-sort); row SETS are identical, granularity-1 output is
  byte-identical, and no frozen artifact uses granularity > 1.

## 13. Guards in all six domains

Sections 5 and 12 left four domains (cyber, ecommerce, healthcare, transport)
with no state-conditioned behavior, so for them `--guards` was a no-op and the
block length L had no teeth. Each now carries one enforced, grounded guard of
the existing form `guard: { src_var, amount_attr }` (suppress the event when the
SRC's state var, read against the block-start snapshot, is below the event's
amount attribute). The engine, the guard vocabulary (rulebase.rs `EventGuard`,
world.rs `compile_guard`, sim.rs sweep and shard paths), the exchange protocol
and the test oracle (tests/common) are UNCHANGED: every addition is YAML. The
src-side-only form was kept deliberately: a dst-side guard would read state
that, under section 12 sharding, lives on another shard and is only current at
block granularity, so it could not be evaluated locally; src-side guards read
the owning shard's own slice and need no protocol change.

NONE of the four domains had a spendable (decreasing) state var: all their
state vars were monotone counters (bytes_out, claim_count, trips, ...), so the
resource each constraint protects had to be ADDED as a state var with a
`sub_from_src` effect. Guards-off byte-compat is preserved by construction
(section 9: `init_state` salts each state var by `1000 + var index`, so a var
APPENDED LAST only adds fresh NodeInit draws; Phase A reads no state; effects
fold into node state only) and was verified on all six domains below. The
`nodes_<entity>.parquet` files of the four domains gain one column (the new
var's final value); edges, labels and ground truth are byte-identical.

Per domain (the declared Phi predicate was added to each `constraints:` list;
none of the four was previously declared, unlike finance/crypto whose
`balance_non_negative` pre-existed):

| domain | guarded event | new state var (init) | effects | Phi predicate | grounding |
|---|---|---|---|---|---|
| cyber | `flow` (host->host), amount `fwd_bytes` | `host.egress_allowance`, log-normal mu 10.0 sigma 1.5 (median ~22 KB; measured span egress per host: median 20 KB, p90 140 KB) | `sub_from_src egress_allowance <- fwd_bytes` | `egress_allowance_non_negative` | a host cannot push more egress bytes than its metered allowance holds (egress filtering / data-cap policy, NIST SP 800-41 egress controls; the YAML's existing `egress_known_good` predicate cites NIST 800-61 / T1041 for the same egress view). Honest: this is a POLICY quota, not a physical law |
| ecommerce | `PURCHASED` (user->product), amount `amount` | `user.available_funds`, log-normal mu 4.7 sigma 1.0 (median ~$110; measured 90-day spend per buyer: median $108, p90 $408) | `sub_from_src available_funds <- amount` on PURCHASED; `add_to_src available_funds <- amount` on RETURNED (refund) | `available_funds_non_negative` | payment authorization: a purchase is charged only if the buyer's available funds cover the amount (stored value / authorized balance); refunds credit back. The same "cannot spend what you do not hold" grounding as finance/crypto |
| healthcare | `DISPENSED` (pharmacy->patient), amount `days_supply` | `pharmacy.stock_days`, log-normal mu 6.3 sigma 0.6 (median ~545 patient-days; measured span demand per pharmacy: median 520, p90 1170) | `sub_from_src stock_days <- days_supply` | `stock_non_negative` | a pharmacy cannot dispense stock it does not hold (perpetual inventory; 21 CFR 1304 record keeping for controlled substances). Honest: inventory is measured in patient-days of supply because `DISPENSED` carries `days_supply`, not a unit count, and the domain has no replenishment event, so stock only decreases over the span |
| transport | `trip` (vehicle->zone), amount `distance_m` | `vehicle.range_m`, log-normal mu 11.0 sigma 1.0 (median ~60 km remaining at t0; measured span trip distance per vehicle: median 11 km, p90 119 km, p99 377 km) | `sub_from_src range_m <- distance_m` | `range_non_negative` | a vehicle cannot complete a trip longer than the fuel/battery range it holds (physical). Honest: the domain has no refuel event, so range only decreases; `traversal` carries no distance and is not guarded |

The init distributions are modelling choices calibrated against the measured
per-actor span demand (1M edges, seed 42) so that the guard binds for a
meaningful fraction of actors without suppressing most events; the CONSTRAINT
in each row is the grounded part.

Exact YAML added (guard lines; the state var, effect and Phi lines are in the
domain files next to them):
- cyber.yaml `flow`: `guard: { src_var: egress_allowance, amount_attr: fwd_bytes }`
- ecommerce.yaml `PURCHASED`: `guard: { src_var: available_funds, amount_attr: amount }`
- healthcare.yaml `DISPENSED`: `guard: { src_var: stock_days, amount_attr: days_supply }`
- transport.yaml `trip`: `guard: { src_var: range_m, amount_attr: distance_m }`

VERIFICATION (all real runs, seed 42, threads 8, default parquet; every output
directory deleted afterwards)

- Guards-off byte-compat, captured BEFORE any edit and re-verified AFTER, 1M
  edges per domain: sha256 prefixes of edges_00000.parquet / labels_nodes.csv
  / ground_truth.json, identical before and after for all six:

  | domain | edges_00000.parquet | labels_nodes.csv | ground_truth.json |
  |---|---|---|---|
  | finance | 5893b7aac3dfa4e9 | 334ba9ab03befa00 | a231a0d2148567d0 |
  | crypto | 9cb42c182962c420 | d8104bf5a60a2352 | 82d361e01c7b9a5c |
  | cyber | 288479830e781b73 | 3395dcd929d1066c | 43399d8a00daaaea |
  | ecommerce | e655061634f06d3a | e503581b5d472fb9 | 067476482702b5e3 |
  | healthcare | efe4e66cc613c1e4 | 266b05f3e93d764b | 9657e70e84f78835 |
  | transport | 78f81253944b05f4 | 99297fe455c03492 | 5adb949fdd3c4a6a |

  (finance and crypto equal the section-9/12 recorded hashes.)

- Violation-vs-L mini-curves (guards ON, 1M edges, span 90 d), "emitted guarded
  events that overdrew on the stale read / all emitted guarded events":

  | domain | L (days) | guarded | violations | rate | edges |
  |---|---|---|---|---|---|
  | cyber | 0.001 | 502,683 | 580 | 0.12% | 809,473 |
  | cyber | 1 | 496,857 | 32,773 | 6.60% | 803,647 |
  | cyber | span (90) | 641,733 | 219,254 | 34.17% | 948,523 |
  | ecommerce | 0.001 | 215,312 | 508 | 0.24% | 827,456 |
  | ecommerce | 1 | 219,711 | 13,276 | 6.04% | 831,855 |
  | ecommerce | span (90) | 340,300 | 152,461 | 44.80% | 952,444 |
  | healthcare | 0.001 | 106,204 | 42 | 0.04% | 942,971 |
  | healthcare | 1 | 106,966 | 1,316 | 1.23% | 943,733 |
  | healthcare | span (90) | 157,021 | 53,288 | 33.94% | 993,788 |
  | transport | 0.001 | 104,148 | 55 | 0.05% | 920,752 |
  | transport | 1 | 104,890 | 2,340 | 2.23% | 921,494 |
  | transport | span (90) | 177,860 | 79,854 | 44.90% | 994,464 |

  All four are monotone in L, near zero as L -> 0 and maximal at L = span,
  the same shape as finance (section 9) and crypto (section 5). The residual
  violations at L = 0.001 d are the documented same-second ties (the engine's
  smallest block is 1 s).

- Sweep spot-points unchanged: finance preset tiny `--no-anomalies` guards on,
  L = 0.001 / 1 / 30: violations 1,214 / 66,205 / 291,420 with edges 878,315 /
  821,880 / 961,453 (exact); crypto 1M L = 1: 23,153 / 232,899 violations,
  278,753 edges (exact).

- Oracle: tests/oracle.rs `remaining_domains_guards_match_block_oracle`
  cross-validates each of the four domains against the independent sequential
  oracle BITWISE (kept-edge multiset and final node state) at L in {1 s, 1 d,
  span}, checks the guarded-count telemetry, proves each guard fires
  (candidates suppressed at every L), and pins threads 1 vs 8 in-process. The
  oracle needed zero changes (it was already domain-generic). At the test's
  5k-node / 100k-edge / 7 d scale, suppressed counts at L = 1 s: cyber 26,533
  flows, ecommerce 21,793 purchases, healthcare 9,144 fills, transport 10,451
  trips (at L = span the healthcare guard suppresses exactly 1 fill: one
  whole-span block means every fill is judged against the initial stock).

- tests/edgecases.rs `guards_are_a_noop_for_domains_without_guards` was
  retargeted: since every built-in domain now declares a guard, the no-guard
  rule base is each of the four domains with its `guard:` fields stripped
  (guards on == guards off, zero telemetry), and the same test asserts the
  intact domain's guard fires and suppresses, and that stripping the field
  leaves guards-off output identical.

- cargo build --release clean; cargo test --release ALL green: 95 passed (the
  94 existing, none regressed, + 1 new oracle test), 2 ignored (unchanged).

HONEST NOTES
- cyber's egress allowance is a declared-quota constraint, the weakest
  grounding of the six; the domain's only amount-bearing src events are
  `flow` (bytes) and `dns` (query length), and its declared Phi predicates
  that are truly state-conditioned (`live_parent_required`,
  `session_before_auth`) need a liveness flag that nothing clears or a
  dst-side / history read, neither expressible in the src-var-vs-amount form
  without the protocol change rejected above.
- healthcare and transport have no replenishment event (restock / refuel), so
  their resources only decrease over the span; ecommerce refunds via
  RETURNED and finance/crypto have inflows, which is why their guarded counts
  at L = span stay closer to the guards-off event counts.
- Guards ON adds one column to the four domains' node exports and changes the
  new column only; no frozen artifact (paper/, paper-vldb/, paper.frozen/,
  agora-anon) was touched, and nothing was committed.

## 14. Parallel encoding (the writer ceiling)

CONTEXT. Section 2.3 of docs/VERIFICATION_MATRIX.md recorded that the
streaming path plateaus at ~4 threads (1.36x total speedup at 1e7) and called
it sink/merge-bound. Phase A (candidate drawing) already ran on the rayon pool
and merge/apply_perm were parallel; the whole row stream then funnelled into
ONE writer thread (crates/saga-io/src/threaded.rs, a std::thread behind a
bounded sync_channel) that built the Arrow RecordBatch and ran the Parquet
encoder (dictionary + ZSTD level 1, one edges_{shard:05}.parquet per shard,
row groups of max_row_group_row_count = 1,048,576 rows).

EVIDENCE (before touching anything; finance 1e7, seed 42, anomaly rate 0.02,
quiet host, loadavg < 8):
- top -H during a 3e7 parquet run at 32 threads: `saga-writer` at 99.9% CPU,
  the main thread at ~53%, the 30 rayon workers at 6 to 20% each.
- Parquet wall identical at 8 and 32 threads (2.03 vs 2.07 s); the CSV sink is
  likewise single-writer-bound (3.52 s at both).
- Temporary instrumentation (removed): the old writer spent ~0.5 s of a 2.0 s
  run building RecordBatches and ~1.5 s encoding, i.e. it was just barely
  keeping up; a discard sink (also temporary) gave 1.83 to 1.87 s at 8 and 32
  threads. So at 1e7 the writer and the engine's own serial path were
  co-bottlenecks within ~10% of each other; at 1e8 the writer was the binding
  one (5.8 M rows/s, flat from 4 threads up).

DESIGN (crates/saga-io/src/parquet.rs). The edge file is still one
SerializedFileWriter per shard, created through ArrowWriter::try_new so the
footer metadata (ARROW:schema key-value, created_by, writer version) is what
the serial writer wrote, then lowered with into_serialized_writer. Row groups
are segmented EXACTLY as ArrowWriter::write segments them: an incoming batch
that would cross the 1,048,576-row limit is sliced at the limit (same slice
sequence, so each column writer sees the same mini-batch boundaries and emits
the same pages). When a row group opens, the dispatcher (the former writer
thread, still the only thread touching the channel) creates the row group's
column writers through ArrowRowGroupWriterFactory::create_column_writers (the
same factory ArrowWriter uses) and hands each leaf column to its own encoder
thread. Each arriving slice is sent to every column thread immediately (an
Arc<RecordBatch> clone, nothing retained on the dispatcher), so encoding
streams while the group fills; at the boundary the feeds close, each thread
closes its writer into an ArrowColumnChunk, and the dispatcher appends the
chunks to the file with append_to_row_group in leaf order and the row groups
in index order. At most `groups_in_flight` = 2 sealed groups can be pending
(backpressure onto the dispatcher, and through the bounded channel onto the
engine); finished groups are drained non-blockingly after every push so
encoded chunks never accumulate. Parallelism per row group equals the number
of leaf columns (12 for finance); the encoder thread count is independent of
--threads. Parquet row groups and column chunks are independent, so the
logical content cannot depend on scheduling, and because segmentation,
properties (ZSTD 1, default dictionary/page limits), column order and
metadata are unchanged the BYTES are unchanged too (verified below). CSV and
GraphML are untouched: CSV is also single-writer-bound (3.5 s at 1e7, slower
than Parquet) and could be parallelised the same way (format chunks on
workers, append in order), but it is not a frozen-hash path and not the
default format; left as is.

ALLOCATOR (crates/saga-cli/src/main.rs, tune_allocator). The first streaming
version was byte-identical and faster guards-off but 11% SLOWER at 1e8
guards-on, with 2x the minor page faults of the serial writer, independent of
the encoder thread count (even one encoder thread reproduced it). Cause,
confirmed with glibc tunables on the unchanged binary: per-row-group column
buffers (multi-MiB) cross glibc's dynamic mmap threshold, so every chunk is
an mmap/munmap pair with fresh page faults, and those faults contend on the
process mmap lock with the sweep on the main thread. M_ARENA_MAX = 4,
M_MMAP_THRESHOLD = 64 MiB, M_TRIM_THRESHOLD = 256 MiB (set via mallopt at
process start, glibc/Linux only, no-op elsewhere) removes the churn: minor
faults drop 5 to 7x and peak RSS roughly halves. Honest attribution: the old
serial binary under the same tunables also speeds up guards-on (15.1 s at
1e8), so the guards-on gain is the allocator; guards-off the old binary stays
writer-bound at 17.1 s under the tunables while the new one reaches 13.8 s,
which is the encoder.

VERIFICATION (all real runs; cargo build --release clean; cargo test
--release 95 passed / 0 failed / 2 ignored, unchanged counts).
- Byte-compat, guards off, 1M edges, seed 42, threads 8, edges_00000.parquet
  sha256 prefix (new == recorded reference == old binary built from HEAD's
  parquet.rs in a scratch tree):
    finance 5893b7aa, crypto 9cb42c18, cyber 28847983, ecommerce e6550616,
    healthcare efe4e66c, transport 78f81253.
  labels_nodes.csv / ground_truth.json / nodes_*.parquet hashes also equal
  old (finance 334ba9ab / a231a0d2, crypto d8104bf5 / 82d361e0, cyber
  3395dcd9 / 43399d8a, ecommerce e503581b / 06747648, healthcare 266b05f3 /
  9657e70e, transport 99297fe4 / 5adb949f).
- Multi-row-group and multi-shard byte-compat vs the old binary: finance 1e7
  at 32 threads (10 row groups) 497bed93aebc0a78 both; finance 3M with
  --shard-size 20 (4 shards) 349e3ec15d2f2c06 both; guards on L = 1 1e7
  (coalesced 128k-row path) fed32dee5ab62f09 both, nodes 24d2559d80dcc641.
- Sweep spot-points (preset tiny, finance, guards on, CSV): L = 0.001 / 1 /
  30 violations 1,214 / 66,205 / 291,420, edges 878,315 / 821,880 / 961,453,
  exact.
- Guards-on telemetry unchanged at 1e8 L = 1: 75,208,475 edges, 9,021,302
  violations, old and new.

SCALE (finance, seed 42, anomaly rate 0.02, threads auto = 32, best of two,
/usr/bin/time -v peak RSS; old = HEAD parquet.rs binary, same session,
interleaved):
  1e8 guards off: old 17.17 s / 1.29 GiB / 5.81 M/s  ->  new 13.77 s /
      0.61 GiB / 7.25 M/s  (-20% wall, -53% RSS)
  1e8 guards on L = 1: old 16.85 s / 1.23 GiB / 4.46 M/s  ->  new 15.25 s /
      0.74 GiB / 4.93 M/s  (-9% wall, -40% RSS)
  1e9 guards off: recorded 182.1 s / 7.92 GB  ->  new 167.1 s / 4.38 GiB /
      5.97 M/s (one run; 172.5 s / 6.17 GiB before the allocator policy).

THREAD SCALING at 1e7 (best of two, old and new interleaved, loadavg 5 to 6):
  guards off, threads 1/2/4/8/16/32:
    old wall 2.67 / 2.19 / 1.97 / 1.97 / 1.95 / 1.98 s
    new wall 2.62 / 2.17 / 1.94 / 1.91 / 1.84 / 1.89 s
  guards on L = 1:
    old wall 3.01 / 2.59 / 2.30 / 2.24 / 2.24 / 2.29 s
    new wall 2.97 / 2.55 / 2.27 / 2.22 / 2.23 / 2.27 s

THE NEW CEILING, honestly. At 1e7 the plateau did not move: best new point
1.84 s vs 1.95 s old (6%), still flat from 4 threads. That is because the
writer was only just co-bottlenecked there; with it removed the bound is the
engine's own serial per-day path on the main thread, which does not overlap
Phase A of the next day: the streaming StatsCollector (TeeSink a) plus the
keep_all/sweep plus the channel copy. Measured by temporarily skipping
pieces (32 threads, 1e7): full 2.16 s; sink discarded 1.88 s; stats skipped
1.33 s; both skipped 1.11 s. So the stats collector alone is ~0.8 s of serial
main-thread time per 1e7 rows, the sweep/keep_all/copy ~1.1 s, and the sink
now costs ~0.1 s of tail plus some interference. The writer ceiling is
lifted (at 1e8 the sink keeps up with 7.25 M rows/s and the run is no longer
sink-bound; 519% CPU vs 381%); the next ceiling is the main thread, and
lifting it means either moving the stats collector off the main thread
(its per-row loop is independent of the sweep; a third tee thread or
per-batch parallel folds) or overlapping day w's serial work with day w+1's
Phase A. Both are engine-side changes, out of scope here, and both are
semantics-neutral. Rows-per-second-per-thread at 1 thread is unchanged
(3.8 M/s), so nothing was lost at the low end.

HYGIENE. Generated graphs deleted after every measurement; no frozen artifact
(paper/, paper-vldb/, paper.frozen/, agora-anon) touched; nothing committed.

## 15. Pipelined day loop (the main-thread ceiling)

CONTEXT. Section 14 lifted the writer ceiling and measured what was left: at
1e7 (finance, 32 threads) the run was bound by the engine's SERIAL main
thread, which per day ran Phase A's handoff, the streaming StatsCollector
(TeeSink a, ~0.8 s per 1e7 rows), the keep_all/sweep and a row copy into the
writer channel (~1.1 s together), and only then started the next day's
Phase A. Full 2.16 s; sink discarded 1.88 s; stats skipped 1.33 s; both
skipped 1.11 s. The plateau was flat from 4 threads (~1.9 s).

WHAT MOVED OFF THE CRITICAL PATH. Three scheduling changes, no semantic
change (the candidate stream, the guard decisions, the fold order and the
statistics are the same functions of the same inputs in the same order):

1. The StatsCollector runs on its own thread
   (saga_introspect::ThreadedStats, thread `saga-stats`). The generation
   thread pushes each batch into a bounded FIFO channel (depth 3) and moves
   on; the collector folds the rows off the critical path. The collector is
   a deterministic fold over the ORDERED batch stream; a single FIFO consumed
   by one thread delivers exactly the sequence TeeSink delivered inline, so
   every field of saga_stats.json (including the order-dependent ones:
   inter-event gaps, per-source burstiness/memory, HLL/Count-Min sketches)
   is bit-identical (verified below). Join happens in `finish`, before the
   report is read.

2. The day loop is pipelined (crates/saga-core/src/sim.rs, `run`). A scoped
   producer thread runs `phase_a_day` for w = 0, 1, 2, ... on the rayon pool
   and hands each finished day over a RENDEZVOUS channel (sync_channel(0)) to
   the main thread, which does day w's Phase B (sweep / keep_all / sink
   handoff / progress) while the producer is already drawing day w+1. The
   producer can be at most ONE day ahead (it blocks on the handoff of day
   w+1 until the consumer has finished day w), so the extra resident memory
   is bounded by one day of candidates plus its merge scratch.
   WHY THIS IS SAFE UNDER GUARDS. Phase A reads `params`, `world` (whose
   `state` is the immutable INITIAL state), the anomaly plan, the marked sets
   and its own per-actor memories; it never reads the sweep's running state
   S_b, and it never did (that is the section-8 L-independence property:
   candidates are keyed per absolute day). Only `filter_block` inside the
   sweep reads state, and the sweep still runs on one thread, consuming days
   strictly in order and blocks strictly in order, so every block of day w+1
   is filtered against the state left after ALL of day w's blocks merged
   (S_{b-1} for block b), exactly the same snapshot sequence as before.
   The memories carry across days inside the single producer thread, in
   day order, so their evolution is unchanged. The exchange (Phase 4) sits
   entirely inside the sweep and is untouched; its barrier schedule is the
   same.

3. Batches are MOVED, not copied, down the sink chain. `EventSink` gained
   `write_batch_owned(EventBatch)` (default: `write_batch(&batch)`, so every
   existing sink is unchanged). The engine calls it for each guards-off day
   and for each swept block's `kept` rows (both were dropped right after the
   write anyway); the guards-on path also moves the day INTO the pending
   buffer when that buffer is empty (the common case at L <= 1 day) instead
   of appending a copy. ThreadedSink's channel now carries
   `Arc<EventBatch>`, so an owned batch crosses to the writer thread with no
   row copy, and the CLI's tee (`SharedTee` in generate.rs) wraps each batch
   in ONE Arc and hands the same allocation to the stats thread and the
   writer thread. The coalescing (guards-on Parquet) path still copies rows
   into its 128k-row chunk, as before; when the chunk is empty the incoming
   owned batch becomes the chunk directly.

What did NOT change: merge_sorted / apply_perm (still on the pool), the
sweep itself, fold_canonical, the exchange, the Parquet/CSV/GraphML writers
and their segmentation (so bytes are unchanged), the allocator policy.

VERIFICATION (all real runs, cargo build --release clean, loadavg 4 to 7).
- cargo test --release: 95 passed / 0 failed / 2 ignored (same counts as
  section 14; the oracle suite, the independent semantic check, included).
- Byte-compat, guards off, 1M edges, seed 42, threads 8, old binary (this
  tree before the edits, built in a scratch copy) vs new, edges_00000.parquet
  sha256 prefix, equal to the recorded references:
    finance 5893b7aa, crypto 9cb42c18, cyber 28847983, ecommerce e6550616,
    healthcare efe4e66c, transport 78f81253.
  labels_nodes.csv, ground_truth.json, every nodes_*.parquet AND
  saga_stats.json hashes equal old in all six domains (stats: finance
  6ba27484, crypto 5495ab74, cyber 43e57d7c, ecommerce 8f89bb3a, healthcare
  add1f4fd, transport b6af921f); saga_meta.json result identical minus the
  wall/rate fields.
- Guards on, finance 1M seed 42, L = 0.01 / 1 / 10000 (>= span), threads 1
  and 8, old and new, CSV (bytes = logical content): edge stream sha256
  prefix 839236636048 / 025099964b50 / 5623a6843035 in all four (old/new x
  1/8 threads) at each L; nodes.csv e77bc30a58 old == new; saga_stats.json
  8a5c8a0b / 2c0e111b / 3ca34eab old == new; written / guarded / violations
  894,233 / 429,059 / 5,556; 887,156 / 421,982 / 20,437; 952,234 / 487,060
  / 162,107, identical across binaries and thread counts.
- Sweep spot-points (preset tiny, finance, guards on, --no-anomalies, CSV):
  L = 0.001 / 1 / 30 violations 1,214 / 66,205 / 291,420, edges 878,315 /
  821,880 / 961,453, exact. (Note for the record: these spot-points are
  taken with --no-anomalies; with anomalies on the same preset gives
  1,216 / 65,736 / 290,351 on BOTH the old and the new binary.)
- Guards-on telemetry at 1e8 L = 1: 75,208,475 edges, 9,021,302 violations,
  old and new.

THREAD SCALING at 1e7 (finance, seed 42, anomaly rate 0.02, parquet, best
of two, old and new interleaved in one session, /usr/bin/time wall):
  guards off, threads 1/2/4/8/16/32:
    old wall 2.63 / 2.16 / 1.93 / 1.94 / 1.88 / 1.91 s
    new wall 1.74 / 1.25 / 1.00 / 0.94 / 0.94 / 0.93 s
  guards on L = 1:
    old wall 2.99 / 2.60 / 2.31 / 2.31 / 2.27 / 2.29 s
    new wall 1.79 / 1.31 / 1.15 / 1.15 / 1.13 / 1.14 s
The plateau moved from ~1.9 s to ~0.94 s guards off (2.0x) and from ~2.3 s
to ~1.14 s guards on (2.0x), and it now sets in at 8 threads rather than 4
(4 -> 8 threads is still a 6% gain guards off; 8 -> 32 is flat). The
1-thread point also drops (2.63 -> 1.74 s) because "1 thread" is the rayon
pool size: the producer, the consumer, the stats thread and the writer are
now four concurrently busy threads even with a one-worker pool.

SCALE (threads auto = 32, best of two, /usr/bin/time peak RSS):
  1e8 guards off: old 13.88 s / 0.65 GiB / 7.21 M/s -> new 7.99 s /
      1.13 GiB / 12.58 M/s (-42% wall; CPU 520% -> 974%)
  1e8 guards on L = 1: old 15.33 s / 0.75 GiB / 4.92 M/s -> new 7.54 s /
      0.92 GiB / 10.05 M/s (-51% wall)
  1e9 guards off (one run each): old 160.8 s / 4.28 GiB / 6.21 M/s -> new
      89.4 s / 7.62 GiB / 11.18 M/s (-44% wall; 997,703,683 edges both)
  1e9 guards on L = 1 (one run each): old 174.9 s / 4.66 GiB / 4.07 M/s ->
      new 89.1 s / 6.27 GiB / 8.01 M/s (-49%; 712,374,311 edges and
      110,064,377 violations, old and new)
RSS went UP, by ~0.45 GiB at 1e8 guards off. Honest accounting: a 1e8 run
has ~1.1M rows per day (~150 MB per day batch); the pipeline now keeps
resident the day being drawn (its parts plus the merged copy), the day
held by the consumer, and up to depth-3 batches queued to each of the two
sink threads, all of which used to be serialized through one buffer. It is
bounded by a constant number of day batches (it does not grow with the run
length), it is the price of the overlap, and the channel depths (3 and 3)
are the knob if it ever matters. At 1e9 the increase is larger in absolute
terms (+3.3 GiB guards off, 4.28 -> 7.62 GiB) because a 1e9 day batch is
~11M rows (~1.5 GB) and the same constant number of them are in flight;
it is still a constant number of day batches, not a function of run length,
but at 1e9 and beyond the channel depths should be lowered (or the day
split into sub-day windows) if RSS matters more than the 44% of wall.

THE NEW CEILING, honestly (instrumented scratch build, env-var toggles,
never in the tree; 1e7, 32 threads, best of two):
  guards off: full 0.92 s; sink discarded 0.81; stats skipped 0.86; both
  skipped 0.72; Phase A only (consumer drops each day) 0.67 s.
  guards on L = 1: full 1.14 s; sink discarded 0.93; stats skipped 1.10;
  both skipped 0.89; Phase A only 0.69 s.
  Phase A only with a 1-worker pool: 1.59 s.
So the run is now within ~25% (guards off) of the time it takes just to
DRAW the candidates, and Phase A itself only speeds up 2.4x from 1 to 32
workers (1.59 -> 0.67 s): its per-day structure has serial parts (one
pool.install per behavior with a small actor chunk count, the concatenation
of the parts, the argsort over the day). That is the first component of the
new bound. The second is the stats thread: top -H during a 3e7 run shows
`saga-stats` at 93% CPU and `saga-writer` at 73%, i.e. the single-threaded
collector (~80 ns/row: three HLLs, a Count-Min, a per-day hash map, the
per-source temporal fold) is now saturated at ~12 M rows/s, which is exactly
where 1e8 guards off lands (12.6 M/s). Removing it buys 0.06 s at 1e7 but
it is the binding thread at 1e8. Guards on, the sweep on the consumer thread
is the larger residual (sink discarded 0.93 vs Phase A 0.69 s). Lifting
these means (a) parallelising inside phase_a_day (all behaviors in one
parallel region, a parallel merge instead of concat+argsort) and (b) a
parallel stats fold (per-batch partial sketches merged in order, which is
exact for the sketches and counters but changes the floating-point
summation order of the moments unless done with the same sequential
fold), both out of scope here and the second not semantics-free for the
JSON bytes. Nothing was lost at the low end: 1 worker is 1.74 s vs 2.63 s.

HYGIENE. Generated graphs deleted after every measurement; scratch old and
instrumented builds deleted; no frozen artifact (paper/, paper-vldb/,
paper.frozen/, agora-anon) touched; nothing committed.

## 16. NFS-robust discovery (the barrier's polling primitive, rebuilt)

WHAT WAS WRONG (docs/remeasure_exchange_cluster.md). The section-12 barrier
discovered a peer's delta file by polling `File::open(<dir>/b<b>_from<j>_to<k>.bin)`
every 2 ms. On the cluster's NFSv4.2 mount (`acdirmin=30`, default
`lookupcache`) the first failed open installs a NEGATIVE dentry that the
client trusts until the directory's cached attributes expire, so the peer's
atomically renamed file became visible only at the next attribute refresh:
N=2 across two nodes took 1,351 s for 90 rounds (15 s per barrier),
deterministically; a measurement sidecar that created and unlinked a file in
the directory every 5 ms (forcing revalidation) brought rounds to 4-5 ms; a
three-node 2+1+1 placement still hung on stale negative dentries (on the same
node `ls` listed the file while `wc -c` got ENOENT). Output was always
correct; the defect was liveness/latency of the polling primitive only.

THE NEW PRIMITIVE (crates/saga-core/src/exchange.rs; wire format, atomic
temp-name-then-rename publish, consume-and-delete, the canonical block
schedule, the merge semantics and the timeout semantics are all unchanged):
- Per-round subdirectory. Round `b` lives in `<dir>/r<b>/`, created by
  whoever arrives first (`mkdir`; `EEXIST` is success and is never followed
  by a stat, which a stale negative dentry could make lie). Files keep their
  names (`b<b>_from<k>_to<j>.bin`, temp `.b<b>_from<k>_to<j>.tmp`, renamed
  within the round dir). A fresh, never-looked-up directory name per round
  means no client can hold a stale negative entry for anything in it from an
  earlier round.
- Discovery by listing. A reader never opens a predicted name. Each poll
  LISTS the round directory (readdir) and opens only the expected inbound
  names that the listing returned (the cluster diagnostic showed `ls` seeing
  what `open()` did not). If an open still fails with NotFound right after a
  listing showed the name (client cache skew), it is counted
  (`exchange_skewed_opens`) and retried on the next poll, never treated as
  absence. Each consumed file is deleted as before (single reader).
- Backoff. The poll sleep starts at 1 ms, doubles to a 32 ms cap, and resets
  to 1 ms whenever a round makes progress (a file consumed). On a local
  filesystem the first listing usually already succeeds, so a round costs one
  readdir of a 2-6 entry directory.
- Revalidation nudge, slow path only. A poll that found nothing new, in a
  round that has already waited >= 4 ms, first creates and unlinks a
  reader-private hidden file `.nudge<k>` in the round dir (exactly what the
  sidecar did, but issued by the engine, only while waiting, and only in the
  directory being waited on). The create returns post-op directory attributes
  to this client, invalidating its cached readdir pages and dentries for the
  round dir. Nudges are counted (`exchange_nudges`); on-time peers on a local
  disk produce none (below: 0 of 180 rounds at N=2, 1-5 of 360 at N=4).
  The nudge file can never block the round-dir removal: a reader nudges only
  while one of its inbound files is still unconsumed, and the dir is removable
  only after every file in it was consumed.
- Cleanup. After consuming its last inbound file of round `b` a shard calls
  `remove_dir(r<b>)`; it fails harmlessly with ENOTEMPTY for everyone but the
  last consumer. A round dir can be empty only after every shard published
  into it (each shard publishes before it consumes), so no shard ever
  publishes into a removed round dir; a clean run leaves the exchange dir
  empty and `tidy()` removes it as before. A timed-out lone shard leaves
  exactly `r<b>/b<b>_from<k>_to<j>.bin` (its own outbound file), no nudge
  file, and the error message still names the missing file and peer.
- Injectable filesystem. The primitives (create_dir, publish, list, read,
  remove_file, remove_dir, nudge) sit behind `exchange::ExchangeFs`;
  production uses `RealFs` only (`Exchanger::new`), tests use
  `Exchanger::with_fs`. Telemetry additions in `GenSummary` (and the CLI
  summary line): `exchange_idle_polls`, `exchange_nudges`,
  `exchange_skewed_opens`.

NFS EVIDENCE WITHOUT THE CLUSTER. This host has no NFS mount (`mount -t
nfs,nfs4` is empty), so the pathology is injected: `exchange::tests::CachingFs`
is one client's view of the shared directory with the two measured defects,
(a) negative-lookup caching (a failed `open()` sticks for `neg_ttl`, even
after the peer's rename landed), (b) listing/open skew (a name a listing
shows opens only `open_lag` after it first appeared on that client); a nudge
clears that client's negative entries for the directory, as the sidecar
did. Each shard runs over its own `CachingFs` (its own client cache) on the
same real directory, in its own thread. Results (cargo test --release,
saga-core lib):
- `listing_discovery_is_immune_to_negative_lookup_caching`: with
  `neg_ttl` = 3 s, the OLD predicted-name 2 ms `open()` poll on one file
  published 20 ms after the first poll waits 3.0009 s (the whole TTL, i.e.
  the 15 s per round of the cluster scaled to the test's TTL); the NEW
  primitive runs 3 shards x 6 rounds with arrivals staggered by up to 20 ms
  on the same caching clients in 125 ms total (~21 ms per round, all of it
  the injected stagger), shards' (nudges, skewed opens, idle polls) =
  (14, 0, 32) / (4, 0, 21) / (0, 0, 0).
- `listed_name_that_fails_to_open_is_retried_not_treated_as_absent`: 2
  shards x 5 rounds with `open_lag` = 40 ms and `neg_ttl` = 3 s complete in
  318 ms (~64 ms per round, the skewed opens retried with backoff: 28/29
  skewed opens counted per shard, no TTL stall).
- `on_time_peers_take_the_fast_path_and_round_dirs_are_removed`: 4 shards x
  20 rounds in 13.8 ms (0.7 ms per round), 0 nudges, every round dir and the
  exchange dir removed.
- `missing_peer_times_out_naming_the_round_file`: the 1 s timeout fires in
  [1, 3) s naming `r0/b0_from1_to0.bin` and `peer shard 1`; the leftover is
  exactly `r0/b0_from0_to1.bin`.

VERIFICATION ON THIS HOST (real subprocesses, seed 42; graphs and exchange
dirs deleted)
- cargo build --release clean; cargo test --release ALL green: 99 passed
  (the 95 existing, none regressed, + the 4 unit tests above), 2 ignored,
  including the 3 exchange_cli subprocess tests (N=2/4 union and slices, the
  2 s timeout, the rail) and the 4 phase4 engine-level tests.
- Equivalence, guards on, finance 1M, L=1, 90 rounds, N=2 and N=4, 2 reps
  each, before (old binary) and after: union edges 887,156 == single; union
  edge digest 36ca881b..., node-state union digest bf8c5d0e... (the
  section-12/cluster digests); every shard's node rows a subset of the
  reference, shard id sets disjoint; per-shard digests IDENTICAL old vs new
  (N=2 shards 1e67846f/d9e02a12 edges, 0aafd961/64c14b39 nodes; N=4
  8943896f/bf59ee81/13132c87/133741db); telemetry identical (N=2 643,748
  deltas / 22,531,180 B; N=4 970,704 / 33,974,640 B; 90 rounds per shard).
  The exchange dir is empty and removed after every run.
- Single-host latency (finance 1M, guards on, L=1, 90 rounds, threads 8,
  local-disk exchange dir, quiet host loadavg ~4; 5 reps, binaries
  alternated; single-machine reference 0.305 s mean):

  | N | old makespan (mean of 5) | new makespan | old per round | new per round | new: idle polls / nudges / skewed (sum over shards, 90 rounds) |
  |---|---|---|---|---|---|
  | 2 | 0.376 s (0.373-0.380) | 0.366 s (0.357-0.380) | 0.80 ms | 0.69 ms | 113-129 / 0 / 0 |
  | 4 | 0.414 s (0.404-0.421) | 0.407 s (0.398-0.425) | 1.22 ms | 1.14 ms | 355-423 / 1-5 / 0 |

  No regression (the readdir-based poll with 1 ms initial backoff is
  slightly cheaper than the fixed 2 ms open-poll).
- Byte-compat guards-off (finance 1M seed 42 threads 8 parquet):
  edges_00000.parquet sha256 5893b7aa..., labels_nodes.csv 334ba9ab...,
  ground_truth.json a231a0d2... - unchanged. Sweep spot-points (preset tiny,
  finance, guards on, --no-anomalies, CSV): L = 0.001 / 1 / 30 violations
  1,214 / 66,205 / 291,420, edges 878,315 / 821,880 / 961,453 - exact.

CLUSTER RECIPE (to repeat docs/remeasure_exchange_cluster.md with the new
binary; scripts in docs/repro/exchange_cluster/bin/x16.slurm + shard16.sh,
no poke sidecar). Stage the binary and scripts:

```
scp target/release/agora docs/repro/exchange_cluster/bin/{x16.slurm,shard16.sh,digest.py} \
    itiger:$PROJECT/saga/bin/
```

Reference (one node, one process; gives `$REF` for the digests):

```
sbatch --nodes=1 --ntasks=1 --cpus-per-task=8 $PROJECT/saga/bin/ref.slurm
```

N=2 on two nodes, the naive case that took 1,351 s (expect ~1.3 s per shard):

```
sbatch --nodes=2 --ntasks=2 --job-name=x16_n2 \
  --export=ALL,N=2,NN=2,NODES=100000,EDGES=1000000,TAG=x16_n2,TO=120,REPS=2,REF=$PROJECT/saga/xrun/ref/rep1/out \
  $PROJECT/saga/bin/x16.slurm
```

N=4 on three nodes (2+1+1), the placement that hung even with the poke:

```
sbatch --nodes=3 --ntasks=4 --job-name=x16_n4_3n \
  --export=ALL,N=4,NN=3,NODES=100000,EDGES=1000000,TAG=x16_n4_3n,TO=120,REPS=2,REF=$PROJECT/saga/xrun/ref/rep1/out \
  $PROJECT/saga/bin/x16.slurm
```

N=4 1e7 (3+1 over two nodes, the 9.3 s vs 7.5 s result):

```
sbatch --nodes=2 --ntasks=4 --job-name=x16_n4_1e7 \
  --export=ALL,N=4,NN=2,NODES=1000000,EDGES=10000000,TAG=x16_n4_1e7,TO=600,REPS=2,REF=$PROJECT/saga/xrun/ref1e7/rep1/out \
  $PROJECT/saga/bin/x16.slurm
```

Inside each job the shards are started by one
`srun --nodes=$NN --ntasks=$N --cpus-per-task=8 --distribution=block bash shard16.sh`
and each task runs
`agora generate --domain finance --nodes $NODES --edges $EDGES --seed 42 --guards --block-len-days 1 --threads 8 --json --quiet --shard-count $N --shard-index $SLURM_PROCID --exchange-dir $PROJECT/saga/xrun/$TAG/rep$r/xchg --exchange-timeout-s $TO --out /tmp/...`.
The job log prints per-shard walls, the summed `exchange_*` telemetry
(watch `exchange_nudges` and `exchange_skewed_opens`: the cluster's
per-round cost should now be governed by the 1-32 ms backoff plus NFS
create/readdir latency, not by `acdirmin`), the leftover entry count of the
exchange dir (0 = removed), and the digests against `$REF`. A 2+1+1 run that
completes in seconds with digests PASS closes the open item of the cluster
note; if `exchange_skewed_opens` is large there, the listing/open skew is
real on that client and the retry path is what carried the round.

HONEST NOTES
- The NFS evidence here is an injected model of the two measured defects,
  not a measurement on NFS; the model is deliberately pessimistic (nothing
  but expiry or a nudge clears a negative entry, a listing never does), and
  the new primitive never opens an unlisted name, so it never installs one.
  Whether the cluster's 2+1+1 placement is fully explained by these two
  defects is exactly what the recipe above re-measures.
- The nudge adds NFS create/unlink traffic only while a shard waits > 4 ms
  for a late peer, at most one pair per poll (poll period 1-32 ms).
- The round-dir layout changes what a timed-out run leaves behind
  (`r<b>/...` instead of top-level files) and the path in the timeout
  message; message wording, wire format and telemetry semantics are
  unchanged. GenSummary gained three `#[serde(default)]` fields.

## 17. Guard-aware anomaly calibration

THE PROBLEM. The planner (anomaly.rs: `campaign_expected_events`, the
`MAX_ANOMALY_FRACTION` cap, `rate_scale`, `n_campaigns`) sizes campaign
volume against the CANDIDATE stream, and sim.rs rebalances the normal stream
against the same candidate expectation. Guards (sections 5, 13) then filter
candidates in Phase B, and they filter anomalous and normal candidates at
DIFFERENT rates: a structuring-sized finance transfer exceeds a typical
balance more often than a normal transfer (finance 10k/200k/7d, L = 1 d:
anomalous candidates 39.1% suppressed vs normal 26.5%), while in crypto the
guarded `tx`/`swap` types are ~70% suppressed and the campaign events ride
mostly unguarded types, so the realized fraction moves UP. Either way the
realized anomalous-edge fraction under guards drifts from the planned one
(finance reference config: 0.894% planned and realized guards-off, 0.740%
realized guards-on), which the paper had to document as a limitation
("read realized rates from the output when guards are on").

DESIGN CHOICE: option A, a deterministic measured fixed point, opt-in.
- Option B (an analytic `P[balance_0 >= amount]` correction) was rejected
  as the default: it ignores the staleness dynamics that ARE the point of
  the L knob (balances drain during the span, within-block reads are
  stale, counterparties refill each other), and the measured suppression
  differs per L and per domain (the tables below), so a first-order
  closed form would have to be re-derived per guard and would still leave
  an L-dependent residual. It is also not cheaper in any way that matters:
  one guarded pass of the generator costs seconds at 1M edges.
- Option C (report-only) is kept as the always-on fallback: with guards on
  the summary now prints `anomaly planned X% vs realized Y% under guards`
  (GenSummary `planned_anomalous_fraction`, the planner's expected campaign
  events over the expected delivered total), so the drift is never silent.
- Option A is EXACT in expectation, deterministic, and generic across
  guards and domains because it measures what the guards actually did.

MECHANISM (`GenParams::calibrate_passes`, CLI `--calibrate-under-guards
[PASSES]`, default 1; requires `--guards` and a whole-graph run).
1. `sim::run` prepares the world and plan ONCE (identical to a plain run).
2. Each calibration pass runs the COMPLETE guarded pipeline (Phase A + the
   L-block sweep) into a counting sink. The sweep (`BlockSweep::filter_block`,
   guards-on path only) tallies, per event type `e` and stream class
   (normal / campaign-labeled / failure-labeled), the candidates it saw and
   the candidates it kept (`SurvivalTally`).
3. `solve_calibration` sets, per event type with campaign candidates,
       g_e = s_bg / s_camp(e)
   where `s_bg` is the survival of the background stream (normal plus
   failure-labeled events, all types) and `s_camp(e)` the survival of the
   campaign candidates of type `e` under the multipliers that pass ran with.
   Campaign candidate counts scale with the multiplier, so kept campaign
   edges of type `e` become `g_e * s_camp(e) * C_e = s_bg * C_e`, and the
   kept labeled fraction returns to the CANDIDATE labeled fraction, which is
   exactly the guards-off realized rate (Phase A is guard- and
   L-independent, section 8). Unguarded campaign types get `g_e = s_bg < 1`
   (the background shrank, so they must too); the rule is one uniform
   statement: every campaign event type survives at the background rate.
   Multipliers are clamped to [1/8, 8] (only reachable when a type's
   campaign candidates were almost all suppressed; reported, never silent).
4. `AnomalyPlan::guard_calibration` (empty unless a pass ran) multiplies
   into `gen_campaign_window`'s stage rate next to `rate_scale`; passes > 1
   iterate the fixed point (survival per attempt falls slightly as the rate
   rises because the ring drains its own balances faster).
5. The final run is a plain pass with the multipliers in place.

Why this preserves the invariants:
- Guards-off byte-compat: `calibrate_passes = 0` leaves `guard_calibration`
  empty, and the emitter does not multiply at all on an empty slice, so the
  guards-off arithmetic, RNG consumption and output are untouched. The
  uncorrected guards-on output is likewise unchanged (the tally increments
  are side-effect free). Re-verified below.
- Determinism (Theorem 1): the multipliers are a pure function of integer
  tallies of a deterministic pass, and the pass itself is schedule-
  independent, so the corrected run is bit-identical for any thread count
  and across reruns. Verified below at 1M edges and in-process.
- The cap: the planner's cap bounds the planned fraction; after calibration
  the realized fraction equals the planned one, so the cap now holds in
  realized terms under guards too.
- Sharding: refused (`shard_count > 1` errors out) because a shard sees only
  its slice of the stream; per-shard multipliers would disagree and the
  shards would no longer union to one graph. The Python binding keeps
  `calibrate_passes = 0` (it has no guarded mode).
- The "requested rate" the calibration targets is the measured candidate
  labeled fraction (the guards-off realized rate), not the planner's
  closed-form `planned_fraction`: the two differ by the planner's own
  approximations (e.g. finance small: 0.883% planned vs 0.894% realized),
  and the guards-off output is the thing users compare against.

Cost: one extra full generation per pass (the real pipeline with a counting
sink, no IO), so `--calibrate-under-guards` is ~2x wall, `2` is ~3x.

MEASUREMENTS (seed 42, threads 8, CSV; all output directories deleted).
"requested" = guards-off realized anomalous-edge fraction (the calibration
target); "planned" = the planner's closed-form fraction; gap = relative
deviation of the realized fraction from the requested one.

| domain / config | L | guards off (requested) | guards on, uncorrected | corrected, 1 pass | corrected, 2 passes |
|---|---|---|---|---|---|
| finance 10k/200k/7d (planned 0.883%) | 0.001 | 0.894% (1798/201025) | 0.753% (-15.8%) | 0.877% (-1.9%) | 0.916% (+2.4%) |
| finance 10k/200k/7d | 1 | 0.894% | 0.740% (-17.3%) | 0.883% (-1.3%) | 0.901% (+0.8%) |
| finance 100k/1M/90d defaults (planned 0.129%) | 0.001 | 0.123% (1226/998371) | 0.109% (-11.4%) | 0.117% (-5.1%) | 0.122% (-0.9%) |
| finance 100k/1M/90d defaults | 1 | 0.123% | 0.110% (-10.3%) | 0.117% (-4.6%) | 0.121% (-1.9%) |
| ecommerce 100k/1M/90d defaults (planned 0.861%) | 0.001 | 0.861% (8581/997103) | 1.029% (+19.5%) | 0.860% (-0.1%) | 0.859% (-0.2%) |
| ecommerce 100k/1M/90d defaults | 1 | 0.861% | 1.024% (+19.0%) | 0.860% (-0.1%) | 0.860% (-0.1%) |
| crypto 100k/1M/90d, --anomaly-rate 0.05 (planned 0.0089%) | 0.001 | 0.0099% (97/981595) | 0.0273% (+176%) | 0.0099% (+0.2%) | 0.0099% (+0.2%) |
| crypto 100k/1M/90d, --anomaly-rate 0.05 | 1 | 0.0099% | 0.0287% (+190%) | 0.0090% (-9.3%) | 0.0090% (-9.3%) |
| crypto 100k/1M/90d defaults (36 anomalous edges) | 0.001 | 0.0037% | 0.0106% (+189%) | 0.0027% (-26%) | 0.0031% (-16%) |
| crypto 100k/1M/90d defaults | 1 | 0.0037% | 0.0111% (+203%) | 0.0029% (-22%) | 0.0032% (-13%) |

Multipliers the 1-pass runs used (L = 1): finance small transfer x1.414,
cash_deposit x0.861; finance 1M transfer x1.263, cash_deposit x0.889;
ecommerce PURCHASED x1.159, WROTE_REVIEW / RETURNED / VOTED_HELPFUL x0.833;
crypto rate-0.05 tx x0.398, swap x0.507, token_transfer / add_liquidity /
approve x0.284.

Honest reading of the residuals:
- Where the labeled count is in the thousands (finance small, ecommerce),
  one pass lands within 2% relative of the request, ecommerce within 0.2%;
  the uncorrected gap was 16 to 20%.
- The residual is dominated by Poisson sampling of the campaign counts
  under the new rate (the count is a fresh Poisson draw, ~1/sqrt(n)): at
  finance 1M the labeled count is ~1000 (noise ~3%), and the 1-pass
  residual of 5% reflects the fixed point not yet reached (survival per
  attempt drops as the rate rises; pass 2 brings it to 1-2%). Crypto at the
  default prevalence has 36 anomalous edges in 1M, so the kept count after
  correction is 8-9 and a +-20% residual is two edges; at `--anomaly-rate
  0.05` (97 labeled) the correction lands within 0.2% / 9.3%. The crypto
  rows also show that the drift is not one-signed: there the UNCORRECTED
  guards-on rate is ~3x the request.
- Stated tolerance: with >= ~2k labeled edges expect the 1-pass corrected
  rate within ~2% relative of the guards-off rate; below that the Poisson
  floor of the labeled count itself is the bound. The tolerance is not a
  claim that the fixed point is exact after one pass; `2` passes are
  available and cost one more generation.
- Multi-pass overshoot (finance small, 2 passes: +0.8% / +2.4%) is within
  the same Poisson floor (1.6k labeled edges, ~2.5%).

DETERMINISM EVIDENCE
- CLI, finance 1M defaults, `--guards --block-len-days 1
  --calibrate-under-guards`, CSV: threads 1 vs 8 vs a second threads-8
  run: edges_00000.csv sha256 bff7f8465e4a470d, nodes.csv 076b401588cd2a86,
  ground_truth.json 5b16938af4d9c43b, labels_nodes.csv 334ba9ab03befa00,
  all three identical.
- In-process: tests/calibration.rs
  `calibration_is_deterministic_across_threads_and_reruns` (2 passes,
  threads 1 vs 8 vs rerun: multipliers, row multiset, stream order, per-
  intent counts and node exports all equal).

BYTE-COMPAT AND SWEEP SPOT-POINTS (re-verified after the change, 1M edges
per domain, seed 42, threads 8, parquet): finance 5893b7aac3dfa4e9 /
334ba9ab03befa00 / a231a0d2148567d0, crypto 9cb42c182962c420 /
d8104bf5a60a2352 / 82d361e01c7b9a5c, cyber 288479830e781b73 /
3395dcd929d1066c / 43399d8a00daaaea, ecommerce e655061634f06d3a /
e503581b5d472fb9 / 067476482702b5e3, healthcare efe4e66cc613c1e4 /
266b05f3e93d764b / 9657e70e84f78835, transport 78f81253944b05f4 /
99297fe455c03492 / 5adb949fdd3c4a6a (edges / labels_nodes / ground_truth;
identical to section 13). Finance preset tiny, `--no-anomalies --guards`,
L = 0.001 / 1 / 30: violations 1,214 / 66,205 / 291,420 with edges 878,315 /
821,880 / 961,453 (exact). The uncorrected guards-on finance reference run
still gives 172,752 edges, 1,278 anomalous (0.740%).

GUARDS x ANOMALIES vs L (measured 2026-08-21 on the frozen binary; finance,
`--nodes 10000 --edges 200000 --time-span 7 --seed 42 --guards`, anomalies on,
no calibration; the run quoted in paper-vldb/main.tex sec:diff):

| L | edges written | overdraft_guarded | overdraft_violations | anomalous edges kept |
|---|---|---|---|---|
| 1 s (1.1574e-5 d) | 179,710 | 84,925 | 2 | 1,350 |
| 1 day | 172,752 | 77,967 | 12,942 | 1,278 |

Staleness acts on the adversarial stream as on the normal one: violations
rise from 2 to 12,942 between the two endpoints while the kept anomalous
edges move from 1,350 to 1,278.

TESTS. tests/calibration.rs adds three tests: realized-rate-within-tolerance
(finance 10k/200k/7d at L in {0.001, 1}: corrected within 5% relative of the
guards-off fraction, never farther than uncorrected, telemetry fields agree
with the measured fractions, every labeled kept edge joins a ground-truth
instance and every normal edge carries anomaly_id -1), determinism (above),
and refusal without guards / under sharding. cargo build --release clean;
cargo test --release ALL green: 102 passed (the 99 existing + 3 new), 2
ignored (unchanged). The oracle and edge-case suites are untouched and green
(the guard path itself did not change; the tally is passive).

SURFACE CHANGES. GenParams gained `calibrate_passes: u32` (0 = off; every
constructor in tests and saga-py sets 0). GenSummary gained
`planned_anomalous_fraction`, `calibration_passes`,
`calibration_target_fraction`, `calibration_uncorrected_fraction` and
`calibration_multipliers`, all `#[serde(default)]`. AnomalyPlan gained
`guard_calibration` and `planned_fraction`. The CLI prints the planned-vs-
realized line whenever guards are on and the calibration lines when it ran.
No frozen artifact (paper/, paper-vldb/, paper.frozen/, agora-anon) was
touched; nothing was committed.

## 18. State-conditioned behavior: the Phase-B transducer (review Option B)

The advisor review observed that the model in section 1 allows behavior to be a
general function of the snapshot, while the implementation only admitted or
suppressed a candidate. This section closes that gap: state now also changes
what an event looks like and how often an agent acts, without breaking the
L-independence of the candidate stream.

Mechanism. A rule base may attach a `switch` to an event type:

    switch:
      src_var: balance          # own state only, like the guard
      below: 5000.0             # active while the read value is below this
      rate_multiplier: 0.5      # thinning: the declared rate is the MAXIMUM
      attr_overrides:           # different samplers while active
        - attr: amount
          dist: !log_normal { mu: 4.0, sigma: 0.9 }

Phase B evaluates the switch against the same block-start snapshot the guard
reads. While it is active the agent (i) emits only `rate_multiplier` of its
candidate events, so its effective rate is state-dependent even though the
candidate stream is not, and (ii) draws the listed attributes from different
samplers. The switch runs before the guard, so the guard checks the amount the
agent actually decided to send, and the emitted row (not the candidate row) is
what feeds the effects, the exact ledger and the sink.

Determinism. The switch's latent draws are re-derived in Phase B from the
candidate's canonical key `(src, dst, t, event_type)` on a dedicated stream
purpose (`StreamPurpose::Switch = 7`), so they depend neither on L, nor on the
block partition, nor on the thread that produced the candidate. Theorem 1's
induction is unchanged: within a block the transducer is a pure function of the
frozen snapshot and of key-derived randomness, and the boundary merge is still
the canonical fold. Verified: finance 5e5 edges, seed 42, guards and switch on,
L = 1 day, threads 1 versus 8 give identical edges, nodes, labels and ground
truth (`edges_00000.csv` sha256 prefix faaf3151, `nodes.csv` 61d46778).

New metric: behavior-switch lag. For every switch evaluation the engine also
computes the decision the never-stale ledger would have given, that is the
decision of an event-serial execution, and counts the disagreements. This is to
the transducer what guard leakage is to the guard. Measured (finance, 1e6 target
edges, seed 42, 20k nodes, 30-day span, guards and switch on):

| L (days) | switch decisions | misprofiled | misprofiled % | thinned | switched emitted | edges |
|---|---:|---:|---:|---:|---:|---:|
| 1.157407e-05 | 315955 | 15 | 0.005 | 158242 | 157698 | 835993 |
| 0.001 | 317178 | 1503 | 0.474 | 158131 | 157544 | 836096 |
| 0.01 | 325148 | 10672 | 3.282 | 157513 | 156963 | 836491 |
| 0.1 | 338436 | 25723 | 7.601 | 156497 | 156216 | 836599 |
| 1 | 348323 | 36594 | 10.506 | 155983 | 155746 | 836117 |
| 7 | 374282 | 65862 | 17.597 | 154174 | 154246 | 832672 |
| 30 | 423800 | 122736 | 28.961 | 150670 | 150394 | 826223 |

The lag is monotone in L over this sweep and is what a user trades away when
choosing a larger block: at 86 seconds of staleness fewer than one switch
decision in 200 differs from the event-serial decision, while at the
no-feedback endpoint nearly three in ten do.

Interaction with the guard. Adaptive behavior lowers constraint pressure: with
the switch active, low-balance accounts send smaller amounts, so at L = 1 day
guard leakage falls from 14.3% (guards only) to 0.55% (guards plus switch) in
the same configuration. Behavior and admission are therefore not independent
knobs, and the paper reports them separately.

Scope. Switches read own state only, like guards, so Theorem 2 and the
per-block exchange are unchanged. `rate_multiplier` can only thin, never
amplify: a state-dependent rate increase would require drawing candidates at
the higher rate and thinning the base case, which is a rule-base authoring
choice rather than an engine change. Off by default (`--state-behavior`); with
the flag off, output is byte-identical to the recorded digests for all six
domains.
