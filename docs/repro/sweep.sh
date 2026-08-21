#!/bin/bash
# Homophily calibration sweep: finance vs Elliptic at matched size, seed 42.
# Copies finance.yaml, rewrites the `homophily:` control line, generates via an
# external rule-base path (no rebuild), measures, deletes the graph.
set -e
SC=./scratch
BIN=./target/release/saga
YAML=./crates/saga-rules/domains/finance.yaml
cd ./

for H in "$@"; do
  TMP=$SC/finance_h.yaml
  # rewrite only the control `homophily:` line (line 364), keep the comment tail
  sed "364s/  homophily: [0-9.]*/  homophily: $H/" "$YAML" > "$TMP"
  OUT=$SC/gen_h
  rm -rf "$OUT"
  $BIN generate --domain "$TMP" --nodes 203769 --edges 234355 --seed 42 \
      --out "$OUT" --format csv --quiet >/dev/null 2>&1
  printf "h=%-5s " "$H"
  python3 $SC/measure.py "$OUT"
  rm -rf "$OUT"
done
rm -f $SC/finance_h.yaml
