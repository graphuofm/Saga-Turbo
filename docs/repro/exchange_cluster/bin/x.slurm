#!/bin/bash
#SBATCH --partition=bigTiger
#SBATCH --cpus-per-task=8
#SBATCH --mem-per-cpu=4G
#SBATCH --time=1:30:00
#SBATCH --output=$PROJECT/saga/logs/%x_%j.out
# env: N, EDGES, NODES, TAG, TO, REPS, POKE, NN (nodes), TPN (tasks per node)
set -u
B=$PROJECT/saga/xrun/$TAG
mkdir -p $B
echo "job $SLURM_JOB_ID nodes $SLURM_NODELIST N=$N EDGES=$EDGES TAG=$TAG POKE=${POKE:-0} NN=${NN:-$N} TPN=${TPN:-1}"
scontrol show hostnames $SLURM_NODELIST
srun --nodes=${NN:-$N} --ntasks=${NN:-$N} --ntasks-per-node=1 bash -c 'echo "$(hostname): $(grep -m1 "model name" /proc/cpuinfo | cut -d: -f2) | $(nproc) cpus | load $(cut -d" " -f1-3 /proc/loadavg) | ib: $(cat /sys/class/infiniband/*/ports/1/rate 2>/dev/null | head -1) $(ls /sys/class/infiniband 2>/dev/null | tr "\n" " ") | nfs /project: $(grep " /project " /proc/mounts | cut -d" " -f4 | cut -c1-60)"'
for rep in $(seq 1 ${REPS:-2}); do
  RD=$B/rep$rep; XD=$B/rep$rep/xchg
  mkdir -p $XD
  export N EDGES NODES TO RD XD POKE
  S=$(date +%s.%N)
  srun --nodes=${NN:-$N} --ntasks=$N --cpus-per-task=8 --distribution=block bash $PROJECT/saga/bin/shard.sh
  E=$(date +%s.%N)
  echo "rep$rep srun_wall $(awk "BEGIN{print $E - $S}")"
  cat $RD/shard*.time
  echo "xchg leftover: $(ls -A $XD 2>/dev/null | wc -l) files; dir exists: $([ -d $XD ] && echo yes || echo no)"
  ls -A $XD 2>/dev/null | head
done
if [ -n "${REF:-}" ]; then
  PY=$PROJECT/hkenv/bin/python
  for rep in $(seq 1 ${REPS:-2}); do
    $PY $PROJECT/saga/bin/digest.py --ref $REF $(ls -d $B/rep$rep/out*) > $B/rep$rep/digest.json
    echo "rep$rep digest:"; grep -E '"edge_union_equals_ref"|"node_union_equals_ref"|"shard_node_ids_disjoint"|"node_slice_equals_ref"|"edge_digest"|"node_digest"|"edges"|"nodes"' $B/rep$rep/digest.json
  done
fi
