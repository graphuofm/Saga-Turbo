import os
MAIN="."
AGORA="$ARTIFACT"
def rename(s): return s.replace("saga","agora").replace("SAGA","AGORA")

files = [
 "crates/saga-cli/src/commands/doctor.rs",
 "crates/saga-core/src/anomaly.rs",
 "crates/saga-core/src/sim.rs",
 "crates/saga-core/src/world.rs",
 "crates/saga-core/tests/anomaly.rs",
 "crates/saga-host/Cargo.toml",
 "crates/saga-host/src/cost.rs",
 "crates/saga-host/src/probe.rs",
 "crates/saga-introspect/src/cm.rs",
 "crates/saga-rules/domains/crypto.yaml",
 "crates/saga-rules/domains/cyber.yaml",
 "crates/saga-rules/domains/ecommerce.yaml",
 "crates/saga-rules/domains/finance.yaml",
 "crates/saga-rules/domains/healthcare.yaml",
 "crates/saga-rules/domains/transport.yaml",
 "crates/saga-rules/src/domains.rs",
 "crates/saga-rules/src/rulebase.rs",
 "python/saga_eval/deep_gen.py",
]
for rel in files:
    arel = rename(rel)
    with open(os.path.join(MAIN,rel),'r',encoding='utf-8') as fh: t=fh.read()
    out=rename(t)
    apath=os.path.join(AGORA,arel)
    assert os.path.exists(apath), f"target missing: {apath}"
    with open(apath,'w',encoding='utf-8') as fh: fh.write(out)
    print("ported",arel)
print("done", len(files))
