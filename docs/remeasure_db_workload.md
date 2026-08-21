# Re-measurement: database workload (Kuzu) on SAGA temporal graphs

Date: 2026-08-21. Host: xzgpu2 (i9-13900K, 24P/8E cores, 32 logical, 62 GiB RAM, NVMe).
Binary: frozen `./target/release/agora` (saga 0.1.0), not rebuilt. Seed 42 everywhere.
Scripts and small result files: `docs/repro/db_workload/` (`run_all.sh` is the driver;
`results/*.json` are the raw per-dataset measurements, `results/summary.csv|md` the
tables below, `results/struct_stats.json` the structural properties, `results/workload.log`
the console log with loadavg per timed block).

## Engine

* **Kuzu 0.11.3** (embedded, `pip install --user kuzu`, Python 3.10.12 bindings). No server.
  Default thread count (32 logical cores). Database is a single file (plus `.wal`); the
  on-disk size reported is the file size after `CHECKPOINT` and close.
* Neo4j: no server was running locally (ports 7474/7687 closed; no `neo4j`/`cypher-shell`
  binaries), and per the protocol none was installed. DuckDB 1.5.3 was used only to
  canonicalize CSVs (untimed); it is not the database under test.

## Schema (one node table with a type column, one edge table)

```
CREATE NODE TABLE Node(id INT64, type STRING, attrs STRING, PRIMARY KEY(id))
CREATE REL TABLE Edge(FROM Node TO Node, t INT64, event_type STRING, label STRING, amount DOUBLE)
COPY Node FROM '<dir>/nodes.csv' (HEADER=true)
COPY Edge FROM '<dir>/edges.csv' (HEADER=true)
```

`attrs` packs the remaining node columns as `k=v;k=v;...` (SAGA: subtype, jurisdiction,
kyc_level, risk_tier, age_days, balance, txn_count, cum_in, cum_out; note the agora
`nodes.csv` is ragged for `merchant`/`institution` rows, so for those rows the packed keys
are positional rather than semantic; `attrs` is an opaque string for Q5 and this does not
affect timing). `t` is unix seconds. `amount` is the first numeric edge attribute (SAGA:
`amount`; LDBC: post/comment `length`; BA/ER: NULL). `label` is the SAGA ground-truth
edge label (`normal` or the anomaly intent); NULL for BA/ER/LDBC.

## Datasets

| name | how built | nodes | edges |
|---|---|---:|---:|
| saga_1m | `agora generate --domain finance --edges 1000000 --seed 42 --format csv` (guards off, default 100k-node scaffold, 90-day span) | 100,000 | 998,371 |
| saga_1m_guards | same plus `--guards --block-len-days 1` | 100,000 | 887,156 |
| saga_10m | `--edges 10000000`, same scaffold (100k nodes) | 100,000 | 9,983,083 |
| ba_1m | `saga_eval.baselines.gen_ba(n=100000, m_edges=998371)`: Barabasi-Albert (m=10), random orientation, uniform random timestamps over the same 90-day span | 100,000 | 999,900 |
| er_1m | `saga_eval.baselines.gen_er(n=100000, m=998371)`: directed G(n,m), uniform random timestamps | 100,000 | 998,371 |
| ldbc_sf1 | LDBC SNB Interactive v1 SF1 official CSVs (see below): person-knows-person (t = knows.creationDate) plus person-hasCreator-post and person-hasCreator-comment "creation" edges (t = post/comment creationDate); nodes = person + post + comment | 3,065,666 | 3,236,397 |

The guarded run has 11% fewer edges than the guards-off run (the overdraft guard suppresses
events); it is produced in the same schema and loads through the same path, which is what the
"loads identically" row is meant to show.

### LDBC SNB status: obtained

`https://datasets.ldbcouncil.org/snb-interactive-v1/social_network-sf1-CsvBasic-LongDateFormatter.tar.zst`
(230 MB) downloaded in 5 s from this host; only the six needed `dynamic/` files were extracted.
Counts: person 9,892; post 1,003,605; comment 2,052,169; knows 180,623; post_hasCreator
1,003,605; comment_hasCreator 2,052,169. Timestamps are epoch milliseconds in the source and
were divided by 1000 to unix seconds; the span is 2010-01-03 to 2012-09-13 (983 days). All
windows in the workload are placed relative to each dataset's own span (see below), so the
same queries run unchanged. Caveat on like-for-likeness: LDBC's edge set is 3.2x SAGA-1M
in edges and 30x in nodes, and 94% of its edges (hasCreator) point to post/comment nodes that
have no out-edges, so its 2-hop (Q2) frontier is much smaller per edge than SAGA's.

## Workload (Cypher, Kuzu dialect), verbatim as executed on saga_1m

Sampling: Q1/Q2 sources drawn with `numpy.default_rng(42)` from the dataset's distinct source
ids; Q4 pairs are 100 distinct (src,dst) pairs drawn with DuckDB `reservoir(100) REPEATABLE (42)`
from the dataset's distinct pairs. The 7-day window for Q1/Q2 starts at `t_min + 0.4*span`
(saga_1m: 1738799948), the 1-day window for Q3 at `t_min + 0.5*span` (1739577531); `t_min` = 1735689619.
Every dataset uses its own sampled ids and window bounds; the query text is otherwise identical.
Timing: warm, same process and connection; 1 warm-up then 5 runs, median kept; that block run
twice and the faster median reported. Latency includes fetching all result rows.

Q1 (time-window 1-hop out-degree, 100 sources):
```
UNWIND [39349, 84594, 60295, 41992, 40520, 70023, 13889, 29363, 14872, 64552, 16440, 44778, 38943, 74017, 33372, 62709, 74926, 18137, 79847, 87041, 20460, 82991, 80337, 11515, 14467, 75521, 71606, 17068, 68383, 12568, 21585, 49888, 63032, 47344, 49045, 25981, 81487, 70693, 57941, 69126, 8276, 75763, 49165, 34890, 39884, 42807, 56716, 6119, 68465, 20441, 58890, 80840, 61441, 33025, 40093, 41663, 83335, 46152, 66181, 66917, 36213, 33383, 69593, 6869, 57348, 61421, 74461, 7723, 24928, 8738, 87306, 68208, 3959, 77198, 63018, 39183, 77201, 17530, 32782, 5753, 8462, 44988, 87717, 39463, 28159, 72439, 56856, 60984, 42247, 70281, 36986, 31917, 67043, 11687, 61776, 29755, 8019, 50813, 40508, 74958] AS sid
MATCH (a:Node {id: sid})-[e:Edge]->(b:Node)
WHERE e.t >= 1738799948 AND e.t < 1739404748
RETURN sid, count(e) AS out_deg_7d
```
Q2 (time-respecting 2-hop reachability, 20 sources, t2 > t1 inside the window):
```
UNWIND [66352, 68792, 23743, 41177, 40242, 7133, 86540, 64563, 71033, 33000, 24532, 23956, 40452, 62981, 70090, 44552, 23529, 87215, 81778, 8668] AS sid
MATCH (a:Node {id: sid})-[e1:Edge]->(b:Node)-[e2:Edge]->(c:Node)
WHERE e1.t >= 1738799948 AND e1.t < 1739404748 AND e2.t > e1.t AND e2.t < 1739404748
RETURN sid, count(DISTINCT c.id) AS reach_2hop
```
Q3 (top-10 in-degree hubs in a 1-day window):
```
MATCH (a:Node)-[e:Edge]->(b:Node)
WHERE e.t >= 1739577531 AND e.t < 1739663931
RETURN b.id AS node, count(e) AS in_deg
ORDER BY in_deg DESC, node ASC LIMIT 10
```
Q4 (recurrence of 100 sampled (src,dst) pairs over the whole span):
```
UNWIND [[65915, 589], [62290, 98000], [77162, 91748], [43849, 0], [87189, 90007], [37963, 98002], [76969, 98004], [81381, 90000], [25205, 90618], [4065, 90320], [42286, 90003], [5764, 61861], [54373, 34296], [77104, 90000], [46495, 90078], [51552, 90008], [22221, 90006], [85313, 90010], [65257, 90201], [31483, 34752], [3119, 90018], [69933, 98001], [13076, 98072], [69894, 98102], [34265, 49173], [80959, 90114], [13381, 16680], [24803, 19963], [21479, 90007], [62991, 90037], [27775, 90344], [41743, 94153], [60117, 29092], [50882, 90054], [32779, 58841], [48146, 11765], [45394, 90014], [5305, 28431], [19203, 0], [35680, 90187], [17681, 13369], [34192, 16314], [64248, 90000], [65862, 90003], [21382, 90003], [56890, 90002], [71679, 90001], [64312, 83651], [83157, 90000], [63010, 98084], [49549, 90149], [24016, 44766], [9714, 38133], [13703, 96475], [83795, 90006], [51737, 49219], [69127, 90024], [9222, 91204], [77159, 90003], [7311, 10293], [1386, 90023], [77133, 91470], [2155, 90239], [75223, 90009], [41201, 90055], [48356, 98000], [59579, 15684], [68094, 90776], [34293, 90124], [52570, 201], [88605, 92207], [30004, 90000], [3928, 90062], [60769, 90001], [35727, 90000], [89246, 98362], [60980, 8591], [18899, 90014], [48610, 90000], [70362, 5712], [10900, 9943], [34233, 365], [51967, 98000], [70064, 19879], [18156, 1214], [42038, 1627], [6306, 748], [14412, 98021], [71277, 51147], [10746, 90001], [1700, 90000], [63319, 90001], [56535, 90388], [46930, 94129], [6056, 90001], [17535, 98002], [87974, 90001], [56519, 90004], [24332, 90036], [21478, 90172]] AS pr
MATCH (a:Node {id: pr[1]})-[e:Edge]->(b:Node {id: pr[2]})
RETURN pr[1] AS src, pr[2] AS dst, count(e) AS repeats
```
Q5 (labeled subgraph retrieval joined to endpoint attributes; N/A for BA/ER/LDBC, which have no labels):
```
MATCH (a:Node)-[e:Edge]->(b:Node)
WHERE e.label <> 'normal'
RETURN a.id, a.type, a.attrs, b.id, b.type, b.attrs, e.t, e.event_type, e.label, e.amount
```
Q6 (per-day edge counts over the span):
```
MATCH (a:Node)-[e:Edge]->(b:Node)
RETURN (e.t - 1735689619) / 86400 AS day, count(e) AS n_edges
ORDER BY day
```

## Results

Load time = fresh database, schema + COPY nodes + COPY edges + CHECKPOINT + close, faster of
two fresh loads. `loadavg` = 1-min load read immediately before the faster timed block.

| dataset | nodes | edges | span (days) | CSV MB | load total s | nodes s | edges s | DB on disk MB | loadavg |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| saga_1m | 100,000 | 998,371 | 90.0 | 62.7 | 0.83 | 0.17 | 0.55 | 57.5 | 3.72 |
| saga_1m_guards | 100,000 | 887,156 | 90.0 | 57.4 | 0.78 | 0.16 | 0.51 | 54.1 | 6.05 |
| ba_1m | 100,000 | 999,900 | 90.0 | 30.3 | 0.59 | 0.07 | 0.42 | 28.0 | 5.88 |
| er_1m | 100,000 | 998,371 | 90.0 | 30.9 | 0.58 | 0.07 | 0.41 | 27.8 | 5.73 |
| saga_10m | 100,000 | 9,983,083 | 90.0 | 492.2 | 5.6 | 0.16 | 5.29 | 351.2 | 5.39 |
| ldbc_sf1 | 3,065,666 | 3,236,397 | 983.4 | 215.2 | 1.09 | 0.5 | 0.46 | 213.5 | 5.0 |

| dataset | Q1 ms (rows) | Q2 ms (rows) | Q3 ms (rows) | Q4 ms (rows) | Q5 ms (rows) | Q6 ms (rows) | loadavg |
|---|---:|---:|---:|---:|---:|---:|---:|
| saga_1m | 29.5 (42) | 34.0 (0) | 9.9 (10) | 105.0 (100) | 43.8 (1226) | 24.9 (90) | 6.22 |
| saga_1m_guards | 27.2 (30) | 30.6 (1) | 9.6 (10) | 100.3 (100) | 40.0 (977) | 24.6 (90) | 6.05 |
| ba_1m | 32.3 (36) | 107.6 (6) | 12.1 (10) | 125.0 (100) | N/A | 28.1 (90) | 5.88 |
| er_1m | 31.4 (14) | 57.9 (0) | 12.5 (10) | 121.9 (100) | N/A | 27.8 (90) | 5.73 |
| saga_10m | 48.9 (90) | 510.3 (11) | 58.7 (10) | 356.0 (100) | 278.6 (12948) | 151.7 (90) | 5.08 |
| ldbc_sf1 | 20.6 (22) | 22.2 (1) | 15.1 (10) | 158.8 (100) | N/A | 22.3 (983) | 7.4 |

| metric | SAGA 1M | SAGA 10M | ratio (10M/1M) |
|---|---:|---:|---:|
| edges | 998,371 | 9,983,083 | 10.00 |
| load total s | 0.83 | 5.60 | 6.74 |
| load edges s | 0.55 | 5.29 | 9.58 |
| DB on disk MB | 57.5 | 351.2 | 6.11 |
| Q1 ms | 29.5 | 48.8 | 1.66 |
| Q3 ms | 9.9 | 58.7 | 5.93 |
| Q6 ms | 24.9 | 151.7 | 6.10 |


Per-run spread: the two independent query blocks agreed within 5% for every (dataset, query)
pair except Q3 on ldbc_sf1 (15.1 vs 20.4 ms) and Q2 on ba_1m (107.6 vs 130.7 ms); the faster
median is reported. Full per-run numbers are in `results/*.json` (`query_runs`, `loads`).

### Structural properties (from `struct_stats.py`, over the canonical edge files)

| dataset | max out-deg (x mean) | max in-deg (x mean) | top-1% in-degree share | distinct-pair ratio | repeat-edge fraction | burstiness B | busiest day / mean day |
|---|---:|---:|---:|---:|---:|---:|---:|
| saga_1m | 749 (60.8x) | 48345 (2956.9x) | 0.409 | 0.283 | 0.87 | 0.425 | 1.35 |
| saga_1m_guards | 561 (51.2x) | 48345 (3238.4x) | 0.449 | 0.312 | 0.855 | 0.421 | 1.39 |
| ba_1m | 939 (93.9x) | 952 (95.2x) | 0.104 | 1.0 | 0.0 | 0.001 | 1.02 |
| er_1m | 25 (2.5x) | 25 (2.5x) | 0.019 | 1.0 | 0.0 | 0.001 | 1.02 |
| saga_10m | 6575 (59.1x) | 482588 (4534.8x) | 0.436 | 0.094 | 0.98 | 0.415 | 1.33 |
| ldbc_sf1 | 10750 (31.8x) | 766 (725.3x) | 0.063 | 1.0 | 0.0 | 0.679 | 11.58 |

## Analysis

**Load.** Edge ingest is the cost; node ingest is small at 100k nodes. SAGA 1M loads in
0.83 s versus 0.58 s for BA/ER at the same node and edge counts. The difference is data
width, not structure: the SAGA CSV is 2.1x larger (62.7 vs 30 MB) because every edge carries
event_type, label and amount and every node carries a packed attribute string, and the Kuzu
file is correspondingly 2.1x larger (57.5 vs 28 MB). The guarded dataset loads in the same
way (0.78 s; the smaller number reflects 11% fewer edges). LDBC SF1 (3.2M edges, 3.07M nodes)
loads in 1.09 s, with node ingest (0.50 s) now as large as edge ingest.

**Q1 (windowed 1-hop).** 29 to 32 ms on all 1M datasets and 21 ms on LDBC: 100 primary-key
lookups plus a short adjacency scan are dominated by fixed per-query overhead, and structure
does not show up in latency. What structure does change is the answer: the number of sources
with at least one edge in the week is 42 (SAGA) vs 36 (BA) vs 14 (ER), consistent with SAGA's
skewed, recurrent out-activity concentrating edges on active accounts.

**Q2 (time-respecting 2-hop).** This is the one query where structure drives latency at 1M
edges: SAGA 34 ms, ER 58 ms, BA 108 ms. BA's randomly oriented preferential attachment gives
hubs with both ~950 in- and ~950 out-edges, so the second hop fans out through hubs; ER has no
hubs but every first-hop neighbour has ~10 out-edges spread uniformly in time; SAGA's in-degree
hubs are merchants/institutions with very few out-edges (max in-degree 48,345 vs max
out-degree 749), and 87% of its edges are repeats of an existing pair (distinct-pair ratio
0.28), so the frontier after one hop is small and mostly dead-ends. The honest caveat is that
at 1M edges (about one edge per node per week) the time-respecting 2-hop inside a 7-day window
finds almost nothing (0 rows for SAGA and ER, 6 for BA, 1 for LDBC), so the latency measures
frontier expansion and filtering rather than result construction. At 10M edges SAGA's Q2 rises
15x to 510 ms for 10x the edges, which is the expected super-linear behaviour of a 2-hop
expansion when mean out-degree goes from 12 to 111.

**Q3 (daily top-10 in-degree).** A full edge scan with a time filter and a group-by: 9.9 ms
(SAGA) vs 12.1 to 12.5 ms (BA/ER) vs 15.1 ms (LDBC). The SAGA graph is slightly faster despite
wider rows; the aggregate has far fewer groups (in-edges concentrate on merchants: the top 1%
of in-degree nodes receive 41% of edges, vs 10% in BA and 2% in ER). That is consistent with
skew helping the hash aggregate, but a 20% gap on a 10 ms query is not strong evidence and we
do not claim more than consistency. The winning hub has 58 in-edges in the day on SAGA vs 26
on BA (a merchant vs a BA hub).

**Q4 (pair recurrence).** 100 to 125 ms on all 1M datasets and 159 ms on LDBC; Kuzu evaluates
the 100 two-endpoint patterns at roughly 1 ms each regardless of structure, so latency is
structure-independent here. The query is only semantically interesting on SAGA, where 87% of
edges are repeats of an existing pair (98% at 10M) and sampled pairs return counts above 1,
while BA/ER/LDBC have no repeated (src,dst) pairs at all (every count is 1). This is the
recurrence property SAGA controls and the classical baselines cannot express.

**Q5 (labeled subgraph).** SAGA only: 1,226 anomalous edges joined to both endpoints' attributes
in 44 ms at 1M, 12,948 edges in 279 ms at 10M (6.4x for 10.6x the rows). N/A for the baselines
and LDBC.

**Q6 (per-day counts).** Scan plus 90-group aggregate: 25 ms (SAGA) vs 28 ms (BA/ER); 22 ms on
LDBC with 983 groups. SAGA's temporal burstiness (B = 0.42 vs 0.00 for the uniform-time
baselines; busiest day 1.35x the mean vs 1.02x) is real but does not show in this query's
latency, and we do not expect it to: a day-resolution group-by is insensitive to how edges
are distributed within or across days.

**Where the evidence does not support a structural explanation.** Q1, Q3, Q4 and Q6 differ by
at most 25% across SAGA/BA/ER at 1M, within what row width and constant overheads explain, so
degree skew, recurrence and burstiness cannot be credited for those differences. Only Q2 shows
a latency effect that tracks the controlled structure (hub orientation and pair recurrence),
and the effect on result counts (Q1 rows, Q3 top hub, Q4 repeats) is where the generator's
properties are unambiguously visible.

**Scaling 1M to 10M (SAGA finance, same 100k nodes).** Load 0.83 to 5.60 s (6.7x total, 9.6x for
the edge COPY alone), on-disk 57.5 to 351 MB (6.1x; string dictionaries amortize). Q1 48.8 ms
(1.7x: point lookups, the growth is the 9x longer adjacency lists), Q3 58.7 ms (5.9x) and Q6
151.7 ms (6.1x), both slightly sub-linear in edges because they are parallel scans.

## loadavg notes

The protocol gate (skip timing while the 1-min load is above 8) was enforced by the runner,
which polls `/proc/loadavg` before each timed block and waits while it is above 8. During all
reported timing the 1-min load was between 3.7 and 7.4 (see the loadavg columns and
`results/workload.log`). The 5-min and 15-min averages were elevated (14 to 16) at the start of
the timed run: a first attempt at the LDBC canonicalization with an all-core DuckDB query had
hung and was killed before timing began; no other experiment of ours was running. Other user
processes on the host were idle (all below 2% CPU when checked). The smoke run of the BA
dataset before the timed run waited through a transient 1-min load of 13 and resumed at 7.5.

## Summary

Kuzu 0.11.3 loads SAGA finance at 1.2M edges/s (1M) to 1.8M edges/s (10M) with a 35 to 58 B/edge
footprint, and the six temporal queries run in 10 to 105 ms at 1M edges and 49 to 510 ms at 10M.
Against Barabasi-Albert and Erdos-Renyi graphs with identical node and edge counts, SAGA loads
about 40% slower purely because its rows are wider (attributes and labels), and its per-query
latency is the same or lower on five of six queries; the only structure-driven latency gap is the
time-respecting 2-hop (SAGA 34 ms vs BA 108 ms vs ER 58 ms), attributable to SAGA's one-directional
hubs and 87% pair recurrence. LDBC SNB SF1 was obtained from the official host and run through
the same workload (load 1.09 s for 3.2M edges; 21 to 159 ms per query), but its node count and
edge semantics differ enough that it is a reference point, not a like-for-like control. The
guarded dataset (guards on, L = 1 day) loads and queries identically to the guards-off one.
All generated graphs, the LDBC download and the Kuzu database files were deleted after
measurement.
