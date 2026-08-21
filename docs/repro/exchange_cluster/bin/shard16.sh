#!/bin/bash
# Section-16 rerun: one shard task of the listing-based exchange (no poke sidecar).
# env: N shards, NODES, EDGES, XD exchange dir (NFS, fresh), RD result dir (NFS), TO timeout s
K=${SLURM_PROCID}
LOC=/tmp/saga_x_${SLURM_JOB_ID}/shard$K
mkdir -p $LOC $RD
H=$(hostname)
cat /proc/loadavg > $RD/shard$K.loadavg_start
S=$(date +%s.%N)
$PROJECT/saga/bin/agora generate --domain finance --nodes $NODES --edges $EDGES --seed 42 \
  --guards --block-len-days 1 --threads 8 --json --quiet \
  --shard-count $N --shard-index $K --exchange-dir $XD --exchange-timeout-s $TO \
  --out $LOC > $RD/shard$K.json 2> $RD/shard$K.err
RC=$?
E=$(date +%s.%N)
echo "$K $H $S $E $RC $(awk "BEGIN{print $E - $S}")" > $RD/shard$K.time
if [ $RC -eq 0 ]; then cp -r $LOC $RD/out$K; fi
rm -rf /tmp/saga_x_${SLURM_JOB_ID}
