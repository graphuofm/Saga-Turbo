#!/usr/bin/env python
"""Logical digests of an agora output dir (parquet).
edge digest  = sha256 over the SORTED multiset of (src,dst,t,event_type,label,amount_bits)
node digest  = sha256 over the SORTED multiset of full node rows (all nodes_*.parquet, all columns, floats by bits)
usage: digest.py [--ref REFDIR] DIR...   (with >1 DIR: also the union digests + shard disjointness;
       with --ref: per-shard subset checks against the reference multiset)"""
import sys, glob, os, json, hashlib, struct, time
from collections import Counter
import pyarrow.parquet as pq

def fbits(x):
    if x is None: return 'N'
    if isinstance(x, float): return struct.pack('<d', x).hex()
    return str(x)

def edge_rows(d):
    rows = []
    for f in sorted(glob.glob(os.path.join(d, '**', 'edges_*.parquet'), recursive=True)):
        t = pq.read_table(f, columns=['src','dst','t','event_type','label','amount'])
        cols = [t.column(c).to_pylist() for c in ['src','dst','t','event_type','label','amount']]
        rows.extend(zip(cols[0], cols[1], cols[2], cols[3], cols[4], map(fbits, cols[5])))
    return rows

def node_rows(d):
    rows = []
    for f in sorted(glob.glob(os.path.join(d, '**', 'nodes_*.parquet'), recursive=True)):
        t = pq.read_table(f)
        name = os.path.basename(f)
        cols = [t.column(c).to_pylist() for c in t.column_names]
        for r in zip(*cols):
            rows.append((name,) + tuple(fbits(v) for v in r))
    return rows

def node_ids(d):
    ids = []
    for f in sorted(glob.glob(os.path.join(d, '**', 'nodes_*.parquet'), recursive=True)):
        ids.extend(pq.read_table(f, columns=['id']).column('id').to_pylist())
    return ids

def h(rows):
    m = hashlib.sha256()
    for r in sorted(rows):
        m.update(repr(r).encode()); m.update(b'\n')
    return m.hexdigest()

args = sys.argv[1:]
ref = None
if args and args[0] == '--ref':
    ref = args[1]; args = args[2:]
t0 = time.time()
res = {'dirs': {}}
ue, un, allids = [], [], []
for d in args:
    e = edge_rows(d); n = node_rows(d); ids = node_ids(d)
    res['dirs'][d] = {'edges': len(e), 'edge_digest': h(e), 'nodes': len(n), 'node_digest': h(n),
                      'node_ids_unique': len(set(ids)) == len(ids)}
    ue.extend(e); un.extend(n); allids.append(set(ids))
if len(args) > 1:
    res['union'] = {'edges': len(ue), 'edge_digest': h(ue), 'nodes': len(un), 'node_digest': h(un)}
    disjoint = all(not (allids[i] & allids[j]) for i in range(len(allids)) for j in range(i+1, len(allids)))
    res['union']['shard_node_ids_disjoint'] = disjoint
if ref:
    re_, rn = edge_rows(ref), node_rows(ref)
    res['ref'] = {'dir': ref, 'edges': len(re_), 'edge_digest': h(re_), 'nodes': len(rn), 'node_digest': h(rn)}
    rnc = Counter(rn)
    for d in args:
        c = Counter(node_rows(d))
        missing = sum((c - rnc).values())
        res['dirs'][d]['node_rows_not_in_ref'] = missing
        res['dirs'][d]['node_slice_equals_ref'] = (missing == 0)
    rec = Counter(re_)
    uec = Counter(ue)
    res['edge_union_equals_ref'] = (uec == rec)
    res['edge_union_minus_ref'] = sum((uec - rec).values())
    res['ref_minus_edge_union'] = sum((rec - uec).values())
    res['node_union_equals_ref'] = (Counter(un) == rnc)
res['digest_seconds'] = round(time.time() - t0, 1)
print(json.dumps(res, indent=1))
