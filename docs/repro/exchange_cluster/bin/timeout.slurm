#!/bin/bash
#SBATCH --partition=bigTiger
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=8
#SBATCH --mem-per-cpu=4G
#SBATCH --time=0:20:00
#SBATCH --output=$PROJECT/saga/logs/%x_%j.out
set -u
B=$PROJECT/saga/xrun/timeout; XD=$B/xchg; mkdir -p $XD; LOC=/tmp/saga_to_${SLURM_JOB_ID}; mkdir -p $LOC
echo "job $SLURM_JOB_ID node $SLURM_NODELIST: launching ONLY shard 0 of N=2, timeout 30 s"
S=$(date +%s.%N)
$PROJECT/saga/bin/agora generate --domain finance --nodes 100000 --edges 1000000 --seed 42 \
  --guards --block-len-days 1 --threads 8 --json --quiet --shard-count 2 --shard-index 0 \
  --exchange-dir $XD --exchange-timeout-s 30 --out $LOC > $B/shard0.json 2> $B/shard0.err
RC=$?; E=$(date +%s.%N)
echo "rc $RC wall $(awk "BEGIN{print $E - $S}")"
echo "--- stderr:"; cat $B/shard0.err
echo "--- stdout bytes: $(wc -c < $B/shard0.json)"
echo "--- exchange dir after failure:"; ls -la $XD
echo "--- out dir after failure:"; ls -la $LOC
rm -rf $LOC
