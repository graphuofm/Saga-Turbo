import glob, hashlib, sys, pandas as pd
def load(d):
    fs=sorted(glob.glob(d+'/**/edges_*.parquet', recursive=True))
    assert fs, 'no edge files in '+d
    return pd.concat([pd.read_parquet(f) for f in fs], ignore_index=True)
def canon_md5(df):
    df = df.sort_values(list(df.columns), kind='mergesort').reset_index(drop=True)
    h = hashlib.md5()
    for i in range(0, len(df), 1_000_000):
        h.update(df.iloc[i:i+1_000_000].to_csv(index=False, header=False).encode())
    return h.hexdigest()
single_dir, shard_dirs = sys.argv[1], sys.argv[2:]
s = load(single_dir); print('single edges = %d' % len(s))
parts=[]
for d in shard_dirs:
    p=load(d); print('%s edges = %d' % (d, len(p))); parts.append(p)
u = pd.concat(parts, ignore_index=True); print('union edges  = %d' % len(u))
print('sum of shards == single:', len(u)==len(s))
h1, h2 = canon_md5(s), canon_md5(u)
print('single sorted-edge-multiset md5:', h1)
print('%d-shard union             md5: %s' % (len(shard_dirs), h2))
print('RESULT:', 'MATCH' if h1==h2 else 'MISMATCH')
