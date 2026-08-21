#!/usr/bin/env python3
"""LLM-size ablation: does the RAG->rules->graph pipeline depend on drafter LLM size?
Only the LLM changes (drafter + extractor forced to the same model); corpus, retrieval,
generate seed/size are fixed. Measures: (1) pipeline validity, (2) grounded key-param
correctness vs the standard (31 CFR 1010.311), (3) end-to-end graph fidelity vs real."""
import sys, os, time, json, shutil, subprocess
sys.path.insert(0, "./python")
from pathlib import Path
import numpy as np
import saga_rag.draft as draft
import saga_rag.extract as extract
import saga_rag.pipeline as pipeline
from saga_eval.load import load_real, load_saga
from saga_eval.stats import compute_stats
from saga_eval.compare import compare

ROOT = Path("./")
SP = Path("./scratch")
BIN = str(ROOT / "target/release/saga")
CORPUS = ROOT / "corpus"
DESC = ("US retail banking payment network with anti-money-laundering monitoring: "
        "accounts and institutions, wire transfers and cash deposits, and AML "
        "typologies including structuring, layering, and mule rings.")
# ground truth from 31 CFR 1010.311 / FinCEN (what the standard actually says)
GT = {"ctr_threshold": 10000.0, "structuring_floor": 7000.0,
      "structuring_window_days": 7.0, "structuring_count": 4.0}

MODELS = [m for m in sys.argv[1:]] or ["qwen2.5:0.5b"]
SIZES = {"qwen2.5:0.5b":0.5,"qwen2.5:1.5b":1.5,"qwen2.5:3b":3.0,"qwen2.5:7b":7.0,"qwen2.5:14b":14.0,"qwen2.5:32b":32.0}

# real reference (fixed)
rs, rd, rt = load_real(str(ROOT/"realdata/snap-email-eu/email-Eu-core-temporal.txt"))
rstat = compute_stats(rs, rd, rt)
n = int(np.unique(np.concatenate([rs, rd])).size); m = int(rs.size)
span_days = round((rt.max()-rt.min())/86400.0, 1)

rows = []
for model in MODELS:
    draft._ollama_available = (lambda timeout=2.0, _m=model: _m)   # force drafter LLM
    extract._ollama_model   = (lambda timeout=2.0, _m=model: _m)   # force extractor LLM
    out = SP / f"rb_{model.replace(':','_').replace('.','')}.yaml"
    t0 = time.time()
    try:
        rep = pipeline.build(DESC, out, CORPUS, domain_hint="finance", k=12, prefer_llm=True)
    except Exception as e:
        print(f"[{model}] BUILD ERROR {type(e).__name__}: {e}", flush=True); continue
    dt = time.time() - t0
    ex = {}
    for e in (getattr(rep, "extracted", None) or []):
        nm = getattr(e, "name", None); val = getattr(e, "value", None); gr = getattr(e, "grounded", False)
        if nm is not None and gr:
            ex[nm] = val
    fid = float("nan")
    if getattr(rep, "rust_ok", False):
        gdir = SP / f"g_{model.replace(':','_')}"
        shutil.rmtree(gdir, ignore_errors=True)
        r = subprocess.run([BIN, "generate", "--domain", str(out), "--nodes", str(n),
                            "--edges", str(m), "--time-span", str(span_days), "--seed", "42",
                            "--out", str(gdir), "--quiet"], stderr=subprocess.PIPE)
        try:
            ss, sd, st = load_saga(str(gdir))
            fid = round(compare(rstat, compute_stats(ss, sd, st))["fidelity_score"], 3)
        except Exception as e:
            print(f"   gen/fid err: {e}")
        shutil.rmtree(gdir, ignore_errors=True)
    rows.append(dict(model=model, size_B=SIZES.get(model), valid=getattr(rep,"rust_ok",False),
                     mode=getattr(rep,"drafter_mode",""), fidelity=fid, params=ex, secs=round(dt)))
    print(f"[{model}] size={SIZES.get(model)}B valid={getattr(rep,'rust_ok',False)} "
          f"mode={getattr(rep,'drafter_mode','')} fid={fid} secs={dt:.0f}\n   params={ex}", flush=True)
    json.dump(rows, open(SP/"llm_ablation.json","w"), indent=2, default=str)

print("\n=== LLM-SIZE ABLATION ===")
print(f"{'model':16s}{'sizeB':>6}{'valid':>7}{'mode':>10}{'fidelity':>9}")
for r in rows:
    print(f"{r['model']:16s}{str(r['size_B']):>6}{str(r['valid']):>7}{r['mode']:>10}{str(r['fidelity']):>9}")
print("GT params:", GT)
print("saved llm_ablation.json")
