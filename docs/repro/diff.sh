#!/bin/bash
# Difficulty sweep: finance, 500k edges, pi=0.05, seed 42, delta in {0,.25,.5,.75,1}
# Detector: GBM (detect_auc), 3 views: all / attr / struct. Report ROC-AUC + AP.
set -e
BIN=./target/release/saga
SCR=./scratch
OUT=$SCR/diff_gen
RES=$SCR/diff_results.csv
export PYTHONPATH=./python
echo "delta,auc_all,auc_attr,auc_struct,ap_all,ap_attr,ap_struct,n_anom,n_total" > $RES
for D in 0 0.25 0.5 0.75 1.0; do
  rm -rf $OUT
  $BIN generate --domain finance --edges 500000 --seed 42 --anomaly-rate 0.05 --difficulty $D --format csv --out $OUT >/dev/null 2>&1
  python3 -c "
from saga_eval.detector import detect_auc
a=detect_auc('$OUT', feature_set='all')
at=detect_auc('$OUT', feature_set='attr')
st=detect_auc('$OUT', feature_set='struct')
print(f\"$D,{a['roc_auc']:.4f},{at['roc_auc']:.4f},{st['roc_auc']:.4f},{a['ap']:.4f},{at['ap']:.4f},{st['ap']:.4f},{a['n_anom']},{a['n_total']}\")
" >> $RES
  tail -1 $RES
  rm -rf $OUT
done
echo "DONE_DIFF"
