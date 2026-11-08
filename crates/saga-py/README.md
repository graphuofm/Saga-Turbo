# Saga Turbo (Python bindings)

Agent-based-simulation graph benchmark generator. `pip install saga`, then:

```python
import saga

saga.domains()        # list built-in domains
saga.doctor()         # probe CPU/RAM/GPU/disk

# write a dataset to disk
s = saga.generate("finance", nodes=20000, edges=200000,
                  time_span_days=20, anomaly_rate=0.03, out="./out")

# or get the graph straight into memory as numpy arrays (no disk)
a = saga.generate_arrays("finance", nodes=10000, edges=100000, time_span_days=10)
a["src"], a["dst"], a["t"], a["label_codes"], a["anomaly_id"]

# staleness knobs (same semantics as the CLI flags)
s = saga.generate("finance", nodes=20000, edges=200000, time_span_days=20,
                  guards=True, block_len_days=0.01, calibrate_passes=1, out="./out")
s["overdraft_guarded"], s["overdraft_violations"], s["planned_anomalous_fraction"]
```

## Staleness knobs

`block_len_days` is L, the simulation block length: within a block, events read
node state from the block-start snapshot, so an update becomes visible to other
events only at the next block boundary. `guards=True` turns on the per-domain
event state guards (e.g. the finance overdraft guard); the output then depends
on L, and the summary reports `overdraft_guarded` (guarded transfers) and
`overdraft_violations` (transfers that passed a stale guard; falls toward 0 as
L -> 0). `calibrate_passes=k` (requires `guards=True`) runs k extra measurement
passes so the realized anomalous fraction under guards matches the planned one
(`planned_anomalous_fraction`, `calibration_*` keys); each pass costs a full
extra generation run. Defaults (`block_len_days=1.0`, `guards=False`,
`calibrate_passes=0`) produce output identical to earlier releases.

Guarded sharded generation (`--guards --shard-count N --exchange-dir`) is
CLI-only: the N shard processes exchange per-block state through a shared
directory, and the CLI is the multi-process entry point. `generate(...,
guards=True, shard_count>1)` raises with that explanation.
