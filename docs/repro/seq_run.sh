#!/bin/bash
# Sequential: difficulty sweep, then downstream. Thread-capped to avoid thrash.
export OMP_NUM_THREADS=6 MKL_NUM_THREADS=6 OPENBLAS_NUM_THREADS=6 NUMEXPR_NUM_THREADS=6
export PYTHONPATH=./python
BIN=./target/release/saga
SCR=$WORK
cd .

# ---- Difficulty sweep ----
OUT=$SCR/diff_gen
RES=$SCR/diff_results.csv
echo "delta,auc_all,auc_attr,auc_struct,ap_all,ap_attr,ap_struct,n_anom,n_total" > $RES
for D in 0 0.25 0.5 0.75 1.0; do
  rm -rf $OUT
  $BIN generate --domain finance --edges 500000 --seed 42 --anomaly-rate 0.05 --difficulty $D --threads 6 --format csv --out $OUT >/dev/null 2>&1
  python3 -c "
from saga_eval.detector import detect_auc
a=detect_auc('$OUT', feature_set='all'); at=detect_auc('$OUT', feature_set='attr'); st=detect_auc('$OUT', feature_set='struct')
print(f\"$D,{a['roc_auc']:.4f},{at['roc_auc']:.4f},{st['roc_auc']:.4f},{a['ap']:.4f},{at['ap']:.4f},{st['ap']:.4f},{a['n_anom']},{a['n_total']}\")
" >> $RES
  echo "[diff] delta=$D done: $(tail -1 $RES)"
  rm -rf $OUT
done
echo "DONE_DIFF"

# ---- Downstream ----
python3 $SCR/downstream.py $SCR/saga_cmsg
echo "DONE_SEQ"
