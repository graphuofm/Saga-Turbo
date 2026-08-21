#!/usr/bin/env bash
# Single entry point for the pinned regression suite.
#
#   scripts/regress.sh            build release, run the default suite, then
#                                 the heavy pins, print a PASS/FAIL table
#   scripts/regress.sh --fast     default suite only (skip the heavy pins)
#   scripts/regress.sh --heavy    heavy pins only (assumes a recent build)
#
# Pinned invariants live in crates/saga-cli/tests/regress.rs; see
# docs/VERIFICATION_MATRIX.md, section "Regression suite". Generated graphs
# go to unique temp directories that the tests remove themselves.
set -u
cd "$(dirname "$0")/.."

mode=all
case "${1:-}" in
  --fast) mode=fast ;;
  --heavy) mode=heavy ;;
  "") ;;
  *) echo "usage: $0 [--fast|--heavy]" >&2; exit 2 ;;
esac

declare -a names results times
overall=0

record() { # name result seconds
  names+=("$1"); results+=("$2"); times+=("$3")
  [ "$2" = PASS ] || overall=1
}

run_step() { # label command...
  local label=$1; shift
  local t0=$SECONDS
  if "$@" >"$log" 2>&1; then r=PASS; else r=FAIL; fi
  record "$label" "$r" "$((SECONDS - t0))"
  [ "$r" = PASS ] || { echo "---- $label FAILED, log follows ----"; tail -n 60 "$log"; echo "-----------------------------------"; }
}

log=$(mktemp)
trap 'rm -f "$log"' EXIT

if [ "$mode" != heavy ]; then
  run_step "cargo build --release" cargo build --release
  run_step "default suite: cargo test --release (workspace)" cargo test --release
fi

# Per-invariant rows (default suite pins, run individually so the table is
# one line per invariant).
fast_pins=(
  "I1 fast: guards-off logical digests, 6 domains, 100k|bytecompat_100k_logical_all_domains"
  "I2: sweep spot-points finance tiny L in {0.001,1,30}|sweep_spot_points_finance_tiny"
  "I3: guards-on threads 1 vs 8, 6 domains, counters|guards_on_thread_determinism_all_domains"
  "I4: ecommerce 1M calibrated fraction within 2%|calibration_ecommerce_1m_within_tolerance"
  "I5 fast: N in {2,4} real shard processes == single (exchange_cli)|real_processes_equal_single_machine_and_emulation"
)
heavy_pins=(
  "I1 heavy: file sha256 + logical digests, 6 domains, 1M|bytecompat_1m_all_domains"
  "I5 heavy: N=2 real processes, 1M config, 887,156 edges|exchange_1m_two_processes_equal_single"
)

if [ "$mode" != heavy ]; then
  for entry in "${fast_pins[@]}"; do
    label=${entry%%|*}; test=${entry##*|}
    tfile=regress; [ "$test" = real_processes_equal_single_machine_and_emulation ] && tfile=exchange_cli
    run_step "$label" cargo test --release -q -p saga-cli --test "$tfile" -- --exact "$test"
  done
fi
if [ "$mode" != fast ]; then
  for entry in "${heavy_pins[@]}"; do
    label=${entry%%|*}; test=${entry##*|}
    run_step "$label" cargo test --release -q -p saga-cli --test regress -- --ignored --exact "$test"
  done
fi

echo
printf '%-70s %-6s %6s\n' "invariant" "result" "sec"
printf '%-70s %-6s %6s\n' "$(printf '%.0s-' {1..70})" "------" "------"
for i in "${!names[@]}"; do
  printf '%-70s %-6s %6s\n' "${names[$i]}" "${results[$i]}" "${times[$i]}"
done
echo
if [ $overall -eq 0 ]; then echo "REGRESS: ALL PASS"; else echo "REGRESS: FAIL"; fi
exit $overall
