import os, sys, glob

MAIN="."
AGORA="$ARTIFACT"

def rename(s):
    return s.replace("saga","agora").replace("SAGA","AGORA")

# collect in-scope files in main tree
patterns = [
    "crates/*/src/**/*.rs",
    "crates/*/tests/**/*.rs",
    "crates/*/Cargo.toml",
    "crates/saga-rules/domains/*.yaml",
    "python/saga_eval/**/*",
    "python/saga_rag/**/*",
]
main_files=set()
for p in patterns:
    for f in glob.glob(os.path.join(MAIN,p), recursive=True):
        if os.path.isfile(f):
            main_files.add(os.path.relpath(f, MAIN))

# also collect agora in-scope files to detect deletions (files in agora not in main)
agora_patterns=[p.replace("saga","agora") for p in patterns]
agora_files=set()
for p in agora_patterns:
    for f in glob.glob(os.path.join(AGORA,p), recursive=True):
        if os.path.isfile(f):
            agora_files.add(os.path.relpath(f, AGORA))

def to_agora_rel(rel):
    return rename(rel)

mapped_main = {to_agora_rel(r) for r in main_files}

differ=[]
missing_in_agora=[]  # exist in main, absent in agora -> need create
for rel in sorted(main_files):
    arel = to_agora_rel(rel)
    apath = os.path.join(AGORA, arel)
    mpath = os.path.join(MAIN, rel)
    with open(mpath,'rb') as fh: mcontent=fh.read()
    try:
        mtext=mcontent.decode('utf-8')
        transformed=rename(mtext).encode('utf-8')
    except UnicodeDecodeError:
        transformed=mcontent
    if not os.path.exists(apath):
        missing_in_agora.append((rel,arel))
        continue
    with open(apath,'rb') as fh: acontent=fh.read()
    if transformed!=acontent:
        differ.append((rel,arel))

extra_in_agora=sorted(agora_files - mapped_main)  # in agora but not main -> delete

print("=== FILES DIFFERING (content) ===")
for m,a in differ: print(f"  {m}  ->  {a}")
print(f"total differ: {len(differ)}")
print("\n=== FILES IN MAIN MISSING IN AGORA (create) ===")
for m,a in missing_in_agora: print(f"  {m}  ->  {a}")
print("\n=== FILES IN AGORA NOT IN MAIN (delete) ===")
for a in extra_in_agora: print(f"  {a}")
