import json, os, re, shutil, subprocess, time
BIN="./target/release/saga"
S="./scratch"
OUT=os.path.join(S,"g_after")
DOMAINS=["finance","crypto","cyber","transport","ecommerce","healthcare"]
PIS=[0.005,0.01,0.02,0.05,0.10]
res=[]
for dom in DOMAINS:
    for pi in PIS:
        shutil.rmtree(OUT,ignore_errors=True)
        cmd=[BIN,"generate","--domain",dom,"--edges","1000000","--seed","42",
             "--anomaly-rate",str(pi),"--out",OUT,"--quiet"]
        p=subprocess.run(cmd,capture_output=True,text=True)
        cell={"domain":dom,"pi":pi}
        if p.returncode!=0:
            cell["status"]="FAILED"; cell["reason"]=(p.stderr or p.stdout)[-300:]
        else:
            prom=open(os.path.join(OUT,"metrics.prom")).read()
            tot=int(re.search(r"saga_edges_total (\d+)",prom).group(1))
            anom=int(re.search(r"saga_anomalous_edges_total (\d+)",prom).group(1))
            cell.update(status="ok",total_edges=tot,anom_edges=anom,
                        frac=anom/tot if tot else float("nan"))
        res.append(cell)
        print(json.dumps(cell),flush=True)
        shutil.rmtree(OUT,ignore_errors=True)
json.dump(res,open(os.path.join(S,"gpi_baseline.json"),"w"),indent=1)
