#!/bin/bash
# Single-machine scale curve to 1B edges (finance), measure wall/RSS/throughput, delete each.
cd ./
SP=./scratch
BIN=./target/release/saga
OUT=$SP/scalecurve.txt
: > $OUT
printf "%-12s %12s %8s %8s %10s %8s\n" edges edges_out wall_s rss_gb Medges_s cpu_pct >> $OUT
for E in 10000000 30000000 100000000 300000000 1000000000; do
  /usr/bin/time -v $BIN generate --domain finance --edges $E --seed 42 \
      --anomaly-rate 0.02 --out $SP/big --quiet 2> $SP/tl >/dev/null
  te=$(python3 -c "import json;print(json.load(open('$SP/big/saga_stats.json'))['total_edges'])" 2>/dev/null)
  wall=$(grep -i "Elapsed" $SP/tl | grep -oE "[0-9:]+\.[0-9]+" | awk -F: '{if(NF==3)print $1*3600+$2*60+$3; else if(NF==2)print $1*60+$2; else print $1}')
  rssk=$(grep -i "Maximum resident" $SP/tl | grep -oE "[0-9]+")
  cpu=$(grep -i "Percent of CPU" $SP/tl | grep -oE "[0-9]+%" | tr -d %)
  rssg=$(python3 -c "print(f'{$rssk/1048576:.2f}')")
  mps=$(python3 -c "print(f'{$te/$wall/1e6:.2f}')" 2>/dev/null)
  printf "%-12s %12s %8s %8s %10s %8s\n" "$E" "$te" "$wall" "$rssg" "$mps" "$cpu" >> $OUT
  rm -rf $SP/big
done
echo "DONE" >> $OUT
