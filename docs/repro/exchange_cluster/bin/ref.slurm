#!/bin/bash
#SBATCH --partition=bigTiger
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=8
#SBATCH --mem-per-cpu=4G
#SBATCH --time=1:00:00
#SBATCH --output=$PROJECT/saga/logs/%x_%j.out
# env: EDGES, NODES, TAG, REPS
set -u
B=$PROJECT/saga/xrun/$TAG; mkdir -p $B
echo "job $SLURM_JOB_ID node $SLURM_NODELIST EDGES=$EDGES"
grep -m1 'model name' /proc/cpuinfo; nproc; cat /proc/loadavg; cat /etc/redhat-release 2>/dev/null
for rep in $(seq 1 ${REPS:-2}); do
  LOC=/tmp/saga_ref_${SLURM_JOB_ID}/rep$rep; mkdir -p $LOC
  S=$(date +%s.%N)
  $PROJECT/saga/bin/agora generate --domain finance --nodes $NODES --edges $EDGES --seed 42 \
    --guards --block-len-days 1 --threads 8 --json --quiet --out $LOC > $B/ref$rep.json 2> $B/ref$rep.err
  RC=$?; E=$(date +%s.%N)
  echo "ref$rep wall $(awk "BEGIN{print $E - $S}") rc $RC"
  cp -r $LOC $B/out$rep
done
rm -rf /tmp/saga_ref_${SLURM_JOB_ID}
PY=$PROJECT/hkenv/bin/python
$PY $PROJECT/saga/bin/digest.py $B/out1 $B/out2 > $B/digest.json
cat $B/digest.json
