import numpy as np

path = "./realdata/tgbl-review/tgbl-review_edgelist_v2.csv"
# columns: ts,source,target,weight
data = np.loadtxt(path, delimiter=",", skiprows=1, usecols=(0, 1, 2), dtype=np.int64)
ts, src, dst = data[:, 0], data[:, 1], data[:, 2]

# sort by ts (stable)
order = np.argsort(ts, kind="stable")
ts, src, dst = ts[order], src[order], dst[order]

N_EDGES = 60000
total = ts.size
start = total // 2 - N_EDGES // 2
end = start + N_EDGES
ts_w, src_w, dst_w = ts[start:end], src[start:end], dst[start:end]

# remap node ids to contiguous 0..N-1 over the union of src+dst in the window
nodes = np.unique(np.concatenate([src_w, dst_w]))
remap = {int(v): i for i, v in enumerate(nodes)}
src_r = np.array([remap[int(v)] for v in src_w], dtype=np.int64)
dst_r = np.array([remap[int(v)] for v in dst_w], dtype=np.int64)

n_nodes = nodes.size
span_s = int(ts_w.max() - ts_w.min())
span_days = span_s / 86400.0

print("edges", ts_w.size)
print("n_nodes", n_nodes)
print("span_s", span_s)
print("span_days", span_days)
print("ts_min", int(ts_w.min()), "ts_max", int(ts_w.max()))

out = np.column_stack([src_r, dst_r, ts_w])
np.save("./../scratch_real.npy", out) if False else None
np.save("$WORK/real.npy", out)
