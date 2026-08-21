#!/bin/bash
# one shard task: $N shards, $EDGES edges, $XD exchange dir (NFS), $RD result dir (NFS), $TO timeout
# POKE=1: run a sidecar that creates+unlinks a hidden file in $XD every ~5 ms so THIS node's NFS
# client revalidates the directory (defeats the negative-dentry / acdirmin cache that otherwise
# makes the 2 ms File::open poll see a peer's renamed file only after ~15-30 s).
K=${SLURM_PROCID}
LOC=/tmp/saga_x_${SLURM_JOB_ID}/shard$K
mkdir -p $LOC $RD
H=$(hostname)
cat /proc/loadavg > $RD/shard$K.loadavg_start
if [ "${POKE:-0}" = "1" ]; then
  ( while [ ! -f $RD/shard$K.done ]; do : > $XD/.poke$K; rm -f $XD/.poke$K; sleep 0.005; done ) &
  PK=$!
fi
S=$(date +%s.%N)
$PROJECT/saga/bin/agora generate --domain finance --nodes $NODES --edges $EDGES --seed 42 \
  --guards --block-len-days 1 --threads 8 --json --quiet \
  --shard-count $N --shard-index $K --exchange-dir $XD --exchange-timeout-s $TO \
  --out $LOC > $RD/shard$K.json 2> $RD/shard$K.err
RC=$?
E=$(date +%s.%N)
touch $RD/shard$K.done
[ -n "${PK:-}" ] && wait $PK
echo "$K $H $S $E $RC $(awk "BEGIN{print $E - $S}")" > $RD/shard$K.time
if [ $RC -eq 0 ]; then cp -r $LOC $RD/out$K; fi
rm -rf /tmp/saga_x_${SLURM_JOB_ID}
