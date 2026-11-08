# Saga Turbo

Saga Turbo generates large, attributed,
time-stamped graphs with exact ground-truth anomaly labels by running an
agent-based simulation whose rules are compiled from domain standards. The
engine is a Rust workspace that executes the simulation in parallel under a
block-bounded staleness consistency model: the timeline is cut into blocks of
`L` days, agents inside a block read state that is at most one block stale,
and cross-agent invariants (guards) are enforced at block boundaries. The
result is bit-identical for a given seed regardless of thread count or number
of shard processes, so a benchmark can be regenerated anywhere from its
parameters alone. A Python package (PyO3 bindings plus an evaluation and
rule-extraction harness) wraps the same engine.

## Install

Requires stable Rust 1.80+ and, for the Python wheel, Python 3.9+ and maturin.

```bash
git clone https://github.com/graphuofm/Saga-Turbo && cd Saga-Turbo
cargo build --release                 # binary at target/release/saga

# optional: Python wheel (import name: saga)
pip install maturin
maturin build --release -m crates/saga-py/Cargo.toml
pip install target/wheels/saga-*.whl
```

## Quick start

```bash
saga generate --domain finance --edges 1000000 --seed 42 --out DIR
saga validate DIR                     # re-check every declared invariant post hoc
saga doctor                           # host probe and feasible-scale estimate
saga domains                          # list built-in domains
```

Consistency knobs on `saga generate`:

| flag | meaning |
|---|---|
| `--block-len-days L` | block length in days; state read inside a block is at most `L` days stale |
| `--guards` | enforce cross-agent invariants (for example no overdraft) at block boundaries |
| `--calibrate-under-guards` | re-calibrate anomaly rates so the labeled fraction is met with guards on |

Graph-form switches on `saga generate` (all off by default; with them off the
output is unchanged):

| flag | meaning |
|---|---|
| `--node-dynamics` | apply the rulebase's node lifecycles: nodes are born in construction order and may die after a sampled lifetime; no edge touches a node outside its lifetime |
| `--state-trajectory` | also write the `node_states` table: the initial state, then every state cell a block changed, stamped with the block end |
| `--state-behavior` | let an agent's own state change its event rate and attribute samplers |

Event types that declare a duration attribute (cyber flows, transport trips,
healthcare prescriptions) also get a `t_end` column, and `labels_nodes.csv`
lists every labeled node with its role (operator, member, affected). See
`docs/OUTPUT.md` for the schemas.

Sharded multi-process generation (N processes, one machine or several sharing
a directory):

```bash
for i in $(seq 0 3); do
  saga generate --domain finance --edges 1000000 --seed 42 --guards \
    --shard-count 4 --shard-index $i --exchange-dir /shared/xchg --out DIR/shard$i &
done; wait
```

The union of the shard outputs is identical, edge for edge, to the
single-process run with the same parameters.

Python:

```python
import saga
s = saga.generate("finance", nodes=20_000, edges=200_000, time_span_days=20,
                  guards=True, block_len_days=0.01, out="out")
a = saga.generate_arrays("finance", nodes=100_000, edges=10_000_000, time_span_days=90)
```

## Regression gate

```bash
bash scripts/regress.sh               # build, workspace tests, then the pinned invariants
```

The gate pins logical digests and file hashes for all six domains at 100k and
1M edges, thread-count determinism with guards on, calibration tolerance, and
equality of real multi-process shard runs against the single-process run. It
must print `REGRESS: ALL PASS`. The pins are listed in
`crates/saga-cli/tests/regress.rs` and explained in
`docs/VERIFICATION_MATRIX.md`.

## Domains

Six built-in domains, each a YAML rulebase in `crates/saga-rules/domains/`:
finance, crypto, cyber, ecommerce, healthcare, transport. `DOMAINS.md`
describes entities, events, anomaly types and constraints per domain;
`CORPUS.md` lists the standards each rulebase is grounded in. The corpus text
itself and the third-party real datasets used for fidelity evaluation are not
redistributed (see below).

## Repository layout

| path | contents |
|---|---|
| `crates/saga-core` | simulation engine, consistency model, shard exchange |
| `crates/saga-rules` | rulebase loader and the six domain YAML files |
| `crates/saga-topology`, `saga-sample` | topology models and samplers |
| `crates/saga-io`, `saga-introspect`, `saga-host` | Parquet/CSV/GraphML output, single-pass statistics, host probe |
| `crates/saga-cli` | the `saga` binary and its CLI tests (including the regression pins) |
| `crates/saga-py` | PyO3 bindings |
| `python/saga_eval` | fidelity, downstream and baseline evaluation harness |
| `python/saga_rag` | grounded parameter extraction from the standards corpus |
| `scripts/regress.sh` | the regression gate |
| `docs/` | design and audit trail, see next section |

## Audit trail

`docs/` contains the consistency model (`CONSISTENCY_MODEL.md`), the
verification matrix mapping each claim to a test (`VERIFICATION_MATRIX.md`),
the engine issue log (`ENGINE_ISSUES.md`), one `remeasure_*.md` note per re-measured result, and
`docs/repro/` with the scripts and raw JSON behind every number. Also in
`docs/`: `ARCHITECTURE.md`, `INSTALL.md`, `OUTPUT.md`, `EVAL.md`,
`EXPERIMENTS.md`, `BASELINES.md`, `VALIDATION.md`.

Some documents refer to an internal design blueprint (`mustread.txt`) that is
not part of this repository.

## Reproducibility

All reported benchmark graphs use seed 42. Output is bit-identical across
thread counts and across shard-process counts; determinism is verified on the
logical content of the output (sorted edge digests, labels, counters) and,
for the pinned configurations, on file SHA-256 as well.

Paths in `docs/repro/` scripts are relative to the repository root and
expect the release binary at `./target/release/saga`. Cluster scripts use
`$PROJECT` for the shared project directory.

### Requires datasets not included here

The following need `realdata/` (third-party datasets with their own licenses)
or `corpus/` (standards text) and are not runnable from this repository
alone: the fidelity and downstream evaluations in `python/saga_eval` and the
corresponding `docs/repro/*fid*`, `*downstream*`, `*anom*` scripts; the RAG
pipeline in `python/saga_rag` (except `validate_compile.py`). The Rust
workspace tests and `scripts/regress.sh` are self-contained.

## Citation

This repository is Saga Turbo, the full system. The earlier demonstration (SAGA, VLDB 2026 demo track):

```bibtex
@article{ding2026saga,
  author    = {Jiacheng Ding and Xiaofei Zhang},
  title     = {{SAGA}: Synthetic Agentic Graph Architecture for Temporal Benchmark Generation},
  journal   = {Proceedings of the VLDB Endowment},
  volume    = {19},
  number    = {12},
  pages     = {4834--4837},
  year      = {2026},
  doi       = {10.14778/3827998.3828134}
}
```

The full paper describing the engine and the consistency model is under
submission.

## Authors

- Jiacheng Ding, University of Memphis, jding2@memphis.edu
- Xiaofei Zhang, University of Memphis, xiaofei.zhang@memphis.edu

License: all rights reserved. No license is granted beyond the rights GitHub requires to host and display this repository; see LICENSE.

## Acknowledgement

Cluster experiments were run on the iTiger cluster at the University of
Memphis (Sharif et al., arXiv:2504.14786).
