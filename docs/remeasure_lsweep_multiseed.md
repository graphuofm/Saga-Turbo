# Multi-seed L sweep with confidence intervals

Purpose: answer the advisor review's section 7.1-A ("one seed is not enough for a
monotonicity claim") and 19-Experiment-2. Every number below is from a real run
of `./target/release/saga` on this host. Guards on, so every run exercises the
state-conditioned admission path.

## Protocol

    saga generate --domain D --nodes 20000 --edges 1000000 --time-span 30 \
        --seed S --guards --block-len-days L --json

Six domains (finance, crypto, cyber, ecommerce, healthcare, transport), five
seeds (42, 7, 20260824, 1337, 99991), seven block lengths L in days:
1 tick (1 s = 1.157e-05 d), 0.001, 0.01, 0.1, 1, 7, and 30 (the no-feedback
endpoint, equal to the span). All 210 runs completed with no error. No output
was written (JSON summary only), so no graphs were generated or deleted.

Reported metric: the guard leakage rate, that is the fraction of emitted guarded
events that violate the declared constraint because the guard read a stale
snapshot (`overdraft_violations / overdraft_guarded`). This is distinct from the
global constraint violation rate over all event types, which is measured
separately by `saga validate` (see VERIFICATION_MATRIX.md).

Aggregates over the five seeds: mean, 95% t-interval half width, and median.


## scale=1000000  (210 good runs)

| domain | L | leak% mean | 95% CI | median | wall s mean | edges mean |
|---|---|---:|---:|---:|---:|---:|
| crypto | 1s | 0.001 | +/-0.001 | 0.002 | 1.474 | 153387 |
| crypto | 0.001 | 0.157 | +/-0.016 | 0.161 | 0.182 | 152109 |
| crypto | 0.01 | 1.335 | +/-0.063 | 1.317 | 0.154 | 145332 |
| crypto | 0.1 | 6.215 | +/-0.455 | 6.122 | 0.152 | 140842 |
| crypto | 1 | 21.872 | +/-1.431 | 21.621 | 0.152 | 153965 |
| crypto | 7 | 67.593 | +/-1.423 | 67.580 | 0.192 | 296223 |
| crypto | span | 90.628 | +/-0.552 | 90.684 | 0.348 | 911402 |
| cyber | 1s | 0.001 | +/-0.000 | 0.001 | 2.046 | 589502 |
| cyber | 0.001 | 0.135 | +/-0.004 | 0.135 | 0.301 | 587843 |
| cyber | 0.01 | 1.288 | +/-0.024 | 1.282 | 0.172 | 578251 |
| cyber | 0.1 | 5.698 | +/-0.079 | 5.679 | 0.159 | 570694 |
| cyber | 1 | 10.054 | +/-0.100 | 10.032 | 0.162 | 568260 |
| cyber | 7 | 29.678 | +/-0.268 | 29.670 | 0.208 | 606873 |
| cyber | span | 70.002 | +/-0.357 | 70.139 | 0.378 | 962830 |
| ecommerce | 1s | 0.004 | +/-0.001 | 0.003 | 1.933 | 738248 |
| ecommerce | 0.001 | 0.329 | +/-0.047 | 0.317 | 0.284 | 738082 |
| ecommerce | 0.01 | 2.529 | +/-0.246 | 2.436 | 0.157 | 737389 |
| ecommerce | 0.1 | 7.325 | +/-0.465 | 7.193 | 0.144 | 737073 |
| ecommerce | 1 | 14.052 | +/-0.441 | 13.821 | 0.145 | 736252 |
| ecommerce | 7 | 38.868 | +/-0.700 | 38.605 | 0.187 | 758665 |
| ecommerce | span | 76.001 | +/-0.394 | 76.051 | 0.329 | 949645 |
| finance | 1s | 0.002 | +/-0.001 | 0.002 | 1.285 | 883245 |
| finance | 0.001 | 0.251 | +/-0.013 | 0.252 | 0.231 | 882439 |
| finance | 0.01 | 1.928 | +/-0.034 | 1.930 | 0.173 | 876859 |
| finance | 0.1 | 6.476 | +/-0.092 | 6.498 | 0.164 | 864157 |
| finance | 1 | 14.293 | +/-0.180 | 14.351 | 0.167 | 840208 |
| finance | 7 | 29.170 | +/-0.602 | 28.964 | 0.208 | 818569 |
| finance | span | 53.723 | +/-1.169 | 53.786 | 0.304 | 960664 |
| healthcare | 1s | 0.001 | +/-0.003 | 0.000 | 2.425 | 885547 |
| healthcare | 0.001 | 0.051 | +/-0.018 | 0.051 | 0.356 | 885549 |
| healthcare | 0.01 | 0.391 | +/-0.036 | 0.375 | 0.220 | 885587 |
| healthcare | 0.1 | 1.481 | +/-0.097 | 1.460 | 0.213 | 885736 |
| healthcare | 1 | 8.442 | +/-0.506 | 8.566 | 0.215 | 887476 |
| healthcare | 7 | 40.178 | +/-1.965 | 40.825 | 0.259 | 905237 |
| healthcare | span | 80.549 | +/-1.174 | 80.992 | 0.413 | 1013724 |
| transport | 1s | 0.001 | +/-0.001 | 0.000 | 1.990 | 876582 |
| transport | 0.001 | 0.068 | +/-0.020 | 0.066 | 0.296 | 876578 |
| transport | 0.01 | 0.525 | +/-0.034 | 0.517 | 0.186 | 876585 |
| transport | 0.1 | 1.890 | +/-0.145 | 1.894 | 0.182 | 876790 |
| transport | 1 | 5.781 | +/-0.199 | 5.716 | 0.185 | 877995 |
| transport | 7 | 30.874 | +/-0.401 | 30.788 | 0.221 | 897828 |
| transport | span | 70.357 | +/-0.886 | 70.416 | 0.345 | 1000364 |

### monotonicity: 30/30 (domain,seed) series monotone non-decreasing in L

## Reading

Guard leakage grows with the staleness bound in every domain, and the growth is
monotone non-decreasing in L for every one of the 30 (domain, seed) series, with
no exception. The tick reference leaks at most 0.004% (tick ties only). Leakage
at the no-feedback endpoint reaches 53.7% (finance) to 90.6% (crypto).

Wall time reproduces the shape reported earlier at a single seed: the tick
reference is 6 to 13 times the plateau, the plateau is flat from L = 0.01 to
L = 1 day, and the no-feedback endpoint is again slower than the plateau because
stale guards admit more work (its emitted-edge count is the highest in every
domain).

Scope: this table measures guard leakage, emitted edges and wall time. Final
state divergence per L and the fixed-emitted-edge control are recorded
separately; both are pending at the time of writing.

## Second scale: 1e7 target edges (100,000 nodes, 30-day span), finance and crypto

## scale=10000000  (70 good runs)

| domain | L | leak% mean | 95% CI | median | wall s mean | edges mean |
|---|---|---:|---:|---:|---:|---:|
| crypto | 1s | 0.002 | +/-0.001 | 0.002 | 5.514 | 1174746 |
| crypto | 0.001 | 0.160 | +/-0.007 | 0.162 | 1.340 | 1164847 |
| crypto | 0.01 | 1.346 | +/-0.039 | 1.337 | 1.332 | 1112300 |
| crypto | 0.1 | 6.582 | +/-0.171 | 6.557 | 1.333 | 1074212 |
| crypto | 1 | 30.038 | +/-0.582 | 30.034 | 1.334 | 1230374 |
| crypto | 7 | 77.222 | +/-0.625 | 77.218 | 1.631 | 2770149 |
| crypto | span | 93.806 | +/-0.183 | 93.807 | 3.207 | 9111266 |
| finance | 1s | 0.003 | +/-0.000 | 0.003 | 4.657 | 8729455 |
| finance | 0.001 | 0.318 | +/-0.004 | 0.318 | 1.342 | 8718184 |
| finance | 0.01 | 2.439 | +/-0.032 | 2.437 | 1.195 | 8649505 |
| finance | 0.1 | 8.857 | +/-0.084 | 8.814 | 1.175 | 8463780 |
| finance | 1 | 19.725 | +/-0.099 | 19.700 | 1.043 | 8128916 |
| finance | 7 | 36.580 | +/-0.242 | 36.568 | 1.458 | 7951990 |
| finance | span | 60.338 | +/-0.553 | 60.382 | 2.236 | 9590146 |

### monotonicity: 10/10 (domain,seed) series monotone non-decreasing in L
