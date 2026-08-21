#!/bin/bash
# g(pi) elasticity sweep: 6 domains x 5 pi values, 1M edges, seed 42
set -e
BIN=./target/release/saga
SCR=./scratch
OUT=$SCR/elast_gen
RES=$SCR/elast_results.csv
echo "domain,pi,edges_written,anomalous_edges,frac" > $RES
for DOM in finance crypto cyber ecommerce healthcare transport; do
  for PI in 0.005 0.01 0.02 0.05 0.10; do
    rm -rf $OUT
    $BIN generate --json --domain $DOM --edges 1000000 --seed 42 --anomaly-rate $PI --out $OUT > $SCR/elast_last.json 2>/dev/null
    python3 -c "
import json
d=json.load(open('$SCR/elast_last.json'))
ew=d['edges_written']; ae=d['anomalous_edges']
print(f'$DOM,$PI,{ew},{ae},{ae/ew:.5f}')
" >> $RES
    tail -1 $RES
    rm -rf $OUT
  done
done
echo "DONE_ELAST"
