#!/bin/bash
# Industrial scale-out: 100M edges per domain, measure wall/RSS/throughput, delete after.
cd ./
SP=./scratch
BIN=./target/release/saga
OUT=$SP/scaleout.txt
: > $OUT
printf "%-12s %12s %8s %8s %10s %8s\n" domain edges wall_s rss_gb Medges_s cpu_pct >> $OUT
for d in finance crypto cyber transport ecommerce healthcare; do
  /usr/bin/time -v $BIN generate --domain $d --edges 100000000 --seed 42 \
      --anomaly-rate 0.02 --out $SP/big --quiet 2> $SP/tl >/dev/null
  te=$(python3 -c "import json;print(json.load(open('$SP/big/saga_stats.json'))['total_edges'])" 2>/dev/null)
  wall=$(grep -i "Elapsed" $SP/tl | grep -oE "[0-9]+:[0-9.]+" | awk -F: '{print $1*60+$2}')
  rssk=$(grep -i "Maximum resident" $SP/tl | grep -oE "[0-9]+")
  cpu=$(grep -i "Percent of CPU" $SP/tl | grep -oE "[0-9]+%" | tr -d %)
  rssg=$(python3 -c "print(f'{$rssk/1048576:.2f}')")
  mps=$(python3 -c "print(f'{$te/$wall/1e6:.2f}')")
  printf "%-12s %12s %8s %8s %10s %8s\n" "$d" "$te" "$wall" "$rssg" "$mps" "$cpu" >> $OUT
  rm -rf $SP/big
done
echo "DONE" >> $OUT
