"""Run DYMOND (Zeno, La Fond, Neville, WWW 2021) on a temporal edge list.
Follows the documented workflow: build the igraph + dataset_info, learn
parameters, then generate. Emits src,dst,t and reports wall time and peak RSS."""
import os, sys, time, pickle, resource, gzip, shutil
sys.path.insert(0, "/tmp/baselines/DYMOND")
import igraph

src_file, work, out_csv, nbins = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
shutil.rmtree(work, ignore_errors=True); os.makedirs(work, exist_ok=True)
rows = []
op = gzip.open if src_file.endswith(".gz") else open
with op(src_file, "rt") as f:
    for line in f:
        p = line.replace(",", " ").split()
        if len(p) < 3 or not p[0].lstrip("-").isdigit(): continue
        rows.append((int(p[0]), p[1], p[2]))   # t u v  (ETNgen .dat order)
rows.sort()
tmin, tmax = rows[0][0], rows[-1][0]
width = max(1, (tmax - tmin) // nbins)
nodes = sorted({r[1] for r in rows} | {r[2] for r in rows})
nidx = {v: i for i, v in enumerate(nodes)}
edges, ets = [], []
for t, u, v in rows:
    edges.append((nidx[u], nidx[v])); ets.append(min(nbins - 1, (t - tmin) // width) + 1)
timesteps = sorted(set(ets))
g = igraph.Graph(n=len(nodes), directed=False, edges=edges, edge_attrs={"timestep": ets})
for v in g.vs:
    v["nid"] = f"nid-{v.index}"
    nb = list(set(g.neighbors(v)))
    if nb: v["active"] = min(g.es.select(_between=([v.index], nb))["timestep"])
gname = "ds.pklz"; g.write_picklez(os.path.join(work, gname))
pickle.dump({"gname": gname, "L": 1, "N": g.vcount(), "T": len(timesteps), "timesteps": timesteps},
            open(os.path.join(work, "dataset_info.pkl"), "wb"))
print(f"input: {len(nodes)} nodes, {len(edges)} edge rows, {len(timesteps)} timesteps", flush=True)
t0 = time.time()
from DYMOND.learning.learn_parameters import get_dataset, learn_parameters
dd, di, gg = get_dataset(dataset_dir=work)
learn_parameters(dd, di, gg)
t_fit = time.time() - t0
print(f"fit: {t_fit:.1f}s", flush=True)
from DYMOND.sampling.generate_dynamic_graph import dymond_generate
dymond_generate(work, len(timesteps))
wall = time.time() - t0
rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024 / 1024
import glob
cand = glob.glob(os.path.join(work, "**", "generated_graph*.pklz"), recursive=True)
n_e = 0
if cand:
    gen = igraph.Graph.Read_Picklez(cand[0])
    with open(out_csv, "w") as o:
        o.write("src,dst,t\n")
        for e in gen.es:
            o.write(f"{e.source},{e.target},{e['timestep']}\n"); n_e += 1
print(f"DONE wall={wall:.1f}s fit={t_fit:.1f}s peakRSS={rss:.2f}GB edges={n_e} out={cand[0] if cand else 'NONE'}")
