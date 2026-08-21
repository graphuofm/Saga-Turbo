#!/bin/bash
# Throughput / scaling: finance, seed 42, sizes 1M 10M 100M. Wall + peak RAM + M/s.
# Uses /usr/bin/time -v for wall clock and max RSS. Deletes each graph immediately.
BIN=./target/release/saga
SCR=./scratch
OUT=$SCR/thrpt_gen
RES=$SCR/thrpt_results.csv
echo "edges,wall_s,maxrss_gb,engine_eps,engine_edges,loadavg1" > $RES
for E in 1000000 10000000 100000000; do
  rm -rf $OUT
  LOAD=$(cut -d' ' -f1 /proc/loadavg)
  /usr/bin/time -v $BIN generate --json --domain finance --edges $E --seed 42 --out $OUT \
    > $SCR/thrpt_last.json 2> $SCR/thrpt_time.txt
  WALL=$(grep "Elapsed (wall clock)" $SCR/thrpt_time.txt | sed 's/.*: //')
  RSS=$(grep "Maximum resident set size" $SCR/thrpt_time.txt | sed 's/.*: //')
  python3 -c "
import json,re
d=json.load(open('$SCR/thrpt_last.json'))
eps=d['events_per_sec']; ew=d['edges_written']
rss_kb=$RSS
gb=rss_kb/1024/1024
# parse wall '$WALL' m:ss or h:mm:ss
w='$WALL'.split(':')
sec=float(w[-1])+ (float(w[-2])*60 if len(w)>=2 else 0)+(float(w[-3])*3600 if len(w)>=3 else 0)
print(f'$E,{sec:.2f},{gb:.2f},{eps:.0f},{ew},$LOAD')
" >> $RES
  tail -1 $RES
  rm -rf $OUT
done
echo "DONE_THRPT"
