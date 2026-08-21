#!/bin/bash
#SBATCH --partition=bigTiger
#SBATCH --cpus-per-task=8
#SBATCH --mem-per-cpu=4G
#SBATCH --time=0:30:00
#SBATCH --output=$PROJECT/saga/logs/%x_%j.out
# Section-16 rerun of the cross-machine exchange with the listing-based discovery.
# env: N, EDGES, NODES, TAG, TO, REPS, NN (nodes), REF (reference out dir for digest.py)
# e.g. N=2 NN=2: sbatch --nodes=2 --ntasks=2 --export=ALL,N=2,NN=2,NODES=100000,EDGES=1000000,TAG=x16_n2,TO=120,REPS=2 x16.slurm
#      N=4 NN=3 (2+1+1, the placement that hung before): sbatch --nodes=3 --ntasks=4 --export=ALL,N=4,NN=3,... x16.slurm
set -u
B=$PROJECT/saga/xrun/$TAG
mkdir -p $B
echo "job $SLURM_JOB_ID nodes $SLURM_NODELIST N=$N EDGES=$EDGES TAG=$TAG NN=${NN:-$N}"
scontrol show hostnames $SLURM_NODELIST
srun --nodes=${NN:-$N} --ntasks=${NN:-$N} --ntasks-per-node=1 bash -c 'echo "$(hostname): load $(cut -d" " -f1-3 /proc/loadavg) | nfs /project: $(grep " /project " /proc/mounts | cut -d" " -f4 | cut -c1-80)"'
for rep in $(seq 1 ${REPS:-2}); do
  RD=$B/rep$rep; XD=$B/rep$rep/xchg
  mkdir -p $XD
  export N EDGES NODES TO RD XD
  S=$(date +%s.%N)
  srun --nodes=${NN:-$N} --ntasks=$N --cpus-per-task=8 --distribution=block bash $PROJECT/saga/bin/shard16.sh
  E=$(date +%s.%N)
  echo "rep$rep srun_wall $(awk "BEGIN{print $E - $S}")"
  cat $RD/shard*.time
  grep -ho '"exchange_[a-z_]*": [0-9]*' $RD/shard*.json | sort | uniq -c | sort -k2 | awk '{print "  "$0}'
  echo "xchg leftover: $(find $XD 2>/dev/null | wc -l) entries (0 = dir removed)"
  find $XD 2>/dev/null | head
done
if [ -n "${REF:-}" ]; then
  PY=$PROJECT/hkenv/bin/python
  for rep in $(seq 1 ${REPS:-2}); do
    $PY $PROJECT/saga/bin/digest.py --ref $REF $(ls -d $B/rep$rep/out*) > $B/rep$rep/digest.json
    echo "rep$rep digest:"; grep -E '"edge_union_equals_ref"|"node_union_equals_ref"|"shard_node_ids_disjoint"|"node_slice_equals_ref"|"edge_digest"|"node_digest"' $B/rep$rep/digest.json
  done
fi
