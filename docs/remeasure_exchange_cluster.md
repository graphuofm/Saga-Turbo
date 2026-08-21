# Re-measurement: per-block exchange ACROSS MACHINES (iTiger SLURM cluster)

Date: 2026-08-20/21. Binary: the locally built `target/release/agora` (saga 0.1.0,
glibc 2.34 symbols; runs unchanged on the cluster's Rocky 9.4 / glibc 2.34, no
rebuild). Purpose: replace the paper's "real multi-process exchange measured on
one host so far" limitation (CONSISTENCY_MODEL.md section 12) with real
cross-machine numbers: N `agora` processes on N different nodes, exchange
directory on the shared NFS filesystem. Every number below is from a real run;
the small per-run JSON/log summaries are kept (see "Artifacts"), all generated
graphs and exchange dirs were deleted.

## Cluster

- University of Memphis iTiger, partition `bigTiger`, SLURM. Per-user quota
  MaxNodes = 3 (hard: a 4-node job pends forever with
  `AssocMaxNodePerJobLimit`), MaxJobs = 6 shared with other running jobs.
- Nodes used: itiger01 (AMD EPYC 9334 32-core, 773 GB), itiger02/03/05/06/07/08
  (AMD EPYC 7513 32-core, 515 GB), 64 hardware threads each, kernel
  5.14.0-427.31.1.el9_4, Rocky Linux 9.4. Each job asked for 8 cpus per task
  and ran `--threads 8`. The nodes were SHARED with other users' jobs (1-min
  load averages at shard start are recorded per run below; itiger01 sat at
  load ~24 throughout).
- Interconnect: 200 Gb/s InfiniBand 4X HDR (mlx5_0) on every node; the NFS
  server is `<nfs-server>` (IB-side name). Shared filesystem:
  `/project` (and `/home`) = NFSv4.2, `hard,proto=tcp,rsize=wsize=1 MiB,
  acregmin=3,acregmax=60,acdirmin=30,acdirmax=60`, default `lookupcache`
  (negative lookups cached). `/tmp` is node-local.
- The exchange dir was always on `$PROJECT/saga/xrun/<tag>/rep<r>/xchg`
  (NFS, shared by all shards, fresh per run). Shard outputs were written to
  node-local `/tmp` during the timed run and copied to NFS afterwards for the
  digests; the single-node references did the same.

## Commands (exact)

Reference (one node, one process), `bin/ref.slurm`, `sbatch --nodes=1 --ntasks=1 --cpus-per-task=8`:

```
agora generate --domain finance --nodes 100000 --edges 1000000 --seed 42 \
  --guards --block-len-days 1 --threads 8 --json --quiet --out /tmp/<job>/rep<r>
```

Cross-machine shards, `bin/x.slurm` + `bin/shard.sh`, submitted as
`sbatch --nodes=N --ntasks=N ... bin/x.slurm`; inside the job, all N tasks are
started concurrently by one

```
srun --nodes=$NN --ntasks=$N --cpus-per-task=8 --distribution=block bash shard.sh
```

and each task runs (K = `$SLURM_PROCID`)

```
agora generate --domain finance --nodes 100000 --edges 1000000 --seed 42 \
  --guards --block-len-days 1 --threads 8 --json --quiet \
  --shard-count $N --shard-index $K \
  --exchange-dir $PROJECT/saga/xrun/<tag>/rep<r>/xchg --exchange-timeout-s 600 \
  --out /tmp/saga_x_<job>/shard$K
```

(1e7 runs: `--nodes 1000000 --edges 10000000`.) Per-task wall is
`date +%s.%N` around the process on its own node (skew-free); the makespan is
max(end) - min(start) across nodes (node clocks are NTP-synced, the start
skew across nodes was 40-100 ms) and is also bounded above by the `srun` wall.

Digests (`bin/digest.py`, pyarrow): the edge digest is sha256 over the sorted
multiset of `(src, dst, t, event_type, label, amount-bits)` over all
`edges_*.parquet`; the node-state digest is sha256 over the sorted multiset of
full rows of every `nodes_<type>.parquet` (floats by bits). Shard union =
multiset union of the shard dirs; "slice" = every shard node row is present
in the reference's multiset (plus shard id sets pairwise disjoint and the
union equal to the reference).

## Step 2: single-node reference (finance 1M, seed 42, guards on, L = 1 d, 90 blocks)

| where | run | engine wall | process wall | edges | guarded / violations | edge digest | node digest |
|---|---|---|---|---|---|---|---|
| itiger05 (load 48) | rep1 | 0.892 s | n/a (see note) | 887,156 | 421,982 / 20,437 | 36ca881b... | bf8c5d0e... |
| itiger05 (load 48) | rep2 | 0.906 s | n/a | 887,156 | 421,982 / 20,437 | 36ca881b... | bf8c5d0e... |
| local i9-13900K (load 5-6) | rep1 | 0.456 s | 0.486 s | 887,156 | 421,982 / 20,437 | 36ca881b... | bf8c5d0e... |
| local i9-13900K | rep2 | n/a | 0.551 s | 887,156 | same | 36ca881b... | bf8c5d0e... |

Full digests: edges `36ca881b9b98755ebccb017d269b6fc6f96c92f97c7cf16e5d294ca48ac10865`,
node state `bf8c5d0e4892d38cd476992e2af7df9fed07c7fd07bd30bbd878583ad61454c9`,
IDENTICAL on the cluster (AMD EPYC, Rocky 9) and the local Intel host: the
guarded run is bit-reproducible across machines. (Note: the cluster reference
job's process-wall line was lost because `bc` is not installed on compute
nodes; the engine's own `wall_time_s` is reported instead. All later scripts
use awk.) The cluster is ~2x slower per process than the local desktop at this
size (shared node, load 48, slower cores).

## Step 3/4: cross-machine exchange, equivalence (Theorem 2 across real machines)

Every row: the union of the N shards' edge multisets equals the reference
(887,156 edges, digest 36ca881b...), the union of the shards' node-state rows
equals the reference (100,000 rows, digest bf8c5d0e...), shard node-id sets
are pairwise disjoint and every shard's rows are a subset of the reference
(slice check). Shard edge counts N=2: 438,406 + 448,750; N=4: 215,178 +
227,885 + 223,228 + 220,865; shard node counts 50,000 x 2 / 25,000 x 4.
Per-shard edge/node digests are identical between the cluster and the local
single-host runs (e.g. N=2 shard 0 edges `1e67846f...`, shard 1 `d9e02a12...`).

| config | nodes (shards per node) | rep | union edges == ref | node union == ref | slices ok | disjoint |
|---|---|---|---|---|---|---|
| N=2, naive NFS polling | itiger01 (1) + itiger02 (1) | rep2 | PASS 887,156 | PASS 100,000 | PASS | PASS |
| N=2, naive NFS polling | same | rep1 | telemetry identical; outputs lost (see notes) | | | |
| N=2, poked | itiger01 (1) + itiger02 (1) | rep1, rep2 | PASS | PASS | PASS | PASS |
| N=4, poked | itiger01 (3) + itiger02 (1) | rep1, rep2 | PASS | PASS | PASS | PASS |
| N=4, 1e7, poked | itiger01 (3) + itiger02 (1) | rep1, rep2 | PASS 8,858,299 | PASS 1,000,000 | PASS | PASS |
| local single host, N=2 / N=4 | 1 host | 2 reps each | PASS | PASS | PASS | PASS |
| local single host, N=4, 1e7 | 1 host | 1 rep | PASS 8,858,299 | PASS 1,000,000 | PASS | PASS |

"poked" = see the NFS finding below. N=4 on 4 distinct nodes is impossible
under the MaxNodes=3 quota; N=4 on 3 nodes (2+1+1) was attempted and is
reported as a failure below, so the N=4 cross-machine numbers are 3+1 over
two machines.

## The NFS finding: negative-lookup caching vs the 2 ms poll

The exchange polls `File::open(<dir>/b<b>_from<j>_to<k>.bin)` every 2 ms. On
a local directory that is ~1.2 ms per barrier. On this NFS mount the first
failed open installs a NEGATIVE dentry that the client trusts until the
directory's cached attributes expire (`acdirmin=30`), so a peer's atomically
renamed file becomes visible only at the next attribute refresh:

- N=2 naive, itiger01 + itiger02: both shards took 1,351 s for 90 rounds,
  twice (rep1 engine wall 1351.6 / 1351.0 s; rep2 process wall 1351.19 /
  1350.91 s) = 15.0 s per barrier round, deterministic. Output still exactly
  correct (rep2 digests PASS): the protocol is correct on NFS, just slow.
- Measurement-side workaround, no engine change: `shard.sh` runs a sidecar
  per task that creates and unlinks a hidden file in the exchange dir every
  5 ms (`: > $XD/.poke$K; rm -f $XD/.poke$K`), forcing that node's client to
  revalidate the directory. With it the 2-node runs took 1.2-1.3 s per shard
  (table below), i.e. the 15 s per round collapsed to a few ms. The sidecar
  leaves no file behind (the dir is empty at the end; in two of the N=4 runs
  the empty dir itself survived because a poke raced the engine's best-effort
  `remove_dir`).
- The workaround did NOT rescue the 3-node placement (2+1+1). Three attempts
  (itiger01-03 twice, itiger06-08 once): shard pairs on the two single-shard
  nodes could not see each other's files for minutes at a time, the 1M run
  progressed ~1 block per 3-4 minutes and was cancelled (the 120 s timeout
  variant failed cleanly: "timed out after 120s waiting for the block-1 delta
  file ... from peer shard 1"). Diagnostic during a hung run on itiger03:
  `ls` of the exchange dir listed `b0_from0_to3.bin` while `wc -c` on the same
  node returned ENOENT for it, i.e. a stale negative dentry on that client
  that the local create/unlink poke did not invalidate. All nodes show the
  same kernel and mount options; why two-node runs are immune and three-node
  runs are not was not resolved. A robust fix belongs in the engine (e.g.
  `readdir`-based polling, a per-round sub-directory, or a socket
  rendezvous), or in the mount (`lookupcache=none`/`noac`, root only).

## Step 5: cost, per-barrier latency, delta volume

Single-host comparison, this host today (load 5-8, local-disk exchange dir):
N=2 per-shard 0.59/0.68 s and makespan 0.60/0.68 s; N=4 per-shard 0.61-0.63 s
and makespan 0.63/0.62 s, against the 0.49/0.55 s single run: ~1.1-1.5 ms per
barrier round, matching the ~1.2 ms of section 12 (0.52 s vs 0.43 s on a
quieter host).

Cross-machine (NFS exchange dir, poked), finance 1M:

| config | rep | per-shard process wall | makespan (cross-node) | srun wall | 1-min load at start | (makespan - ref 0.90 s) / 90 |
|---|---|---|---|---|---|---|
| N=2, itiger01+02 | rep1 | 1.254 / 1.300 s | 1.30 s | 1.61 s | 24.0 / 6.3 | 4.4 ms per round |
| N=2, itiger01+02 | rep2 | 1.258 / 1.339 s | 1.34 s | 1.75 s | 24.0 / 6.5 | 4.9 ms per round |
| N=4 (3+1), itiger01+02 | rep1 | 2.436 / 2.444 / 2.440 / 2.521 s | 2.52 s | 2.96 s | 24.7 x3 / 8.4 | 18 ms per round |
| N=4 (3+1), itiger01+02 | rep2 | 2.265 / 2.267 / 2.269 / 2.285 s | 2.29 s | 2.81 s | 24.7 x3 / 8.6 | 15 ms per round |
| N=2, naive NFS | rep1, rep2 | 1351 s (all shards) | 1351 s | | 24.0 / 7.9 | 15.0 s per round |

The last column charges the whole excess over the single-node reference to
the barrier; it is an upper bound, since a shard also does its own share of
compute plus fixed per-process costs (on one host that per-shard wall was
0.52 s vs a 0.43 s reference at this size). So the cross-machine NFS barrier
costs roughly 4-5 ms per round for 2 machines (vs ~1.2 ms on a local dir), and
15-18 ms per round for the 3+1 placement where three 8-thread shards shared a
node that was already at load 24 (itiger01 is a shared node; this number mixes
contention and barrier cost). The 4-shard N=4 run writes 3 files and reads 3
files per round per shard (12 NFS creates + renames + 12 reads + 12 unlinks
per round in total) vs 2 + 2 at N=2.

Delta volume (sum of `exchange_bytes_sent` over shards; received totals are
identical), exactly equal on the cluster and on the single host, and equal to
section 12:

| config | rounds | cross-shard deltas | delta bytes | mean per round |
|---|---|---|---|---|
| N=2, 1M | 90 | 643,748 | 22,531,180 B (22.53 MB) | 250 KB |
| N=4, 1M | 90 | 970,704 | 33,974,640 B (33.97 MB) | 377 KB |
| N=4, 1e7 | 90 | 9,720,683 | 340,223,905 B (340.2 MB) | 3.78 MB |

1e7 (finance 1,000,000 nodes / 10,000,000 edges, 8,858,299 edges written,
guarded 4,241,413 / violations 206,541 in the reference):

| where | config | rep | per-shard wall | makespan | single-node reference |
|---|---|---|---|---|---|
| itiger02 | single process | rep1 / rep2 | | | 7.54 / 7.46 s process (7.43 / 7.35 s engine) |
| itiger01 (3) + itiger02 (1) | N=4 poked | rep1 | 9.184 / 9.182 / 9.183 / 9.250 s | 9.28 s | load at start 1.7 x3 / 6.4 |
| itiger01 (3) + itiger02 (1) | N=4 poked | rep2 | 9.226 / 9.223 / 9.223 / 9.322 s | 9.32 s | load at start 2.3 x3 / 7.0 |
| local i9 (load 7-9) | N=4, local dir | 1 rep | 3.79 / 3.77 / 3.79 / 3.79 s | 3.80 s | 6.48 s process (6.42 s engine) |

On one host, N=4 at 1e7 gives a 1.7x makespan speedup over the single run
(3.80 vs 6.48 s; the per-block compute now dominates the ~1 ms barrier). On
the cluster with the NFS exchange dir the same N=4 run is 1.25x SLOWER than
one process on one node (9.3 vs 7.5 s), even though the reference and shard
telemetry are identical: the 90 NFS barriers at ~3.8 MB per round plus the
3-on-one-node placement eat the whole 4-way partition of the compute.
Charging the difference between the cluster makespan (9.3 s) and a per-shard
compute estimate scaled from the local ratio (3.79/6.48 x 7.5 s = 4.4 s) to
the barrier gives ~55 ms per round at 1e7, i.e. the NFS round cost grows with
the per-round delta volume (3.8 MB written via tmp-file + rename and read
back through a 1 MiB rsize mount). Overhead fraction: (9.3 - 4.4) / 9.3 =
~53% of the sharded makespan is barrier/placement overhead at this size on
this filesystem, vs ~0% (3.80 vs 4 x 0.95) on one host.

## Step 6: robustness, missing peer on the cluster

`bin/timeout.slurm`: one node (itiger02) launched ONLY shard 0 of
`--shard-count 2` with `--exchange-timeout-s 30`, exchange dir on NFS.
Result: exit code 1 after 30.18 s wall, empty stdout, stderr:

```
error: shard 0/2: timed out after 30s waiting for the block-0 delta file
`$PROJECT/saga/xrun/timeout/xchg/b0_from1_to0.bin` from peer shard 1.
All 2 shard processes must run concurrently with identical seed/params and
the same --exchange-dir; start the missing peer or raise --exchange-timeout-s.
```

No hang. Leftovers exactly as expected: the exchange dir containing only
shard 0's own outbound block-0 file `b0_from0_to1.bin` (142,835 B, the file
the absent peer would have consumed) and the shard's output directory
skeleton (`shard_00/`, no edge/node files). Both were deleted in cleanup.

## Honest notes

- Nodes were shared with other users' jobs; load averages are given per run.
  itiger01 carried a constant load of ~24 (another user's job) during every
  run that used it, so the N=4 (3+1) numbers include CPU contention.
- MaxNodes=3 forbids 4 shards on 4 nodes; the 3-node (2+1+1) placement hung
  on NFS visibility three times (above) and is not reported as a
  measurement. The cross-machine N=4 result is therefore 3+1 over two
  machines, which still exercises cross-machine delivery for 3 of the 6
  shard pairs.
- The naive-NFS N=2 rep1 outputs were lost: `shard.sh` was overwritten on the
  shared filesystem while that 45-minute job was still reading it (bash reads
  scripts incrementally), so rep1 did not copy its outputs; its JSON telemetry
  (edges per shard, deltas, bytes, 1351.6 / 1351.0 s engine wall) is intact
  and identical to rep2, whose outputs passed the digests.
- The poke sidecar is a measurement workaround, not part of the engine; it
  adds ~200 NFS create/unlink ops per second per node. `x.slurm` and
  `shard.sh` are in the artifact directory.
- Two N=4 1M runs left the EMPTY exchange dir in place (a poke raced the
  engine's best-effort `remove_dir`); zero files remained in every successful
  run.
- `cp -r` of two shard outputs printed `cannot stat .../metrics.prom` while
  copying from node-local /tmp (a file listed but not yet visible to cp);
  `metrics.prom` is not used by the digests and the digests passed.
- No crate was touched; the binary is the locally built one (sha-identical
  outputs to the local host, see step 2).

## Artifacts

Kept (small): cluster `$PROJECT/saga/xrun/` (per-run `shard*.json`,
`shard*.time`, `shard*.err`, `digest.json`, `ref*.json`), `$PROJECT/saga/logs/x*_2020*.out`,
scripts in `$PROJECT/saga/bin/` (`x.slurm`, `shard.sh`, `ref.slurm`,
`timeout.slurm`, `digest.py`, and the staged `agora` binary, 12 MB). A copy
of the summaries and scripts is in `docs/repro/exchange_cluster/` (3.6 MB; the 1e7 shard JSONs there have their `ground_truth` arrays stripped to keep the copy small, the full files stay on the cluster; the `.poke` sidecar noise was removed from the copied SLURM logs). Deleted:
every `out*` directory (parquet graphs, ~800 MB total on the cluster) and
every exchange dir, on the cluster and locally.
