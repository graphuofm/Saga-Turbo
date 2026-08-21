#!/usr/bin/env bash
# Database-workload experiment driver (docs/remeasure_db_workload.md).
# Generates the datasets with the frozen binary, canonicalizes them, loads each
# into an embedded Kuzu database and runs the six-query workload, writing one
# JSON per dataset into $RES. All generated graphs and database files are
# deleted afterwards; only the JSONs (and summary CSV) are kept.
#
# usage: run_all.sh <scratch_dir> <results_dir>
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
BIN=$ROOT/target/release/saga
HERE=$ROOT/docs/repro/db_workload
S=${1:?scratch}; RES=${2:?results}
D=$S/data; mkdir -p "$D" "$S/db" "$RES"

echo "loadavg $(cat /proc/loadavg)"
$BIN generate --domain finance --edges 1000000  --seed 42 --format csv --out "$D/saga_1m" -q
$BIN generate --domain finance --edges 1000000  --seed 42 --guards --block-len-days 1 --format csv --out "$D/saga_1m_guards" -q
$BIN generate --domain finance --edges 10000000 --seed 42 --format csv --out "$D/saga_10m" -q
for d in saga_1m saga_1m_guards saga_10m; do python3 "$HERE/prepare.py" saga "$D/$d" "$D/${d}_canon"; done
N_EDGES=$(($(wc -l < "$D/saga_1m_canon/edges.csv") - 1))
python3 "$HERE/gen_baselines.py" "$D" 100000 "$N_EDGES" 42

# LDBC SNB Interactive v1 SF1 (CsvBasic, LongDateFormatter), official pre-generated CSVs.
if [ ! -d "$D/ldbc_sf1" ]; then
  mkdir -p "$S/ldbc"
  if timeout 600 curl -sS -L -o "$S/ldbc/sf1.tar.zst" \
       https://datasets.ldbcouncil.org/snb-interactive-v1/social_network-sf1-CsvBasic-LongDateFormatter.tar.zst; then
    (cd "$S/ldbc" && tar --use-compress-program=unzstd -xf sf1.tar.zst --wildcards \
       '*/dynamic/person_0_0.csv' '*/dynamic/post_0_0.csv' '*/dynamic/comment_0_0.csv' \
       '*/dynamic/person_knows_person_0_0.csv' '*/dynamic/post_hasCreator_person_0_0.csv' \
       '*/dynamic/comment_hasCreator_person_0_0.csv')
    python3 "$HERE/prepare.py" ldbc "$S/ldbc/social_network-sf1-CsvBasic-LongDateFormatter" "$D/ldbc_sf1"
  else
    echo "LDBC download failed; skipping"
  fi
fi

for pair in saga_1m:saga_1m_canon saga_1m_guards:saga_1m_guards_canon ba_1m:ba_1m er_1m:er_1m saga_10m:saga_10m_canon ldbc_sf1:ldbc_sf1; do
  name=${pair%%:*}; dir=${pair##*:}
  [ -d "$D/$dir" ] || continue
  echo "== $name  loadavg $(cat /proc/loadavg)"
  python3 "$HERE/run_workload.py" "$name" "$D/$dir" "$S/db" "$RES/$name.json"
done
python3 "$HERE/summarize.py" "$RES"

rm -rf "$D" "$S/db" "$S/ldbc"
echo "cleaned: $(du -sh "$S" 2>/dev/null)"
