#!/bin/bash
#SBATCH --partition=bigTiger
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=8
#SBATCH --mem-per-cpu=4G
#SBATCH --time=0:10:00
#SBATCH --output=$PROJECT/saga/logs/%x_%j.out
set -u
A=$PROJECT/saga/bin/agora
echo "job $SLURM_JOB_ID node $SLURM_NODELIST"; cat /etc/redhat-release; ldd --version | head -1; cat /proc/loadavg
sha256sum $A
$A --version
echo "--- generate --help exchange flags:"; $A generate --help | grep -iE 'shard|exchange'
LOC=/tmp/saga_smoke_$SLURM_JOB_ID; mkdir -p $LOC
echo "--- tiny single run json telemetry keys:"
$A generate --domain finance --nodes 2000 --edges 20000 --seed 42 --guards --block-len-days 1 --threads 8 --json --quiet --out $LOC/single | grep -oE '"(exchange_[a-z_]*|edges_written|wall_time_s|guarded[a-z_]*|violations[a-z_]*)": [0-9.]*'
echo "--- tiny N=2 on this node (two processes, local /tmp exchange dir):"
XD=$LOC/xchg; mkdir -p $XD
for k in 0 1; do $A generate --domain finance --nodes 2000 --edges 20000 --seed 42 --guards --block-len-days 1 --threads 4 --json --quiet --shard-count 2 --shard-index $k --exchange-dir $XD --exchange-timeout-s 60 --out $LOC/s$k > $LOC/s$k.json 2> $LOC/s$k.err & done; wait
for k in 0 1; do echo "shard $k:"; grep -oE '"(exchange_[a-z_]*|edges_written|wall_time_s)": [0-9.]*' $LOC/s$k.json | tr '\n' ' '; echo; cat $LOC/s$k.err; done
echo "xchg leftover entries: $(find $XD 2>/dev/null | wc -l)"
rm -rf $LOC
