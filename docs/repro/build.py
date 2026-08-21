import json

NORMAL = {  # scale-1 mean events/day per actor of that type, from SAGA_DEBUG_RATES=1 (edges=200k, seed 42)
 ("finance","account"): 6.903531, ("crypto","eoa"): 445.384897,
 ("cyber","host"): 328.609694, ("cyber","user"): 89.666579, ("cyber","process"): 22.358517,
 ("transport","vehicle"): 16.955094, ("ecommerce","user"): 0.265121, ("healthcare","provider"): 17.232717,
}

# quote bank
Q = {
 "ctr": ('31 CFR 1010.311 (Filing obligations for reports of transactions in currency)',
   '"Each financial institution other than a casino shall file a report of each deposit, withdrawal, exchange of currency or other payment or transfer, by, through, or to such financial institution which involves a transaction in currency of more than $10,000, except as otherwise provided in this section."',
   'corpus/text/finance/fincen_31cfr_chx.txt @ ~8,051,600'),
 "struct": ('31 CFR 1010.100(xx) (definition of "structure (structuring)")',
   '"...to conduct one or more transactions in currency, in any amount, at one or more financial institutions, on one or more days, in any manner, for the purpose of evading the reporting requirements... The transaction or transactions need not exceed the $10,000 reporting threshold at any single financial institution on any single day in order to constitute structuring within the meaning of this definition."',
   'corpus/text/finance/fincen_31cfr_chx.txt @ ~8,023,700'),
 "erc20": ('ERC-20 Token Standard, `approve` (EIP-20)',
   '"Allows `_spender` to withdraw from your account multiple times, up to the `_value` amount. If this function is called again it overwrites the current allowance with `_value`."',
   'corpus/text/crypto/ethereum_ercs.txt (erc-20.md)'),
 "capec": ('CAPEC — TCP SYN / port scanning',
   '"Unlike TCP Connect scanning, it is possible to scan thousands of ports per second using this method"',
   'corpus/text/cyber/mitre_capec.txt'),
 "capec49": ('CAPEC-49 Password Brute Forcing',
   '"an adversary can brute force this password in maximum 1+26+26^2+26^3+26^4+26^5 = ... = 12,356,631 attempts, and half these tries (6,178,316) on average"',
   'corpus/text/cyber/mitre_capec.txt'),
 "mue": ('CMS NCCI Medically Unlikely Edits — Practitioner Services (MUE table, eff. 07-01-2026)',
   '"HCPCS/CPT Code | Practitioner Services MUE Values | MUE Adjudication Indicator | MUE Rationale | 0001U | 1 | 2 Date of Service Edit: Policy | ..." (per-code max units of service per beneficiary per date of service)',
   'corpus/text/healthcare/cms_ncci_mue_practitioner.txt'),
 "t1110": ('MITRE ATT&CK T1110.003 Password Spraying',
   '"Logins are attempted with that password against many different accounts on a network to avoid account lockouts that would normally occur when brute forcing a single account with many passwords." (no rate stated)',
   'corpus/text/cyber/mitre_attack_enterprise.txt'),
 "t1071": ('MITRE ATT&CK T1071 / T1568 (DNS beaconing)',
   '"DNS beacons may be difficult to detect because the beacons infrequently communicate with infected devices." (qualitative only; no interval stated)',
   'corpus/text/cyber/mitre_attack_enterprise.txt'),
 "ftc465": ('FTC 16 CFR Part 465 — Rule on the Use of Consumer Reviews and Testimonials',
   '"It is an unfair or deceptive act or practice and a violation of this part for a business to write, create, or sell a consumer review..." (conduct prohibition; the Part states no count, frequency, or rate anywhere in its 14 chunks)',
   'corpus/text/ecommerce/ftc_16cfr_465.txt'),
 "fhwa": ('FHWA Traffic Analysis Toolbox Vol. III',
   '"...signalized intersections was 1800 vehicles per hour of green per lane (vphgpl)" (network capacity; nothing on spoofing or probe-report rates)',
   'corpus/text/transport/fhwa_tat_vol3.txt'),
 "none": (None, None, None),
}

NOT_IN_STD = "Standard/corpus states no rate, frequency, or count-in-window for this behavior."

S = []
def add(dom, intent, stage, actor, kmin, kmax, scope, dur, ring, cls, quote_key, note, derived=None, assumption=None):
    n = NORMAL[(dom,actor)]
    S.append(dict(domain=dom, intent=intent, stage=stage, actor=actor,
        current_k=[kmin,kmax], current_implied_rate_per_day=[round(kmin*n,3), round(kmax*n,3)],
        normal_actor_rate_per_day_scale1=n, scope=scope, duration_days=dur, ring_size=ring,
        classification=cls,
        derived_events_per_day=derived, derived_implied_k=(None if derived is None else [round(d/n,4) for d in derived]),
        assumption=assumption,
        source_citation=Q[quote_key][0], source_quote=Q[quote_key][1], source_locator=Q[quote_key][2],
        note=note))

C_FIN = "Modelling choice, not grounded. 31 CFR 1010.100(xx) is affirmatively rate-free — it says structuring may be 'one or more transactions ... on one or more days'. FATF text is NOT in the corpus (fetch produced provenance only). Recommend the neutral default k=1."
# ---------------- FINANCE ----------------
add("finance","structuring","smurf_deposits","account",0.1449,0.5794,"normal","7-21","3-12","C","struct",
 "AMOUNT is grounded (override $8,200-$9,900 sits under the $10,000 CTR threshold of 31 CFR 1010.311 — cite Q['ctr']). RATE is not. IMPORTANT: the finance.yaml `ctr_structuring_window` constraint (k:4, floor:$7,000, window 7d) is presented as '31 CFR 1010.311' but NEITHER the count-of-4, the 7-day window, NOR the $7,000 floor appears in the CFR. The premise that this yields a grounded 0.57 deposits/day is FALSE. Recommend k=1 (neutral default) and re-label the constraint's k/floor/window as a modelling choice.")
add("finance","structuring","aggregate_to_collector","account",0.1449,0.4346,"collector","2-6","3-12","C","struct",C_FIN)
add("finance","layering","chain_hops","account",0.5794,1.7382,"chain","3-10","4-10","C","none",C_FIN+" (Cited as 'FATF placement/layering typology' — the FATF 40 Recommendations fetch left only provenance.json; there is no retrievable text.)")
add("finance","fan_in_out","gather","account",0.8691,2.8971,"collector","1-5","5-25","C","none",C_FIN)
add("finance","fan_in_out","scatter","account",0.8691,2.8971,"victims 10-40","1-4","5-25","C","none",C_FIN)
add("finance","round_tripping","cycle","account",0.2897,0.8691,"chain","2-8","3-8","C","none",C_FIN)
add("finance","mule_network","receive","account",0.1449,0.5794,"collector","1-3","4-15","C","none",C_FIN)
add("finance","mule_network","forward","account",0.2897,0.8691,"victims 3-10","1-2","4-15","C","none",C_FIN)

# ---------------- CRYPTO ----------------
C_CRY = ("Modelling choice, not grounded. The crypto corpus contains NO typology text: FATF VASP guidance fetch FAILED (HTTP 403, provenance.json only); "
 "grep of sok_defi_attacks.txt for 'rug pull'/'wash trad'/'ponzi'/'pump' returns 0 hits; OFAC SDN/crypto-address lists are address enumerations (they ground PREVALENCE and the existence of mixers, not rates); "
 "ERC-20/721/1155 ground the event schema, not rates. Recommend the neutral default k=1.")
add("crypto","wash_trading","cycle_volume","eoa",0.0180,0.0674,"chain","2-10","3-10","C","none",C_CRY+" SEE DENOMINATOR BUG below — the eoa yardstick is 445/day, a MEV-bot-dominated mean.")
add("crypto","wash_trading","pool_wash","eoa",0.0135,0.0449,"normal","1-5","3-10","C","none",C_CRY)
add("crypto","phishing_drain","bait_approve","eoa",0.0011,0.0045,"victims 8-40","1-7","2-6","B","erc20",
 "DERIVABLE from mechanism, not from a stated rate. ERC-20 `approve` grants multi-use spend rights in ONE call, so the campaign needs exactly 1 approve per victim (2 if the spec's set-to-0-first note is honoured). ASSUMPTION: each victim approves once over the stage window.",
 derived=[8.0/7.0, 40.0/1.0], assumption="One approve per victim (ERC-20 allowance is multi-use); victims = 8-40; stage window = 1-7 days => 8/7=1.14/day to 40/1=40/day. This bounds the COUNT the mechanism requires; it is not a rate the standard states.")
add("crypto","phishing_drain","drain_to_collector","eoa",0.0090,0.0337,"collector","0.5-3","2-6","C","erc20",
 "ERC-20 grounds that ONE approve permits unbounded transferFrom draws ('withdraw from your account multiple times'), so the standard sets NO upper bound on drain count. Rate is a modelling choice; recommend k=1.")
add("crypto","mixing","deposit_fixed","eoa",0.0022,0.0112,"collector","1-6","4-14","C","none",C_CRY+" The fixed 10-unit denomination is a structural choice; OFAC listings name mixers but state no cadence.")
add("crypto","mixing","withdraw_fixed","eoa",0.0022,0.0090,"victims 5-20","2-12","4-14","C","none",C_CRY)
add("crypto","pump_and_dump","accumulate","eoa",0.0112,0.0449,"collector","1-4","5-25","C","none",C_CRY)
add("crypto","pump_and_dump","dump","eoa",0.0225,0.0898,"collector","0.25-1.5","5-25","C","none",C_CRY)
add("crypto","rug_pull","seed_liquidity","eoa",0.0011,0.0045,"normal","1-5","1-4","C","none",C_CRY)
add("crypto","rug_pull","pull","eoa",0.0045,0.0180,"normal","0.1-1","1-4","B","none",
 "DERIVABLE from mechanism. A rug pull is by definition a near-total single removeLiquidity: the stage requires ~1 event per operator, not a sustained rate. ASSUMPTION: one pull per operator per campaign. No corpus text states this (SoK DeFi paper has zero 'rug pull' hits) — the derivation is from the event's own semantics.",
 derived=[1.0/1.0, 1.0/0.1], assumption="Exactly 1 remove_liquidity per operator over a 0.1-1 day stage => 1/day to 10/day.")
add("crypto","ponzi","collect_deposits","eoa",0.0022,0.0135,"collector","10-40","6-30","C","none",C_CRY+" FLAGGED ABSURD: current k=0.0022-0.0135 makes a Ponzi operator ~100x LESS active than the average eoa.")
add("crypto","ponzi","payout_earlier","eoa",0.0011,0.0067,"ring","10-40","6-30","C","none",C_CRY+" FLAGGED ABSURD (same as above). Note this is the ONLY crypto stage with scope:!ring — the sole source of ring-internal crypto traffic (see homophily section).")

# ---------------- CYBER ----------------
add("cyber","recon_scan","sweep","host",0.3043,2.4345,"victims 30-200","0.1-2","1-3","C","capec",
 "Modelling choice, not grounded. CAPEC states a CAPABILITY CEILING ('thousands of ports per second'), not a campaign rate; using it as the rate would give ~10^8/day. MITRE T1046 states no rate. The current 100-800/day encodes a 'low-and-slow' choice. Recommend k=1 unless a scan-rate source is added.")
add("cyber","brute_force","hammer","user",2.2305,22.3049,"collector","0.05-1","1-4","C","capec49",
 "Modelling choice, not grounded. CAPEC-49 gives an exhaustive-search COUNT (6.18M attempts on average), which is a search-space size, not a rate — using it would give ~10^7/day. ATT&CK T1110.003 says spraying stays 'under lockout' but names no threshold. The natural grounding source (NIST SP 800-63B throttling guidance, <=100 failed attempts) is NOT in the corpus — CORPUS GAP. Recommend k=1 pending that source.")
add("cyber","lateral_movement","pivot","user",0.0223,0.1115,"victims 3-12","1-7","1-5","C","none",
 "Modelling choice, not grounded. MITRE T1021 is a prose technique description; ATT&CK states no rates anywhere (grep for 'interval'/'frequenc'/'sleep' returns only qualitative prose). Recommend k=1.")
add("cyber","c2_beacon","beacon","host",0.0730,0.2921,"collector","3-21","1-3","C","t1071",
 "Modelling choice, not grounded. ATT&CK T1071/T1568 say beaconing is 'periodic'/'infrequent' but state no interval. NIST SP 800-94's 'every 15 minutes, every 30 minutes' span is about NBA FLOW-BATCH COLLECTION LATENCY, not beacon cadence — do NOT cite it here. The YAML's '~hourly to ~15-min beacons' comment is an unsourced choice. Recommend k=1.")
add("cyber","exfiltration","stage_chunks","host",0.0152,0.0913,"collector","0.5-5","1-2","C","none",
 "Modelling choice, not grounded. MITRE T1041/T1029 describe scheduling ('at certain times of day or at certain intervals') without numbers. Recommend k=1.")
add("cyber","priv_esc","escalate","process",0.0895,0.5367,"chain","0.05-1","1-4","C","none",
 "Modelling choice, not grounded. MITRE T1068 states no rate. Mechanistically priv-esc is a small burst of execs, not a sustained rate — but no standard says how many. Recommend k=1.")

# ---------------- TRANSPORT ----------------
add("transport","gps_spoofing","spoofed_traversals","vehicle",1.1796,4.7183,"victims 5-20","0.5-3","2-8","C","fhwa",
 "Modelling choice, not grounded. FHWA TAT Vol.III / MUTCD Part 4 / NGSIM / SUMO ground vehicle DYNAMICS and traffic VOLUMES (the normal side and the physical-range constraints), never adversary probe rates. Recommend k=1. NOTE the corpus DOES ground the normal denominator (MUTCD Table 4C-1 veh/h warrants; FHWA 1800 vphgpl saturation flow) — a grounding opportunity for normal rates, not for k.")
add("transport","fake_trip_injection","phantom_trips","vehicle",0.5898,2.3592,"collector","1-7","3-12","C","fhwa",
 "Modelling choice, not grounded. Same corpus finding as gps_spoofing. Recommend k=1.")

# ---------------- ECOMMERCE ----------------
C_ECO = ("Modelling choice, not grounded. FTC 16 CFR 465 and 16 CFR 255 are conduct rules — a full read of 465 shows no count, frequency, or rate anywhere; "
 "16 CFR 255 and the FTC endorsement FAQ likewise return 0 rate-language hits. Ott et al. 2011 is a text-classification study of 400 reviews (linguistic features), not posting cadence. Recommend the neutral default k=1.")
add("ecommerce","fake_reviews","blitz","user",3.7719,15.0875,"victims 3-12","2-10","1","C","ftc465",C_ECO)
add("ecommerce","fake_reviews","camo_votes","user",1.8859,7.5437,"normal","1-6","1","C","ftc465",C_ECO)
add("ecommerce","collusion_ring","coordinated_flood","user",3.7719,11.3156,"collector","3-9","5-20","C","ftc465",C_ECO)
add("ecommerce","collusion_ring","mutual_helpful","user",1.8859,7.5437,"ring","2-7","5-20","C","ftc465",C_ECO+" This is one of only 3 ring-scoped stages in the whole rule base (see homophily section).")
add("ecommerce","collusion_ring","benign_spread","user",1.1316,3.7719,"normal","5-20","5-20","C","ftc465",C_ECO)
add("ecommerce","sockpuppet","puppet_reviews","user",1.8859,9.4297,"collector","4-18","3-10","C","ftc465",C_ECO)
add("ecommerce","sockpuppet","puppet_helpful","user",1.8859,5.6578,"ring","3-12","3-10","C","ftc465",C_ECO+" Ring-scoped (see homophily section).")
add("ecommerce","singleton_burst","one_shot_flood","user",3.0175,7.5437,"collector","0.5-3","8-30","B","none",
 "DERIVABLE from the intent's own definition, not from a standard. 'Singleton' means each account fires EXACTLY ONCE (Xie et al., KDD 2012 — a research paper, not a standard, and NOT in the corpus). ASSUMPTION: 1 review per account over the stage window.",
 derived=[1.0/3.0, 1.0/0.5], assumption="Exactly 1 review per account over a 0.5-3 day stage => 0.33/day to 2/day.")
add("ecommerce","return_fraud","buy","user",3.7719,11.3156,"normal","1-4","1-4","C","ftc465",C_ECO)
add("ecommerce","return_fraud","positive_review","user",1.8859,5.6578,"normal","1-3","1-4","C","ftc465",C_ECO)
add("ecommerce","return_fraud","refund","user",3.7719,11.3156,"normal","1-7","1-4","C","ftc465",C_ECO)

# ---------------- HEALTHCARE ----------------
add("healthcare","upcoding","inflate_intensity","provider",0.5803,1.3927,"normal","30-120","1-3","C","mue",
 "Modelling choice for RATE, not grounded. Upcoding does not change claim VOLUME at all — it changes the code on claims the provider already files. The corpus grounds the ATTRIBUTE (HCPCS/CPT code set), never the rate. NCCI MUE caps units per code per beneficiary per date of service; it says nothing about claims/day. Recommend k=1 — which here is not merely a default but SEMANTICALLY CORRECT: an upcoder bills at a normal cadence by construction.")
add("healthcare","phantom_billing","ghost_claims","provider",8.7044,23.2117,"victims 40-150","14-90","1-2","C","mue",
 "Modelling choice, not grounded. NCCI MUE is a per-code/per-beneficiary/per-date-of-service UNITS cap — it is a genuine count-in-a-window, but it constrains the `units` field, NOT a provider's claims/day. No corpus source states an implausible-daily-claim-volume threshold. The 150-400/day is unsourced. Recommend k=1 unless a per-provider volume source (e.g. CMS Provider Utilization & Payment PUF) is added — CORPUS GAP.")
add("healthcare","unbundling","split_components","provider",0.6963,1.7409,"normal","20-90","1-3","C","mue",
 "RATE is a modelling choice; the ATTRIBUTE is grounded. NCCI PTP edits (4,041 chunks) ground WHICH Column1/Column2 code pairs may not be billed together, and NCCI MUE grounds the max units per code per beneficiary per DOS — so the `units` override IS citable (A-grade for the attribute). Neither states claims/day. Like upcoding, unbundling does not change cadence by construction: recommend k=1.")
add("healthcare","kickback_ring","reciprocal_referrals","provider",0.0580,0.2321,"chain","30-120","4-12","C","none",
 "Modelling choice, not grounded. 42 CFR 1001 (exclusion authorities / AKS safe harbors) prohibits remuneration for referrals; it states no referral frequency. Recommend k=1.")
add("healthcare","kickback_ring","steered_prescriptions","provider",0.1161,0.3482,"collector","30-120","4-12","C","none",
 "Modelling choice, not grounded. 42 CFR 1001 / CMS Pub 100-04 Ch.1 state no prescribing rate.")
add("healthcare","kickback_ring","cover_claims","provider",0.1741,0.4642,"normal","30-120","4-12","C","none",
 "Modelling choice, not grounded. Same as above.")
add("healthcare","doctor_shopping","prescriber_fanin","provider",0.0174,0.0580,"collector","20-90","5-15","C","none",
 "Modelling choice, not grounded — BUT THIS IS THE BEST CORPUS-GAP CANDIDATE IN THE WHOLE RULE BASE. CMS's Part D Overutilization Monitoring System (OMS) publishes an explicit count-in-a-window criterion for opioid doctor/pharmacy shopping (beneficiaries meeting an MME threshold with >=N prescribers and >=M pharmacies over a lookback). That source is NOT in the corpus. Fetching the CMS OMS / Part D Opioid Overutilization guidance would make this stage a genuine (A) and would also ground `ring_size` (distinct prescribers). Until then: recommend k=1, labelled a modelling choice.")

for s in S:
    s["current_k_absurd_flag"] = bool(s["current_k"][1] < 1.0)

json.dump({"stages":S}, open('./scratch/grounded_rates.json','w'), indent=2)
from collections import Counter
print(len(S), Counter(s['classification'] for s in S))
print("k<1 everywhere:", sum(1 for s in S if s['current_k_absurd_flag']))
