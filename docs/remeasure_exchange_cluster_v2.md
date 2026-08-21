# Re-measurement v2: cross-machine exchange on the iTiger cluster with the NFS-robust discovery (CONSISTENCY_MODEL.md section 16)

Date: 2026-08-21. Supersedes the latency numbers of `docs/remeasure_exchange_cluster.md`
(the naive 2 ms `File::open` poll: 15 s per round on NFS; the measurement-side
"poke" sidecar: 4-5 ms per round; the 2+1+1 placement that hung). This run uses
NO sidecar: the engine's own per-round-directory + readdir discovery (section 16)
does all the work. Every number below is from a real run; the small per-run
JSON/log summaries are kept (see "Artifacts"); every generated graph and every
exchange directory was deleted on the cluster and locally.

Binary: the locally built `target/release/agora` (saga 0.1.0) copied to
`$PROJECT/saga/bin/agora` BEFORE any other work in this session, sha256
`4488f123ff852f3caf4163ad9011feaaf684260d2dfc09e6ab39987b5618a235` (12,220,912 B),
identical on the cluster and in the local staging copy; the section-12 binary
was kept beside it as `agora_sec12` (sha256 `0f787418...`). Smoke test on
itiger02 (Rocky 9.4, glibc 2.34, job 202104): `--version` = saga 0.1.0,
`generate --help` lists `--shard-count/--shard-index/--exchange-dir/
--exchange-timeout-s`, `--json` emits `exchange_idle_polls`, `exchange_nudges`,
`exchange_skewed_opens` (all 0 in a non-sharded run), and a tiny N=2 on a
node-local dir gave 90 rounds, 45/46 idle polls, 0 nudges, 0 skewed opens,
exchange dir removed.

## Setup

- Cluster, nodes, mount: as in the v1 note (University of Memphis iTiger,
  partition bigTiger, SLURM; per-job MaxNodes=3 = `MaxTRES node=3`, MaxJobs=6
  per user shared with two unrelated running jobs of the same account; nodes
  AMD EPYC 7513/9334 32-core, 64 threads, kernel 5.14.0-427.31.1.el9_4,
  Rocky 9.4; `/project` = NFSv4.2 `hard,proto=tcp,rsize=wsize=1 MiB,
  timeo=600,retrans=2,acdirmin=30,acdirmax=60`, server `<nfs-server>`
  <private-ip>, 200 Gb/s IB). Every task: 8 cpus, `--threads 8`, 4 GB per cpu.
- Commands: exactly the section-16 recipe (`bin/x16.slurm` + `bin/shard16.sh`,
  `bin/ref.slurm`, `bin/timeout16.slurm`, `bin/digest.py`, `bin/smoke16.slurm`,
  `bin/nfsprobe.slurm`, all copied to `docs/repro/exchange_cluster_v2/bin/`).
  Each shard: `agora generate --domain finance --nodes N0 --edges E0 --seed 42
  --guards --block-len-days 1 --threads 8 --json --quiet --shard-count N
  --shard-index $SLURM_PROCID --exchange-dir $PROJECT/saga/xrun/<tag>/rep<r>/xchg
  --exchange-timeout-s TO --out /tmp/...`, all N tasks started by one
  `srun --nodes=NN --ntasks=N --cpus-per-task=8 --distribution=block`. Outputs
  written to node-local `/tmp`, copied to NFS for the digests, then deleted.
- Per-shard wall = `date +%s.%N` around the process on its own node; makespan
  = max(end) - min(start) across nodes (NTP-synced, start skew 40-130 ms);
  srun wall is an upper bound that includes task launch.
- Placements actually measured (node, shards on it):
  N=2: itiger01 (1) + itiger08 (1). N=4 2+1+1: itiger01 (2) + itiger02 (1)
  + itiger08 (1). N=4 3+1: itiger01 (3) + itiger02 (1). itiger01 carried an
  unrelated load of 16.8-19.4 during every run that used it; itiger02/08 sat
  at load 4.7-6.5 (1-minute load average at shard start, recorded per run).
- Why these nodes: the first N=2 and 2+1+1 submissions were placed by SLURM
  on itiger02+03 and itiger01-03 and both timed out at round 0; the retries
  landed on itiger07+08 / itiger06-08 and would have failed the same way. The
  cause is not the exchange protocol, see "The itiger03 / itiger07 finding".
  All reported runs were then pinned with `-w` to nodes whose NFS client
  passed a metadata-latency probe.

## Step 2: single-node reference (new binary, itiger02, job 202105/202106)

| size | rep | process wall | engine wall | edges | edge digest | node digest |
|---|---|---|---|---|---|---|
| 1M (100k nodes / 1M edges) | rep1 | 0.543 s | 0.479 s | 887,156 | 36ca881b... | bf8c5d0e... |
| 1M | rep2 | 0.570 s | n/a | 887,156 | 36ca881b... | bf8c5d0e... |
| 1e7 (1M nodes / 10M edges) | rep1 | 5.393 s | 5.326 s | 8,858,299 | 562786dc... | 6f128c73... |
| 1e7 | rep2 | 5.355 s | 5.253 s | 8,858,299 | 562786dc... | 6f128c73... |

The 1M digests equal the recorded `36ca881b9b98755ebccb017d269b6fc6f96c92f97c7cf16e5d294ca48ac10865` /
`bf8c5d0e4892d38cd476992e2af7df9fed07c7fd07bd30bbd878583ad61454c9` (v1 note,
section 12, and the local host): the pipelined engine is bit-identical to the
old one and across machines. The reference is faster than with the v1 binary
(0.90 s and 7.5 s then, on loaded nodes; 0.54 s and 5.4 s now on itiger02 at
load 6): the 1e7 comparison below therefore uses the NEW reference. Local
desktop (i9-13900K, load 5-6), same binary: 0.307 / 0.308 s at 1M, 2.90 s at
1e7.

## Equivalence (Theorem 2 across real machines), all completed runs

Every completed run: union of the N shards' edge multisets == reference
(887,156 edges, digest 36ca881b...; 1e7: 8,858,299, digest 562786dc...), union
of node-state rows == reference (100,000 / 1,000,000 rows, digest bf8c5d0e... /
6f128c73...), shard node-id sets pairwise disjoint, every shard's rows a
subset of the reference (slice check). Per-shard digests are identical to the
v1 cluster run and the local host (N=2 edges 1e67846f... / d9e02a12..., nodes
0aafd961... / 64c14b39...; N=4 edges 8943896f... / bf59ee81... / 13132c87... /
133741db...). Shard edge counts: N=2 438,406 + 448,750; N=4 215,178 + 227,885 +
223,228 + 220,865; 1e7 N=4 2,213,289 + 2,231,847 + 2,209,316 + 2,203,847.

| config | placement | rep | union edges == ref | node union == ref | slices | disjoint | xchg dir after run |
|---|---|---|---|---|---|---|---|
| N=2 1M | itiger01 + itiger08 | rep1, rep2 | PASS 887,156 | PASS 100,000 | PASS | PASS | removed (0 entries) |
| N=4 1M, 2+1+1 | itiger01 x2 + itiger02 + itiger08 | rep1, rep2 | PASS 887,156 | PASS 100,000 | PASS | PASS | removed |
| N=4 1M, 3+1 | itiger01 x3 + itiger02 | rep1 | PASS 887,156 | PASS 100,000 | PASS | PASS | removed |
| N=4 1e7, 2+1+1 | itiger01 x2 + itiger02 + itiger08 | rep1, rep2 | PASS 8,858,299 | PASS 1,000,000 | PASS | PASS | removed |
| N=4 1e7, 3+1 | itiger01 x3 + itiger02 | rep1 | PASS 8,858,299 | PASS 1,000,000 | PASS | PASS | removed |

Delta volume, identical to v1 and section 12: N=2 643,748 deltas / 22,531,180 B
over 90 rounds; N=4 970,704 / 33,974,640 B; N=4 1e7 9,720,683 / 340,223,905 B
(3.78 MB per round in total, ~0.9-1.1 MB written and read per shard per round).

## Per-shard wall, makespan, telemetry (1M, 90 rounds)

idle / nudges / skewed = `exchange_idle_polls` / `exchange_nudges` /
`exchange_skewed_opens` per shard (shard order 0,1,...). load = 1-minute load
average of the shard's node at its start.

| config | rep | shard host (load) | per-shard process wall | makespan | srun wall | idle polls | nudges | skewed opens |
|---|---|---|---|---|---|---|---|---|
| N=2 | rep1 | itiger01 (16.8) / itiger08 (6.4) | 1.094 / 1.149 s | 1.149 s | 1.51 s | 65 / 30 | 2 / 1 | 0 / 0 |
| N=2 | rep2 | itiger01 (16.8) / itiger08 (6.4) | 1.241 / 1.305 s | 1.305 s | 1.67 s | 47 / 51 | 4 / 6 | 0 / 0 |
| N=4 2+1+1 | rep1 | itiger01 (16.8) x2 / itiger02 (5.5) / itiger08 (6.4) | 2.048 / 2.050 / 2.141 / 2.171 s | 2.171 s | 2.58 s | 15 / 19 / 349 / 38 | 9 / 18 / 131 / 14 | 0 x4 |
| N=4 2+1+1 | rep2 | itiger01 (16.9) x2 / itiger02 (5.6) / itiger08 (6.4) | 1.454 / 1.453 / 1.531 / 1.526 s | 1.538 s | 1.96 s | 3 / 3 / 254 / 75 | 2 / 1 / 26 / 13 | 0 x4 |
| N=4 3+1 | rep1 | itiger01 (6.3) x3 / itiger02 (7.2) | 1.797 / 1.802 / 1.801 / 1.905 s | 1.907 s | 2.31 s | 0 / 0 / 1 / 278 | 0 / 0 / 0 / 81 | 0 x4 |

`exchange_skewed_opens` was 0 in every shard of every run (26 shard-runs,
2,340 rounds): on these clients a name returned by the listing always opened.
Nudges are the slow path (a poll that found nothing after >= 4 ms of waiting):
they concentrate on the shard that sits alone on an unloaded node and waits
for the shards on loaded itiger01 (e.g. 131 nudges = at most 1.5 per round on
itiger02 in 2+1+1 rep1; 81 on itiger02 in 3+1), and are near zero on the
shards that arrive last. At N=2 they are 1-6 per 90 rounds.

## Per-round latency: naive / sidecar / new discovery / local

Upper bound that charges the whole excess of the makespan over the
single-process reference to the 90 barriers: (makespan - ref) / 90, ref =
0.543 s on the cluster (new binary), 0.307 s locally. It also contains the
per-process fixed cost and the shards' own compute imbalance, so the true
barrier cost is smaller.

| mechanism | where | N / placement | makespan | per round (upper bound) |
|---|---|---|---|---|
| naive 2 ms `open()` poll (v1) | NFS, itiger01+02 | N=2 | 1,351 s | 15.0 s |
| 2 ms poll + poke sidecar every 5 ms (v1) | NFS, itiger01+02 | N=2 | 1.30 / 1.34 s (ref 0.90) | 4.4 / 4.9 ms |
| 2 ms poll + poke sidecar (v1) | NFS, itiger01 x3 + 02 | N=4 3+1 | 2.52 / 2.29 s (ref 0.90) | 18 / 15 ms |
| section-16 discovery, no sidecar (this run) | NFS, itiger01+08 | N=2 | 1.149 / 1.305 s | 6.7 / 8.5 ms |
| section-16 discovery | NFS, itiger01 x2 + 02 + 08 | N=4 2+1+1 | 2.171 / 1.538 s | 18.1 / 11.1 ms |
| section-16 discovery | NFS, itiger01 x3 + 02 | N=4 3+1 | 1.907 s | 15.2 ms |
| section-16 discovery | local disk, i9 desktop | N=2 | 0.357 / 0.386 s | 0.56 / 0.87 ms |
| section-16 discovery | local disk, i9 desktop | N=4 | 0.404 / 0.416 s | 1.08 / 1.21 ms |

Reading: the new engine-side discovery brings the cross-machine NFS barrier
from 15 s per round to single-digit milliseconds at N=2 (6.7-8.5 ms) with no
help from outside the engine, i.e. into the same regime the v1 sidecar reached
(4.4-4.9 ms), not below it. The section-16 layout does more NFS metadata
operations per round than the flat layout (mkdir of the round dir, a readdir
per poll, rmdir by the last consumer, plus the 1 ms initial backoff sleep), and
on the healthy clients one metadata operation costs ~2 ms from a shell
(`bin/nfsprobe.slurm`: mkdir / create / rename / readdir / unlink each 2-3 ms
on itiger01/02/06/08/11, 5-10 ms on itiger09/10), so 3-4 extra round-trips
per round account for the difference to the sidecar number. The v1 sidecar
numbers were measured against a slower reference (0.90 s), so their absolute
makespans (1.30 / 1.34 s) are in fact the same as today's (1.15 / 1.31 s). The
N=4 numbers on the cluster are dominated by node sharing: the three or two
shards on itiger01 (load 17-19) finish last in every run, the lone shard on
itiger02 does all the idle polling and nudging, and 2+1+1 rep1 vs rep2 (2.17 vs
1.54 s, identical telemetry) shows the run-to-run noise of that node.

## The 2+1+1 verdict

It completes. Two reps of N=4 over three nodes (itiger01 x2 + itiger02 +
itiger08): 2.17 s and 1.54 s makespan, 90 rounds per shard, exchange dir
removed, all digests PASS, 0 skewed opens. The v1 attempts (itiger01-03 twice,
itiger06-08 once) that "hung on stale negative dentries" were, in hindsight,
runs that included itiger03 or itiger07; see the next section. With the new
binary, the same bad placements (itiger02+03, itiger01-03) still fail, cleanly
and for a reason the engine reports correctly (timeout at round 0, the peer's
file named), because the peer on the sick node had not published the file
yet: it is a node problem, not a discovery problem, and it is also not
something the section-16 primitive (or any discovery primitive) can fix.

## The itiger03 / itiger07 finding (why the first submissions failed)

Job 202107 (N=2, SLURM-chosen itiger02+03) and 202108 (N=4 2+1+1, itiger01-03)
both ended with `timed out after 120s waiting for the block-0 delta file
.../r0/b0_from1_to0.bin from peer shard 1` (resp. `b0_from3_to{0,1,2}.bin`
from shard 3), the shard on itiger03 in both cases. NFS server-side mtimes
(`find -printf %T@` from the login node) of what the itiger03 shard did:

- its `shard16.sh` wrote `loadavg_start` 66 s after the other tasks (02:38:50
  vs 02:37:45), i.e. `mkdir -p` + one `cat >` on NFS took about a minute
  before `agora` even started; `agora`'s stdout redirect file appeared 21 s
  later (02:39:12);
- its round-0 outbound files landed at 02:40:22.877, 02:40:44.32, 02:41:06.67
  (one every ~22 s; in the N=2 job the single outbound file landed at exactly
  02:40:22.877 as well), i.e. 70 s after the process started, 17 s AFTER the
  peers' 120 s timeouts had fired at 02:40:05 / 02:39:45;
- the itiger03 shard itself consumed the peers' round-0 files without trouble,
  published round 1 (02:41:06) and then timed out cleanly 120 s later because
  the peers were gone (`shard1.time`: rc 1, 271.9 s).

`bin/nfsprobe.slurm` (one task, shell-level timing of single NFS metadata
operations in a fresh directory) then gave, per node:

| node | load | mkdir | create (touch) x3 | rename x3 | readdir | unlink x3 | rmdir |
|---|---|---|---|---|---|---|---|
| itiger01 | 18.4 | 4 ms | 2 / 2 / 2 ms | 2 / 2 / 2 ms | 2 ms | 2 ms | 2 ms |
| itiger02 | 6.4 | 3 ms | 2 / 2 / 2 ms | 2 / 2 / 2 ms | 2 ms | 2 ms | 2 ms |
| itiger06 | 27.5 | 4 ms | 3 / 2 / 2 ms | 3 / 2 / 2 ms | 3 ms | 3 ms | 2 ms |
| itiger08 | 6.2 | 3 ms | 2 / 2 / 2 ms | 2 / 2 / 2 ms | 2 ms | 2 ms | 2 ms |
| itiger09 | 7.0 | 7 ms | 6 / 5 / 5 ms | 7 / 5 / 5 ms | 6 ms | 5 ms | 5 ms |
| itiger10 | 8.3 | 10 ms | 8 / 8 / 8 ms | 8 / 8 / 7 ms | 9 ms | 8 ms | 7 ms |
| itiger11 | 1.0 | 3 ms | 3 / 2 / 2 ms | 2 / 2 / 2 ms | 2 ms | 2 ms | 2 ms |
| itiger07 | 3.9 | 3.33 s | 3.74 / 2.84 / 21.55 s | 1.25 / 0.65 / 1.69 s | 1.24 s | 1.27 s | 0.84 s |
| itiger03 | 6.5 | 1.25 s | 21.06 / 20.62 / 20.84 s | 0.43 / 1.10 / 0.49 s | 0.50 s | 0.48 s | 0.32 s |

Same kernel, same mount options and same server address on every node;
node-local `/tmp` is 1-2 ms everywhere including itiger03/07. On itiger03 a
file CREATE costs ~21 s and every other metadata operation 0.3-1.2 s; itiger07
is the same pattern, milder (creates 2.8-21.5 s). That is the ~22 s cadence of
the itiger03 shard's publishes (each publish = create temp + write + rename)
and why the v1 note's 2+1+1 attempts (itiger01-03, itiger06-08) "progressed
one block per 3-4 minutes" with the 2 ms poll: 4 shards x (create + ...) per
round at 21 s per create. The v1 diagnostic "`ls` lists the file while `wc -c`
returns ENOENT on itiger03" was taken on that sick client. The two
itiger03/07 probe jobs also needed 1-2 minutes to write a 2 KB log. This is a
per-node NFS client (or its server session) pathology that a user cannot fix;
it was reported here for the operators' benefit and the placement was pinned
away from it. All the numbers in this note are from itiger01/02/08.

## 1e7: does cross-machine sharding pay now?

| config | rep | shard host (load) | per-shard wall | makespan | srun wall | idle / nudges / skewed |
|---|---|---|---|---|---|---|
| single process, itiger02 | rep1 / rep2 | itiger02 (6.1) | | 5.393 / 5.355 s | | 0 |
| N=4 2+1+1 | rep1 | itiger01 (18.9) x2 / itiger02 (6.4) / itiger08 (6.2) | 7.499 / 7.501 / 7.607 / 7.672 s | 7.719 s | 9.15 s | 12,15,537,178 / 12,14,290,164 / 0 |
| N=4 2+1+1 | rep2 | itiger01 (19.4) x2 / itiger02 (6.5) / itiger08 (6.5) | 7.492 / 7.494 / 7.573 / 7.608 s | 7.629 s | 9.15 s | 87,96,597,65 / 77,93,402,61 / 0 |
| N=4 3+1 | rep1 | itiger01 (18.6) x3 / itiger02 (4.7) | 8.029 / 8.030 / 8.032 / 8.093 s | 8.095 s | 9.64 s | 6,11,11,536 / 6,8,10,287 / 0 |
| local desktop, single | | i9 (load 6.5) | | 2.904 s | | |
| local desktop, N=4, local dir | | i9 | 2.631 / 2.628 / 2.631 / 2.629 s | 2.633 s | | 163,151,152,127 / 14,12,15,14 / 0 |

Verdict: no. On this cluster, N=4 at 1e7 over three machines is 1.42-1.43x
SLOWER than one process on one node (7.63-7.72 s vs 5.36-5.39 s); the 3+1
placement is 1.50x slower (8.10 s). The absolute sharded makespans improved
against v1 (9.28 / 9.32 s with the sidecar, 3+1), but the single-process
engine improved more (7.5 -> 5.4 s), so the ratio got worse (v1: 1.25x). The
per-shard engine wall (7.4-7.6 s for a quarter of the events) says where the
time goes: charging everything beyond a perfect quarter of the reference
(5.39 / 4 = 1.35 s) to the barriers gives an upper bound of ~70 ms per round
at 1e7, versus 7-18 ms at 1M: the round cost on this filesystem grows with the
per-round delta volume (3.78 MB per round in total, each shard writing ~1 MB
through create + write + rename and reading ~1 MB, synchronously inside the
barrier, through a 1 MiB rsize/wsize NFS mount), plus the load-17-19 node
holding two or three of the shards. The barrier telemetry is healthy (0
skewed opens; the lone shard on itiger02 nudges up to 4.5 times per round
while waiting for the itiger01 shards, the itiger01 shards almost never wait),
so this is bandwidth and placement, not discovery. On the desktop with a
local-disk exchange dir the same N=4 run is only 1.10x faster than the single
process (2.63 vs 2.90 s; 4 x 8 threads on a 32-thread host, so the single
process already has the machine), which is a different limit (CPU), not the
barrier.

## Timeout behaviour (lone shard, job 202110, itiger02)

`bin/timeout16.slurm`: ONLY shard 0 of `--shard-count 2`, `--exchange-timeout-s 30`,
exchange dir on NFS. Exit code 1 after 30.21 s wall, empty stdout, stderr:

```
error: shard 0/2: timed out after 30s waiting for the block-0 delta file
`$PROJECT/saga/xrun/timeout16/xchg/r0/b0_from1_to0.bin` from peer shard 1.
All 2 shard processes must run concurrently with identical seed/params and
the same --exchange-dir; start the missing peer or raise --exchange-timeout-s.
```

Leftovers exactly as section 16 documents: `xchg/r0/b0_from0_to1.bin`
(142,835 B, the shard's own outbound round-0 file), nothing else in `xchg/`
(no nudge file, no temp file), and the output dir skeleton `shard_00/` without
edge/node files. The real failures above (jobs 202107/202108, 120 s timeout)
behaved identically: the message names the round file and peer, and the
leftovers were the published-but-unconsumed files of the surviving peers
(`r0/b0_from1_to0.bin`, `r1/b1_from1_to0.bin` resp. `r0/b0_from3_to{0,1,2}.bin`,
`r1/b1_from3_to{0,1,2}.bin`), which the other side had published into the
round dirs late.

## Honest notes

- Node sharing: itiger01 carried an unrelated load of 16.8-19.4 (v1: ~24)
  during every run that used it, itiger02/08 sat at 4.7-6.5 and an unrelated
  job of this account ran on itiger02 throughout. Every N=4 number mixes that
  contention with the barrier cost; the 2+1+1 rep1/rep2 spread (2.17 vs
  1.54 s) is that noise. N=2 (itiger01 + itiger08) is the cleanest
  cross-machine number.
- The per-round figures are upper bounds ((makespan - single-process
  reference) / 90). The v1 sidecar numbers were computed against the slower
  v1 reference; by absolute makespan the new discovery and the v1 sidecar are
  equal at N=2 (1.15-1.31 s vs 1.30-1.34 s).
- Placement had to be pinned (`-w`) to healthy nodes after SLURM twice chose a
  node whose NFS client takes ~21 s per file create (itiger03; itiger07 is
  similar). Four jobs were cancelled for that reason (202107, 202108 after
  rep1 had timed out cleanly; 202117, 202118 before their first round
  completed). The cancelled jobs' node-local `/tmp` scratch was swept
  afterwards with one small job per node (itiger01/02/03/06/07/08: nothing
  was left to delete, the shard scripts' own `rm -rf` had run). This also reinterprets the v1
  note's 2+1+1 failures as a sick-node effect rather than a cache-coherence
  effect, and the v1 claim "why two-node runs are immune and three-node runs
  are not was not resolved" is resolved: the two-node runs never included
  itiger03/07.
- NFS caching observations on the healthy clients: 0 skewed opens in 26
  shard-runs (a listed name always opened), nudges only on the shard that
  waits for a loaded peer, round dirs always removed, exchange dir always
  removed (no race with a sidecar this time). No 30 s `acdirmin` stall was
  observed anywhere; whether the nudge or the fresh round dir is what keeps
  the listings fresh cannot be separated from these runs (both are always on).
- The 1e7 reference and the 1e7 N=4 runs were on different node sets
  (reference on itiger02 alone, which is also one of the shard nodes); the
  reference was re-measured with the new binary as required.
- Digest time (pyarrow, login-side python) is 17 s per 1M run and 190 s per
  1e7 run and is not part of any timing.
- No crate, paper or artifact directory was touched; nothing was committed.

## Artifacts

Kept (small): on the cluster `$PROJECT/saga/xrun/{ref16,ref16_1e7,
timeout16,x16_n2,x16_n4_3n,x16_n4_2n,x16b_n2,x16b_n4_3n,x16c_n2,x16c_n4_3n,
x16c_n4_1e7_3n,x16c_n4_1e7_2n,nfsprobe}` (per-run `shard*.json` with full
`ground_truth`, `shard*.time`, `shard*.err`, `shard*.loadavg_start`,
`digest.json`, `ref*.json`; 30 MB in total), `$PROJECT/saga/logs/
{smoke16,ref16*,timeout16,x16*,nfsprobe_*}_2021*.out`, and the scripts plus
the staged binaries in `$PROJECT/saga/bin/` (`agora` = this run,
`agora_sec12` = v1). A copy of all summaries, logs and scripts is in
`docs/repro/exchange_cluster_v2/` (2.6 MB, 211 files; the 1e7 shard/ref JSONs
have `ground_truth` replaced by a marker string, the full files stay on the
cluster); `local/` there holds the desktop reference and 1e7 shard JSONs.
Deleted: every `out*` directory on the cluster (~1.0 GB of parquet graphs),
every exchange dir (including the leftovers of the timed-out runs and the
timeout test), the node-local `/tmp/saga_*` scratch on itiger01/02/03/06/
07/08, and every local graph output and exchange dir under the scratchpad.
