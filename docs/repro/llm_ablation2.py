#!/usr/bin/env python3
"""LLM-size ablation, v2: isolate the LLM DRAFT quality (no template fallback).
For each model, use ONLY its draft_llm output, ground the factual params (as the
real pipeline does), lint + Rust-validate, and if valid, generate a graph and score
fidelity vs real. Measures whether a bigger drafter LLM yields a more valid / higher
-fidelity rule base, or whether grounding+validator make size irrelevant."""
import sys, os, time, json, shutil, subprocess
sys.path.insert(0, "./python")
from pathlib import Path
import numpy as np
import saga_rag.draft as draft
import saga_rag.extract as extract
import saga_rag.grounding as grounding
import saga_rag.validate_compile as vc
from saga_rag.retrieve import Retriever
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
SIZES = {"qwen2.5:0.5b":0.5,"qwen2.5:1.5b":1.5,"qwen2.5:3b":3.0,"qwen2.5:7b":7.0,"qwen2.5:14b":14.0}
MODELS = sys.argv[1:] or ["qwen2.5:0.5b"]

rs, rd, rt = load_real(str(ROOT/"realdata/snap-email-eu/email-Eu-core-temporal.txt"))
rstat = compute_stats(rs, rd, rt)
n = int(np.unique(np.concatenate([rs, rd])).size); m = int(rs.size)
span_days = round((rt.max()-rt.min())/86400.0, 1)

retriever = Retriever(CORPUS)
chunks = retriever.retrieve(DESC, domain="finance", k=12)
print(f"retrieved {len(chunks)} chunks (shared)\n", flush=True)

rows = []
for model in MODELS:
    draft._ollama_available = (lambda t=2.0, _m=model: _m)
    extract._ollama_model   = (lambda t=2.0, _m=model: _m)
    t0 = time.time()
    rb = draft.draft_llm(DESC, chunks, CORPUS, "finance", model)   # LLM-ONLY, no fallback
    json_ok = isinstance(rb, dict) and "meta" in rb
    n_ev = n_adv = 0
    lint_ok = rust_ok = False
    fid = float("nan"); params = {}
    if json_ok:
        n_ev = len(rb.get("event_types", []) or [])
        n_adv = len(rb.get("adversaries", []) or [])
        # ground factual params exactly like the pipeline
        try:
            extracted = extract.extract_params("finance", CORPUS, retriever=retriever, allow_llm=True)
            if extracted:
                grounding.verify(extracted, corpus_dir=CORPUS)
                extract.apply_extractions(rb, extracted)
                params = {e.name: e.value for e in extracted if getattr(e, "grounded", False)}
        except Exception as e:
            print(f"   {model} ground err: {e}")
        lr = vc.lint(rb); lint_ok = lr.ok
        if not lint_ok:
            print(f"   {model} LINT FAIL: {list(lr.errors)[:3]}")
        if lint_ok:
            out = SP / f"rbllm_{model.replace(':','_').replace('.','')}.yaml"
            out.write_text(vc.to_yaml(rb), encoding="utf-8")
            rust = vc.run_rust_validator(out, out)
            rust_ok = rust.ok
            if rust_ok:
                gdir = SP / f"gl_{model.replace(':','_')}"
                shutil.rmtree(gdir, ignore_errors=True)
                subprocess.run([BIN, "generate", "--domain", str(out), "--nodes", str(n),
                                "--edges", str(m), "--time-span", str(span_days), "--seed", "42",
                                "--out", str(gdir), "--quiet"], stderr=subprocess.DEVNULL)
                try:
                    ss, sd, st = load_saga(str(gdir))
                    fid = round(compare(rstat, compute_stats(ss, sd, st))["fidelity_score"], 3)
                except Exception as e:
                    print(f"   {model} gen/fid err: {e}")
                shutil.rmtree(gdir, ignore_errors=True)
            else:
                print(f"   {model} RUST REJECTED: {rust.stderr.strip()[:160]}")
    dt = time.time() - t0
    rows.append(dict(model=model, size_B=SIZES.get(model), json_ok=json_ok, n_events=n_ev,
                     n_adversaries=n_adv, lint_ok=lint_ok, rust_valid=rust_ok, fidelity=fid,
                     params=params, secs=round(dt)))
    print(f"[{model}] {SIZES.get(model)}B  json={json_ok} ev={n_ev} adv={n_adv} "
          f"lint={lint_ok} RUST_VALID={rust_ok} fid={fid}  ({dt:.0f}s)", flush=True)
    json.dump(rows, open(SP/"llm_ablation2.json","w"), indent=2, default=str)

print("\n=== LLM-DRAFT ABLATION (LLM-only, no fallback) ===")
print(f"{'model':14s}{'B':>5}{'json':>6}{'ev':>4}{'adv':>4}{'rustValid':>10}{'fid':>7}")
for r in rows:
    print(f"{r['model']:14s}{str(r['size_B']):>5}{str(r['json_ok']):>6}{r['n_events']:>4}"
          f"{r['n_adversaries']:>4}{str(r['rust_valid']):>10}{str(r['fidelity']):>7}")
print("saved llm_ablation2.json")
