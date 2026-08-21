# SAGA architecture

A map of the codebase against the blueprint (`mustread.txt`). The data contract
is `RuleBase -> World -> EventStream(+labels) -> Export`, each link strictly
typed for independent testing.

## Crates (Rust workspace)

| crate | role | blueprint |
|---|---|---|
| `saga-host` | CPU/RAM/GPU/disk/NUMA probe + dry-run cost model (feasibility guard) | §11a |
| `saga-rules` | the 7-primitive `RuleBase` schema, `GenerationConfig`, presets, validation, `saga_meta.json`; the 6 built-in domains as embedded YAML | §5, §12 |
| `saga-sample` | splittable seeded RNG (`stream(seed, purpose, a, b)`), Vose alias tables, compiled distribution samplers | §7 |
| `saga-topology` | 8 skeleton/evolution models → CSR adjacency | §8 |
| `saga-core` | the DES engine: world build, windowed parallel event loop, anomaly campaigns/failures, calibration | §7, §3 |
| `saga-introspect` | single-pass streaming stats: HLL, Count-Min top-k, Welford+reservoir | §11b |
| `saga-io` | sharded Parquet/CSV/GraphML writers, readers, background-writer thread | §7 |
| `saga-cli` | the `saga` binary: doctor / domains / init / rules / generate / stats / validate | §12 |
| `python/saga_rag` | offline corpus fetcher + RAG rule-base synthesis (off the hot path) | §9 |

## The seven primitives (`saga-rules`)

`EntityType` · `RelationRule` (pluggable topology) · `EventType` (emits a
temporal edge + state effects) · `BehaviorProcess` (normal dynamics) ·
`Constraint` Φ (detector's-eye legality) · `AdversaryProcess` /
`FailureProcess` (anomaly sources, labeled by intent) · `ControlParams` (the
five axes). All serde-typed; the loader validates type-coherence (an actor can
only emit events of its own type; neighbor relations must be rooted at the
actor type; ring/chain scopes need same-type events).

## Determinism

Every random decision draws from `stream(master_seed, Purpose, id_a, id_b)` —
a SplitMix64-derived PCG stream keyed by what the decision is *about*, not by
thread or iteration order. World build, topology, per-actor behavior, anomaly
placement, and attribute draws all key off stable ids, so output is
**bit-identical for any `--threads`**. Within a window, parallel chunks emit
into private buffers that are merged and totally time-sorted before writing.

## The event loop (`saga-core/sim.rs`)

Time is partitioned into 1-day windows. Per window:
1. **generate** (parallel over behavior × actor-chunk, rayon): inhomogeneous
   Poisson via thinning (diurnal × weekly), geometric bursts, O(1) counterparty
   (skeleton neighbor / repeat-partner / global).
2. **campaigns** (parallel over campaigns overlapping the window): staged
   adversary policies; failures (silence/rate-shift/attr-corruption) woven into
   the normal stream.
3. **merge + parallel sort** to a total `(t, src, dst, event_type)` order.
4. **apply state effects** in time order.
5. **stream** the batch to the sink (stats tee + background writer thread).

Memory is bounded by the densest window, not the run → streams to 10⁸–10⁹ edges.

## Calibrated emergence (the P0 contribution, §3)

Anomalies are not injected. `AdversaryProcess`/`FailureProcess` are generative
processes whose events emerge from the same loop and carry an **intent label**.
The five control axes set only *process parameters* and calibrate to targets:
- **prevalence** → node-fraction budget → campaign/incident counts
- **difficulty** `d` → `c_eff = clamp(2·d·camouflage)`: feature camouflage
  (attrs blend toward the normal distribution) + low-and-slow rate damping
- **type-mix** → per-intent weights · **placement** → community blocks / time
  windows · **cascade** → follow-up campaigns + neighbor failure propagation

A calibration layer rebalances normal volume against expected anomaly volume to
hit the edge budget, and caps anomalies at a minority fraction of edges.

## Self-built RAG (`python/saga_rag`, §9)

Offline, one-time, never per event: a real corpus of downloaded authoritative
standards (CORPUS.md manifest) → embed → hybrid retrieve (dense + BM25) → draft
a rule base under schema constraints → validate (the Rust loader is the final
authority) → compile. Zero-code domain migration, every rule traceable to a
source.
