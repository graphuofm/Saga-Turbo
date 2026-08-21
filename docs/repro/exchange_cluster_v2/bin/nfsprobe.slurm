#!/bin/bash
#SBATCH --partition=bigTiger
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=1
#SBATCH --mem=2G
#SBATCH --time=0:15:00
#SBATCH --output=$PROJECT/saga/logs/%x_%j.out
# NFS metadata-latency probe for one node: time mkdir / create / rename / readdir / unlink / rmdir on /project
set -u
H=$(hostname); D=$PROJECT/saga/xrun/nfsprobe/$H.$SLURM_JOB_ID
echo "node $H job $SLURM_JOB_ID $(date +%T) load $(cut -d' ' -f1-3 /proc/loadavg) kernel $(uname -r)"
grep " /project " /proc/mounts
awk '/\/project /{f=1} f&&/xprt:/{print "xprt:",$0; exit}' /proc/self/mountstats
awk '/\/project /{f=1} f&&/per-op statistics/{g=1} g&&/(LOOKUP|CREATE|RENAME|READDIR|REMOVE|GETATTR|OPEN):/{print; getline; print "   ",$0}' /proc/self/mountstats
t() { local s=$(date +%s.%N); "$@"; local e=$(date +%s.%N); awk "BEGIN{printf \"  %-28s %8.3f s\n\", \"$*\", $e-$s}" | sed "s#$D#D#g"; }
t mkdir -p $D
for i in 1 2 3; do t touch $D/.f$i.tmp; t mv $D/.f$i.tmp $D/f$i.bin; done
t ls $D
t rm -f $D/f1.bin $D/f2.bin $D/f3.bin
t rmdir $D
echo "local /tmp for comparison:"; L=/tmp/saga_probe_$SLURM_JOB_ID; t mkdir -p $L; t touch $L/a; t mv $L/a $L/b; t rm $L/b; t rmdir $L
echo "done $(date +%T)"
