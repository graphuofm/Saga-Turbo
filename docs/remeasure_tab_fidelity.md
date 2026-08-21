# Camera-ready re-measurement: tab:fidelity (7 real graphs, final fixed engine)

Re-measurement of the 7-row fidelity-vs-real table (`paper/main.tex`
`tab:fidelity`, label ~line 1081) on the FINAL fixed engine
(`target/release/agora`, the pre-built binary; NOT rebuilt this session). Every
number below is from a real seed-42 run. All generated graphs were deleted
immediately after scoring. This is a measurement log only: it does NOT touch
`paper/main.tex`, `paper.frozen/`, `../agora-anon`, or
`docs/PAPER_SPEC.md`, and nothing is committed.

## 1. Protocol (reproduced exactly from the submission driver)

Source-of-truth driver: `scratchpad/fidelity_driver.py` -> `fidelity_multi.json`,
whose `fidelity` / `burstiness_b` / `repeat_edge_ratio` columns reproduce the
frozen `tab:fidelity` cells exactly (see the column-identity note below). The
driver was re-run with only the binary path changed to the final `agora`.

Per row:
1. Load the real temporal edge list `(src, dst, t)` via `saga_eval.load.load_real`.
2. Cap to the earliest `CAP = 2_500_000` events (stable argsort on `t`, keep the
   first CAP, re-sort to index order). Real graphs below 2.5M edges are used whole
   (only wiki-talk 7.83M and tgbl-review 4.87M are actually capped, to 2.5M).
3. Read the capped real graph's `(n, m, span_days)`.
4. Generate AGORA matched to it, seed 42:
   `agora generate --domain <d> --nodes <n> --edges <m> --time-span <span_days>
   --seed 42 --anomaly-rate 0.02 --out <dir> --quiet`, then cap the AGORA output
   the same way (earliest <=2.5M events).
5. `compute_stats` on both, then `saga_eval.compare.compare(real, synth)`.

The three printed columns:
- **fid.** = `compare(...)["fidelity_score"]` (1 - mean of bounded per-metric
  distances; KS on in/out/total-degree + inter-event-time, plus squashed scalar
  deltas). Rounded to 2 dp for display.
- **B r/s** = `burstiness_b` real / synth = `(sigma-mu)/(sigma+mu)` of per-source
  inter-event gaps (Goh & Barabasi).
- **recip. r/s** = see the column-identity note.

**Row -> AGORA domain map** (this table's map; note it differs from the 6x10
matrix, where sx-mathoverflow->healthcare and sx-askubuntu->transport):
finance{email-Eu, CollegeMsg}, crypto{wiki-talk, sx-mathoverflow},
cyber{sx-superuser, sx-askubuntu}, ecommerce{tgbl-review}. Real references:
`realdata/snap-email-eu/`, `realdata/snap-collegemsg/`,
`~/.cache/zord/wiki-talk-temporal.txt.gz`, `~/.cache/zord/sx-mathoverflow.txt.gz`,
`~/.cache/zord/sx-superuser.txt.gz`, `~/.cache/zord/sx-askubuntu.txt.gz`,
`realdata/tgbl-review/tgbl-review_edgelist_v2.csv` (cols source/target/ts).

### Column-identity note (important)

The paper column labeled **"recip. r/s"** is NOT `compute_stats.reciprocity`. Its
frozen values (email-Eu .92/.97, CollegeMsg .66/.82, wiki-talk .58/.80,
sx-mathoverflow .53/.85, sx-superuser .36/.56, sx-askubuntu .38/.53,
tgbl-review .04/.46) match the driver's **`repeat_edge_ratio` (recurrence)**
column cell-for-cell, and are consistent with the paper body's claim that AGORA is
"slightly more recurrent than real." True `reciprocity` is a different, much
smaller quantity (email-Eu real .71, not .92). So the "recip." column is the
repeat-edge / recurrence ratio; that is what is re-measured below. True
reciprocity is reported separately in section 4 for completeness. (No paper file
is edited; this only documents what the frozen column actually is.)

## 2. Re-measured 7-row table (final fixed engine, seed 42)

| domain | real graph | edges | fid. | B r/s | recip.(=recurrence) r/s |
|---|---|--:|:--:|:--:|:--:|
| finance | email-Eu-core   | 332k  | .80 | .78/.48 | .92/.96 |
| finance | CollegeMsg      | 60k   | .80 | .68/.41 | .66/.83 |
| crypto  | wiki-talk       | 2.5M  | .83 | .76/.59 | .58/.85 |
| crypto  | sx-mathoverflow | 507k  | .72 | .69/.63 | .53/.87 |
| cyber   | sx-superuser    | 1.44M | .81 | .63/.33 | .36/.66 |
| cyber   | sx-askubuntu    | 964k  | .80 | .66/.32 | .38/.64 |
| e-comm. | tgbl-review     | 2.5M  | .84 | .41/.35 | .04/.48 |

Raw (3 dp) fidelity: email-Eu 0.804, CollegeMsg 0.795, wiki-talk 0.829,
sx-mathoverflow 0.717, sx-superuser 0.808, sx-askubuntu 0.804, tgbl-review 0.838.
Matched sizes/spans (days): email-Eu 986n/332,334m/803.93; CollegeMsg
1,899n/59,835m/193.66; wiki-talk 192,665n/2,500,000m/1670.99; sx-mathoverflow
24,818n/506,550m/2350.34; sx-superuser 194,085n/1,443,339m/2773.32; sx-askubuntu
159,316n/964,437m/2613.82; tgbl-review 281,674n/2,500,000m/5525.52.

## 3. Frozen (paper) vs final, side by side

| domain / real | frozen fid | final fid | frozen B s | final B s | frozen recur s | final recur s |
|---|--:|--:|--:|--:|--:|--:|
| finance / email-Eu   | .80 | .80 | .48 | .48 | .97 | .96 |
| finance / CollegeMsg | .79 | .80 | .42 | .41 | .82 | .83 |
| crypto / wiki-talk   | .85 | **.83** | .61 | **.59** | .80 | **.85** |
| crypto / sx-mathov.  | .73 | **.72** | .64 | .63 | .85 | .87 |
| cyber / sx-superuser | .80 | .81 | .42 | **.33** | .56 | **.66** |
| cyber / sx-askubuntu | .80 | .80 | .44 | **.32** | .53 | **.64** |
| e-comm / tgbl-review | .84 | .84 | .36 | .35 | .46 | .48 |

(real-side B and recurrence are engine-independent and reproduce exactly:
B real .78/.68/.76/.69/.63/.66/.41; recurrence real 0.925/0.661/0.582/0.526/0.359/
0.381/0.043 -- identical to the frozen driver; email-Eu's 0.925 is the tie the
paper prints as .92.)

### Cells that moved beyond rounding

- **crypto fid down**: wiki-talk .85 -> .83 (0.848 -> 0.829, -0.019),
  sx-mathoverflow .73 -> .72 (0.730 -> 0.717, -0.013). Cause: the P2-b one-sided
  crypto difficulty-damping fix (same direction as the matrix crypto cell
  0.843 -> 0.827; see `docs/remeasure_matrix_agora.md`). This is a
  correctness/interpretability fix that slightly lowers fidelity.
- **cyber synth burstiness down**: sx-superuser B_s .42 -> .33, sx-askubuntu
  B_s .44 -> .32. AGORA cyber traffic is less bursty than at submission.
- **synth recurrence up** for crypto/cyber: wiki-talk .80 -> .85,
  sx-superuser .56 -> .66, sx-askubuntu .53 -> .64 (AGORA a touch more recurrent).
  These preserve the paper's qualitative claim (AGORA slightly MORE recurrent than
  real) and in fact widen it for cyber.
- **finance barely moved** (the row the fix targeted): fid .80/.79 -> .80/.80,
  B unchanged, recurrence email-Eu .97 -> .96 / CollegeMsg .82 -> .83. As
  anticipated, at anomaly-rate 0.02 the finance homophily=0.95 redirect only
  touches marked-actor edges, so the aggregate fidelity is essentially unchanged
  (+0.003 / -0.002). No visible finance shift.

All other cells are within +/-0.005 (rounding at 2 dp). The final numbers match the
independent fixed-engine run in `scratchpad/fidelity_after.json` (Jul 17) to within
0.002 on fidelity, confirming reproducibility.

## 4. True reciprocity (for completeness, not a paper column)

`compute_stats.reciprocity` real/synth, final engine: email-Eu .71/.30,
CollegeMsg .64/.12, wiki-talk .32/.00, sx-mathoverflow .38/.00, sx-superuser
.38/.13, sx-askubuntu .39/.12, tgbl-review .00/.00. AGORA under-reciprocates
across the board (an honest, unchanged gap); the paper does not print this column.

## 5. Range check for the abstract

Abstract states fidelity "0.73--0.85 across seven real graphs."

- Frozen range: min 0.730 (sx-mathoverflow), max 0.848 (wiki-talk) -> "0.73--0.85".
- **Final range: min 0.717 (crypto / sx-mathoverflow), max 0.838 (e-comm /
  tgbl-review) -> "0.72--0.84".**

The abstract's exact "0.73--0.85" no longer holds on the final engine: both
endpoints dropped ~0.01 (the low end to 0.717 as crypto/mathoverflow fell with the
P2-b damping; the high end to 0.838 because wiki-talk fell from 0.848 to 0.829, so
tgbl-review's 0.838 is now the max). The honest new range is **0.72--0.84**. All
seven rows remain in a tight [0.72, 0.84] band; the overall realism claim stands,
only the two boundary digits shift.

## 6. Reproduce

```bash
cd python && python3 <scratchpad>/fid_final.py    # BIN=target/release/agora, seed 42
```
The driver deletes each generated graph directory immediately after scoring; no
graph output is left on disk and nothing is written into the repo tree.
