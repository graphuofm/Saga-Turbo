# Camera-ready revision plan (fixed engine)

Status: PLANNING ONLY. `paper/main.tex` and all frozen submission numbers are
untouched. This file records, section by section, how the paper should change
once the fixed-engine re-measurement is complete, so the rewrite can be done in
one pass when the numbers are all in. Every number below traces to a real
seed-42 run recorded in `docs/remeasure_fidelity.md` and
`docs/remeasure_sweeps.md` (and `docs/remeasure_downstream_learned.md` when the
in-flight clean run finishes).

The submission was frozen on the pre-fix engine (git HEAD `c438363`) and the
anonymized artifact mirrors that. Camera-ready swaps in the fixed engine, so
every table that the fix moves must be re-stated. Below, each item is tagged:
STRENGTHENS (the fix makes the claim more defensible), NEUTRAL (number moves,
story intact), or NEW LIMITATION (an honest gap to disclose).

---

## 1. g(pi) elasticity  --  the largest change

**Frozen submission says:** realized anomalous-edge fraction g(pi) ~= 3*pi;
tabulated ~3.1 / 6.8 / 15.9 / 30.0 % at pi = 0.01 / 0.02 / 0.05 / 0.10, with the
0.30 cap binding at pi = 0.10.

**Fixed engine measures (1M edges, seed 42):** gentler and domain-dependent, cap
never binds (max realized 4.4 %):

| domain | 0.005 | 0.01 | 0.02 | 0.05 | 0.10 | slope |
|---|---|---|---|---|---|---|
| finance | 0.036 | 0.077 | 0.157 | 0.408 | 0.819 % | ~0.082*pi |
| crypto | 0.006 | 0.010 | 0.024 | 0.062 | 0.126 % | ~0.013*pi |
| cyber | 0.006 | 0.017 | 0.028 | 0.067 | 0.133 % | ~0.013*pi |
| ecommerce | 0.201 | 0.414 | 0.861 | 2.137 | 4.338 % | ~0.43*pi |
| healthcare | 0.221 | 0.393 | 0.820 | 2.069 | 4.414 % | ~0.43*pi |
| transport | 0.010 | 0.022 | 0.034 | 0.090 | 0.191 % | ~0.019*pi |

**How to rewrite:** this is NEUTRAL-to-STRENGTHENS, not a loss. Reframe the
section around what the fix actually buys:
- Drop the "g(pi) ~= 3*pi" universal-slope claim and the "cap binds at pi=0.10"
  sentence. The 0.30 cap is now a safety rail that does not bind in the tested
  range; state it as such, not as an operating point.
- Lead with the invariance property that motivated `activity_multiplier` in the
  first place: realized anomaly fraction is a smooth monotone function of pi that
  is invariant to graph size (the pre-fix elasticity leaked into `--edges`; the
  fix removes that). This is the real methodological point and it now holds
  cleanly.
- Present the per-domain slope as a feature: each domain's slope reflects its
  base activity level (ecommerce/healthcare are high-throughput, so a given pi
  buys more anomalous edges; crypto/cyber are sparse-adversary, so less). This is
  more honest than a single fabricated-looking 3x slope.
- Keep the monotonicity claim (all six domains monotone in pi) -- it is the load-
  bearing property for "pi is a usable control knob" and it survives.

---

## 2. Difficulty / camouflage (tab:diff)  --  switch the lead metric to AP

**Frozen submission says:** detector ROC-AUC across difficulty for
structure-only / attribute-only / full feature views; the structure row is flat
~0.94 (already corrected in the freeze), attribute and full near 0.99.

**Fixed engine measures (finance 500k, pi=0.05, GBM, 70/30 stratified, emitted
edge base rate ~0.4%). Full 3x5 sweep, both metrics (docs/remeasure_diff_ap.md):**

AP (test):
| view | d=0 | 0.25 | 0.5 | 0.75 | 1.0 |
|---|---|---|---|---|---|
| attribute-only | 0.402 | 0.350 | 0.270 | 0.164 | 0.091 |
| structure-only | 0.056 | 0.055 | 0.054 | 0.055 | 0.067 |
| full           | 0.532 | 0.509 | 0.388 | 0.308 | 0.381 |

ROC-AUC (test):
| view | d=0 | 0.25 | 0.5 | 0.75 | 1.0 |
|---|---|---|---|---|---|
| attribute-only | 0.843 | 0.843 | 0.878 | 0.920 | 0.923 |
| structure-only | 0.941 | 0.941 | 0.938 | 0.938 | 0.948 |
| full           | 0.868 | 0.914 | 0.916 | 0.905 | 0.914 |

Key finding, stronger than the frozen prose: at a ~0.4% base rate ROC-AUC does
not merely understate the knob, it INVERTS it. Attribute-only AUC RISES
0.843 -> 0.923 while the anomalies are provably getting harder to catch; only AP
shows the true monotone decline (attribute-only 0.402 -> 0.091, 4.4x). The frozen
table's attribute/full rows (0.995 -> 0.968 etc.) are pre-fix and do NOT
reproduce on the fixed engine; only the structure-only ~0.94 AUC row carries over.

**How to rewrite:** NEUTRAL-to-STRENGTHENS. Re-state tab:diff on AP as the lead
metric (optionally keep AUC as a second block to make the inversion visible).
- Attribute-only AP falls monotonically 0.402 -> 0.091: this IS the difficulty
  evidence, and it matches the prose already in the paper.
- Structure-only: flat and difficulty-invariant (AUC ~0.94), but its AP is near
  floor (~0.05) because positives are rare. Frame honestly: structure carries a
  weak, difficulty-independent signal; the knob acts on attributes, not
  structure. Do NOT say "structure-only stays near 0.94" without saying that is
  AUC and its AP is ~0.05, or it reads as a strong structural detector when it is
  not.
- Full AP declines 0.532 -> ~0.31-0.38 (a small non-monotone uptick at d=1.0;
  state the range, do not claim strict monotonicity).
- Add one sentence on WHY AUC is the wrong lens: under a ~0.4% base rate AUC is
  dominated by easy negatives and here even moves the wrong way, while AP tracks
  the precision collapse. This reads as rigor. This is now a measured claim, not
  a hand-wave, so the frozen prose's "understated by ROC-AUC" can be upgraded to
  "inverted by ROC-AUC" with the table to back it.

---

## 3. Downstream single-task (tab:downstream)  --  minor cell updates

**Fixed engine reproduces the frozen table** (CollegeMsg-matched):
- TGN: real 0.841 (frozen 0.843), SAGA 0.927 (frozen 0.943), BA 0.622 (0.611).
- EdgeBank_inf: 0.764 / 0.942 / 0.500 -- exact reproduction.
- repeated-edge fraction: 0.661 / 0.829 / 0.000.

**How to rewrite:** NEUTRAL. Update two cells only: SAGA TGN 0.943 -> 0.927 and
SAGA repeated-edge 0.94 -> 0.829 (the 0.829 already matches VALIDATION.md's 0.827
at this scale). Ordering SAGA ~= real >> BA is preserved, so the surrounding
prose ("AGORA supports the same downstream ranking as the real graph") stands.
The prose sentence "94% of its edges ... versus 66% in the real graph" must
become "83% ... versus 66%."

---

## 4. Fidelity vs real (anomaly structure + distributions)

**Cured overshoots (STRENGTHENS):**
- Anomaly degree ratio: 12.9x (pre-fix) -> 0.52-0.56x now (real 0.87x). The
  pathological super-hub anomalies are gone; AGORA now slightly *under*-shoots.
- Anomaly in/out magnitude: at like-for-like Elliptic size, 1.22 vs real 1.01
  (pre-fix was 16.55 vs 1.71). Overshoot essentially closed.
- Repeated-edge fraction: 0.94 -> 0.829 vs real 0.661. Closer to real.

If the frozen paper cited the 12.9x or the in/out overshoot as a known
limitation, camera-ready should move those from "limitation" to "resolved," and
report the new near-real values.

**Unchanged (NEUTRAL):**
- C2ST / discriminative score: 0.9994-0.9997, still near-perfectly separable. The
  Limitations paragraph ("fidelity should be read as marginal, not as
  indistinguishability; AUC ~ 1.0 on all matched graphs") stands verbatim.
- Finance aggregate fidelity score: 0.805, unchanged from frozen ~0.80.

**NEW LIMITATION to disclose:**
- Anomaly homophily. Using anom.py's definition (P(dst marked | src marked)
  enrichment), AGORA finance = 2.25 vs real Elliptic = 13.27. Real illicit
  transactions are strongly homophilous (illicit talks to illicit); AGORA's
  anomalies under-cluster. Add one honest sentence to Limitations: AGORA
  reproduces anomaly *degree* and *repetition* structure but under-produces
  anomaly *homophily*, so community-structure-dependent detectors may find AGORA
  anomalies easier to isolate than real ones. (This is the flip side of the cured
  degree overshoot: the recruitment fix stopped anomalies from forming super-hubs
  but did not make them preferentially link to each other.)

---

## 5. Throughput / scaling  --  REPRODUCES frozen, no change

Clean single-tenant re-measure done (i9-13900K, loadavg ~4). 1e7 = 2.02s
(4.94 M/s), 1e8 = 18.999s (5.25 M/s), peak RAM 0.28/1.35 GB. All squarely in the
frozen tab:scale range (5-6 M/s, ~0.26/1.34 GB). The contended 48.35s/100M was
pure CPU starvation, not an engine regression. NEUTRAL: keep the frozen
throughput claim; optionally refresh the exact numbers to the clean values above.

## 5b. Downstream multi (tab:downmulti)  --  REPRODUCES frozen, no change

Clean six-domain re-measure done. TGN-on-SAGA mean 0.965 vs frozen 0.964 (per
domain within +/-0.003); EdgeBank-on-SAGA reproduces the frozen numbers to the
third decimal in all six domains; SAGA >> BA ordering holds everywhere. Only the
random BA baseline wobbles (TGN-BA mean 0.681 vs 0.697, normal run-to-run
variance on a signal-free graph). NEUTRAL: no cells need changing. Numbers in
`docs/remeasure_downstream_learned.md`.

---

## 6. Learned-generator contest + 10x6 matrix  --  PENDING

The learned-generator toy-scale contest (TIGGER-Lite/CTGAN/TVAE vs AGORA) is
being re-run clean. If the fixed-engine AGORA fidelity at toy scale shifts, the
fig_matrix AGORA row and the "learned generators only run at toy scale" claim may
need updated cells. Hold the matrix until that run reports.

---

## 7. Non-number camera-ready items

- Binary rename: paper already says `agora`; the fixed-engine tree builds `agora`
  and the wheel is `agora`. The anonymized artifact still ships the `saga`
  binary; camera-ready needs a fresh de-anonymized artifact snapshot built from
  the fixed tree.
- SAGA citation: already de-anonymized in the working bib (Ding and Zhang, VLDB
  2026 demo). Fill in PVLDB volume/pages/DOI once the proceedings are out.
- Availability statement: replace the anonymous.4open.science URL with the
  de-anonymized GitHub URL.
- Any TODO items recorded by the consolidation (priv_esc threat-intel rate, the
  crypto k<1 difficulty-sign question) are engine-improvement items, not
  camera-ready blockers; decide separately whether to address or to scope out.

---

## One-pass rewrite order (when all numbers are in)

1. g(pi) table + elasticity prose (biggest, sets the tone).
2. tab:diff -> AP, plus the base-rate sentence.
3. tab:downstream two cells + the "94% -> 83%" prose fix.
4. Fidelity: move cured overshoots out of limitations; add the homophily
   limitation; keep the C2ST paragraph.
5. Throughput numbers (after clean run).
6. Matrix / learned contest (after clean run).
7. Availability + artifact + citation de-anonymization (last, at camera-ready
   deadline).
