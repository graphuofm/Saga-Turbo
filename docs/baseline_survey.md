# Temporal-generator baselines: what we ran, what it took to run it

Audit record for the learned temporal-graph generators the review asked us to
compare against (DYMOND, ETN-gen, TagGen, TIGGER, STM). This file records the
provenance, the environment, and every source change we had to make. The
measured numbers are in `docs/baseline_results.md`.

Machine: single node, NVIDIA RTX A5000 (24 GB), Python 3.10.12, Ubuntu.
All runs at the repository default seed unless noted. Wall time and peak
resident set size are from `/usr/bin/time -v` around the whole process.

## 1. Provenance

| generator | source | state on arrival | ran end to end |
|---|---|---|---|
| DYMOND (WWW'21) | upstream repo | 2 patches (below) | yes |
| ETN-gen (Comm. Phys. 2024) | upstream repo | runs as published | yes |
| TagGen (KDD'20) | upstream repo | 5 patches (below) | yes |
| TIGGER (AAAI'22) | upstream repo | 2 patches (below) | yes |
| STM | no public implementation located | - | no |

STM: we did not locate a public implementation. It is described in the related
work and excluded from the measured table rather than reported from the
original paper's numbers, which were measured on different data and hardware.

## 2. Source changes we made

Every change below is a compatibility repair, not a modelling change. We list
them so that anyone re-running these numbers gets the same code. Each is a
break introduced by a dependency after the generator was published; none alters
the generative process.

### DYMOND (2 patches)

1. `learn_parameters.py`, 3 call sites: `time_code()` is called with no
   argument, but `log_time()` returns a lambda that requires one. Changed to
   pass the argument. Without this the script raises `TypeError` before
   fitting begins.
2. `learn_parameters.py`: the fitted `model_params` dictionary receives `'N'`
   from `dataset_info` but never `'L'`, while the sampler reads
   `model_params['L']`. Added `model_params['L'] = dataset_info['L']`. Without
   this the sampler raises `KeyError: 'L'`.

Both are unconditional failures on any input, so DYMOND as published does not
run end to end in our environment without them.

### TagGen (5 patches)

1. `graph_utils.py`: `np.int` (removed in NumPy 1.24) to `int`.
2. `node2vec/node2vec.py`: same.
3. `deepwalk/graph.py`: `from collections import Iterable` to
   `collections.abc` (moved in Python 3.3, removed in 3.10).
4. `deepwalk/skipgram.py`: `from gensim.models.keyedvectors import Vocab`
   (removed in gensim 4) replaced with the `Word2Vec` import the module
   actually needs, and the `size` keyword renamed to `vector_size`
   (gensim 4 rename). `deepwalk/main.py`: same rename at 2 call sites.
5. `graph_utils.py` and `graph_fairnet.py`: the two `os.system("python ...")`
   shell-outs changed to `python3`, since the environment has no `python`
   on PATH.

TagGen's bundled `metrics.py` additionally fails on
`nx.from_numpy_matrix` (removed in networkx 3.0). We did not patch it: that
file is TagGen's own evaluation script, not its generator, and we score every
generator with the same metric code (`python/saga_eval`) rather than each
generator's own. Generation completes and writes its output sequences before
that script is invoked.

### TIGGER (2 patches)

1. `tgg_utils.py`: `np.int` to `int`.
2. `model_classes/*.py`, 4 files: `pack_padded_sequence(X, X_lengths, ...)`.
   Since PyTorch 1.7 the `lengths` argument must be a CPU tensor; TIGGER pins
   `torch==1.6.0`. Changed to `X_lengths.cpu()`.

TIGGER's pinned requirements (`torch==1.6.0`, `numpy==1.18.1`, `scann==1.2.1`,
`python_igraph==0.9.6`) have no wheels for Python 3.10, so restoring the pinned
environment was not an option on this machine; we ran against current versions
with the two patches above.

### Non-patch adjustments

Not source changes, but needed to reproduce the runs.

- Missing packages installed at current versions: `gensim` (TagGen),
  `powerlaw`, `python-igraph`, `path`, `numba`, `h5py` (TIGGER).
- TIGGER's `train_transductive.py` calls `os.mkdir(config_dir)` on a
  multi-level path, so the parent must exist first (`mkdir -p models/hs11`).
- TIGGER's graph assembly at the documented `--time_window=1` raises
  `IndexError: list index out of range` in
  `metrics/metric_utils.py::sample_adj_graph_topk` when a window receives no
  sampled edges. Our target has 13,617 time steps over 126 nodes, so empty
  windows are common. We used `--time_window=500`, which is inside the range
  the README describes; the assembly then completes.
- ETN-gen was run on its own bundled `Datasets/High_School11.dat` at the
  `gap=299` documented in its notebook. An earlier attempt on CollegeMsg
  produced 2 edges: that dataset is far too sparse at a 299 s window for this
  method, which is a property of the input, not of ETN-gen.

## 3. Output extraction

DYMOND, ETN-gen, and TIGGER emit a temporal edge list (or a per-snapshot edge
set we concatenate with its snapshot index as the timestamp).

TagGen emits random walks over composite `(node, timestamp)` tokens rather than
an edge list. We convert with `docs/repro/baselines/taggen_seq_to_edges.py`,
which decodes tokens through the `index` dictionary in TagGen's own
`graph.pickle` and emits one edge per consecutive pair at
`max(t_i, t_j)`. Both the decoding and the pair rule are taken from TagGen's
own `metrics.py`. That file then accumulates the edges into a cumulative
adjacency and thresholds it to the observed per-window edge count; that step is
TagGen's evaluation convention rather than part of generation, so we stop at the
temporal edge list, which is what our comparison scores.

## 4. Task split

The generators do not solve the same problem, and scoring them on one number
hides that. We report two tasks.

- **Task A, prior-driven.** The generator is given a target size (node count,
  edge count, time span) and nothing else. It never sees the target graph.
  SAGA, Erdos-Renyi, Barabasi-Albert, R-MAT, and Watts-Strogatz are in this
  task.
- **Task B, target-conditioned.** The generator is fitted to the target graph
  itself. DYMOND, ETN-gen, TagGen, TIGGER, the configuration model, and
  Chung-Lu are in this task: each reads the real graph before generating.

A Task B generator has strictly more information than a Task A generator, so a
Task B generator scoring higher on a similarity metric is the expected outcome,
not a finding. The comparison that carries information is within a task.

## 5. Timestamp protocol for the static generators

The static generators (ER, BA, R-MAT, Watts-Strogatz, configuration model,
Chung-Lu) produce topology with no time. How we attach timestamps determines
what their temporal metrics say, so we report two protocols rather than one.

- **uniform**: `m` timestamps drawn uniformly over the real span and sorted.
  Uses one real scalar, the span.
- **renewal (matched marginal)**: inter-event gaps drawn by bootstrap from the
  real inter-event-gap distribution, cumulated, and rescaled to the span. This
  hands the static baseline the real inter-event marginal outright, which is
  more target information than SAGA is given.

Both are implemented in `python/saga_eval/baselines.py`
(`set_time_protocol`). The measured effect of the choice is in
`docs/baseline_results.md`; it is not negligible, and under the renewal
protocol the configuration model scores above SAGA on the composite.
