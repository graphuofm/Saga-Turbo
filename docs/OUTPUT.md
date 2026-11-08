# Saga Turbo output format & loaders

Every `saga generate` run writes a self-describing output directory. This page
documents the layout and gives copy-paste loaders for PyG, DGL, Neo4j, igraph
and polars/pandas — **no preprocessing required**.

## Directory layout

```
out/
  edges_00000.parquet, edges_00001.parquet, …   # the temporal edge stream, sharded
  nodes_<entity_type>.parquet                    # one table per entity type
  ground_truth.json                              # omniscient per-instance anomaly record
  labels_nodes.csv                               # per-node anomaly labels (actors, with role)
  node_states.parquet                            # state trajectory (only with --state-trajectory)
  saga_meta.json                                 # config + seed + version + host probe + result
  saga_stats.json                                # single-pass introspection report
  metrics.prom                                   # Prometheus exposition of run self-metrics
```

With `--format csv` the edge shards are `edges_*.csv` and nodes are a single
`nodes.csv`; with `--format graphml` a single `graph.graphml` is written.

## Edge schema

| column | type | meaning |
|---|---|---|
| `src`, `dst` | uint64 | global node ids (match `id` in the node tables) |
| `t` | int64 | event time, **unix seconds** (epoch is in `saga_meta.json`) |
| `event_type` | string (dict) | which `EventType` fired (e.g. `transfer`, `swap`) |
| `label` | string (dict) | **ground-truth intent**: `normal` or an anomaly intent |
| `anomaly_id` | int64 | **ground-truth instance id**: `-1` for normal, else the campaign/incident that caused this edge — joins to `ground_truth.json` |
| *(domain attrs)* | float64 / string | per-event attributes; **null where not applicable** to that event type |
| `t_end` | int64 | end time, unix seconds: `t + duration` for event types whose rule declares `duration_attr` (cyber flows, transport traversals and trips, healthcare prescriptions and dispensings), else `t`. Present only when the domain declares a duration. |

Edges are written in non-decreasing `t` order within each shard, and shards are
ordered by name, so concatenating them in filename order yields a globally
time-sorted stream.

`label` is the generative cause, not a detector's guess — the central Saga Turbo
property. The exact per-intent counts are in `saga_stats.json`
(`label_introspection`).

## Node schema

`nodes_<type>.parquet`: `id` (uint64) plus that entity type's attributes and
**final** state variables (e.g. an account's closing `balance`, `txn_count`).

## Loaders

### polars / pandas
```python
import polars as pl, glob
edges = pl.concat([pl.read_parquet(f) for f in sorted(glob.glob("out/edges_*.parquet"))])
accounts = pl.read_parquet("out/nodes_account.parquet")
y = (edges["label"] != "normal")          # binary anomaly target
```

### PyG (temporal / heterogeneous)
```python
import torch, polars as pl, glob
edges = pl.concat([pl.read_parquet(f) for f in sorted(glob.glob("out/edges_*.parquet"))])
edge_index = torch.tensor([edges["src"].to_list(), edges["dst"].to_list()])
t      = torch.tensor(edges["t"].to_list())
label  = torch.tensor((edges["label"] != "normal").to_list(), dtype=torch.long)
# e.g. torch_geometric.data.TemporalData(src=edge_index[0], dst=edge_index[1], t=t, msg=...)
```

### DGL
```python
import dgl, torch, polars as pl, glob
e = pl.concat([pl.read_parquet(f) for f in sorted(glob.glob("out/edges_*.parquet"))])
g = dgl.graph((torch.tensor(e["src"].to_list()), torch.tensor(e["dst"].to_list())))
g.edata["t"]     = torch.tensor(e["t"].to_list())
g.edata["label"] = torch.tensor((e["label"] != "normal").to_list(), dtype=torch.long)
```

### igraph
```python
import igraph as ig, polars as pl, glob
e = pl.concat([pl.read_parquet(f) for f in sorted(glob.glob("out/edges_*.parquet"))])
g = ig.Graph(edges=list(zip(e["src"], e["dst"])), directed=True)
g.es["t"] = e["t"].to_list(); g.es["label"] = e["label"].to_list()
```

### Neo4j (bulk import via CSV)
```bash
saga generate --domain finance --preset small --format csv --out out
# then map columns: nodes.csv -> :Account, edges_*.csv -> [:EVENT {t, label, amount}]
neo4j-admin database import full --nodes=out/nodes.csv --relationships=out/edges_00000.csv ...
```

## Reproducibility

`saga_meta.json` records the seed, full config, Saga Turbo version, git commit and the
host probe. Re-running with the same seed + config + version reproduces the
output **bit-for-bit**, independent of thread count (`--threads`).

## Ground truth (`ground_truth.json` + `labels_nodes.csv`)

Because Saga Turbo *is* the mechanism, the ground truth is the full causal record —
not just a binary label (§3, §11b). This is what a real-data curator can never
provide and is the main difference from topology generators.

`ground_truth.json`:
```json
{
  "intent_names": ["normal", "structuring", ...],
  "edges_per_intent": [["normal", 1949613], ["structuring", 13435], ...],
  "instances": [
    {"id": 0, "intent": "structuring", "kind": "adversary",
     "camouflage": 0.4, "n_members": 4, "members": [33589, 40769, ...],
     "community": 5, "start_t": 1737297477, "end_t": 1739120894,
     "cascade": false}
  ]
}
```
Each edge's `anomaly_id` joins to `instances[].id`, so you can stratify
detector evaluation by **typology** (intent), **difficulty** (camouflage),
**community** (placement), **time window**, and **cascade** membership — exact
and free.

`labels_nodes.csv` (`node_id,intent,kind,anomaly_id,camouflage,community,role`):
per-NODE labels for the anomaly actors, for node-level detection benchmarks.
The member lists are complete. `role` is `operator` for the member that runs
the scheme (the centre of a hub stage or the collector of a fan-in stage),
`affected` for a failure's entity, else `member`.

## Node dynamics (`--node-dynamics`)

Each entity type may declare a `lifecycle` in its rule base. With the flag,
node `i` of a type (in id order, which is the topology's construction order)
exists from t = 0 if `i / n < initial_fraction`, else it is born at the matching
point of a uniform ramp over the span; it dies `lifetime_days` after birth.
No edge is emitted outside a node's lifetime, at either end. The node tables
gain `birth_t` and `death_t` (unix seconds; `death_t` is null for nodes that
never die). Birth and death are pure functions of the seed and the node id, so
the output stays identical for any thread count. Without the flag the
population is fixed and the output is unchanged.

## State trajectory (`--state-trajectory`)

`node_states.parquet` (`node`, `t`, `var`, `value`): the initial value of
every state cell at the epoch, then, after each L-block merge, the new value
of every cell the block changed, stamped with the block end. Each row is a
cell of a merged snapshot, so the table is identical for any thread count.
The last row of each (node, var) equals the final value in the node table.
Sharded runs need `--exchange-dir` for this table.

## Distributed generation (sharding)

For graphs beyond one machine, run the SAME command on N machines with the
same seed/config and a distinct `--shard-index`:
```bash
# on machine k of N (here N=4):
saga generate --domain finance --preset huge --seed 42 \
    --shard-index $k --shard-count 4 --out /shared/out
```
Each shard writes `out/shard_<k>/` and emits only the edges whose **source
node** it owns (`node % N == k`) plus its node-table slice. The shards are
disjoint and their union is **bit-for-bit identical** to a single-machine run
(determinism makes this exact — verified by `sharded_union_equals_whole_graph`).
Load the whole graph with `glob("out/shard_*/edges_*.parquet")`.
