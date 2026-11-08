"""Run ETN-gen on a real temporal graph and emit a generated temporal edge list.
Follows USAGE_ETN-GEN.ipynb exactly (load_data -> build_graphs -> count_ETN ->
get_dict -> generate_graph_g2 loop). Reports wall time and peak RSS."""
import sys, time, resource, gzip, random
sys.path.insert(0, "/tmp/baselines/ETNgen")
import numpy as np, construction as cs
from ETN import count_ETN
from ETNgen import generate_graph_g2, get_dict

src_path, out_path, gap, nsnap = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
t0 = time.time()
data_in = cs.load_data(src_path)
graphs = cs.build_graphs(data_in, gap=gap)
if nsnap and len(graphs) > nsnap: graphs = graphs[:nsnap]
print(f"snapshots: {len(graphs)}", flush=True)
k = 3
local_split = max(1, len(graphs)//24); global_split = len(graphs)
nbuck = max(1, global_split//local_split)
daily = [[] for _ in range(nbuck)]
hour, c = 0, 1
for i, g in enumerate(graphs):
    if i < local_split*c: daily[min(hour, nbuck-1)].append(g)
    else:
        c += 1; hour = 0 if hour == nbuck-1 else hour+1
        daily[min(hour, nbuck-1)].append(g)
dizs = []
for i in range(nbuck):
    if not daily[i]: dizs.append(dizs[-1] if dizs else {}); continue
    E = count_ETN(daily[i], k)
    E = {kk: v for kk, v in sorted(E.items(), reverse=True, key=lambda it: it[1])}
    dizs.append(get_dict(E, k))
t_fit = time.time()-t0
print(f"fit (learn ETN dictionaries): {t_fit:.1f}s", flush=True)
tg = graphs[:2]; nodes = list(tg[0].nodes())
hour, c = 0, 1
for i in range(len(graphs)-1):
    if i < local_split*c: pos = hour
    else:
        c += 1; hour = 0 if hour == nbuck-1 else hour+1; pos = hour
    diz = dizs[min(pos, len(dizs)-1)]
    if not diz: continue
    tg.append(generate_graph_g2(nodes, tg[i:i+k], diz, k, alpha=0.5))
    if i % 200 == 0: print(f"  {i}/{len(graphs)}", flush=True)
wall = time.time()-t0
rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss/1024/1024
n_e = 0
with open(out_path, "w") as f:
    f.write("src,dst,t\n")
    for t, g in enumerate(tg):
        for u, v in g.edges():
            f.write(f"{u},{v},{t}\n"); n_e += 1
print(f"DONE wall={wall:.1f}s fit={t_fit:.1f}s peakRSS={rss:.2f}GB snapshots={len(tg)} edges={n_e} nodes={len(nodes)}")
