import sys, csv, json, collections, numpy as np
sys.path.insert(0,'.'); sys.path.insert(0,'./python')
import anom

def load_cic(path):
    s_raw,d_raw,lab_raw=[],[],[]
    with open(path) as fh:
        r=csv.reader(fh); next(r)
        for row in r:
            s_raw.append(row[0]); d_raw.append(row[2]); lab_raw.append(row[87])
    ids={v:i for i,v in enumerate(sorted(set(s_raw)|set(d_raw)))}
    n=len(ids)
    src=np.array([ids[x] for x in s_raw],dtype=np.int64)
    dst=np.array([ids[x] for x in d_raw],dtype=np.int64)
    t=np.arange(len(s_raw),dtype=np.float64)      # row order = temporal index (SCHEMA)
    atk=np.array([x!='BENIGN' for x in lab_raw])
    lab=np.zeros(n,dtype=np.int8)
    for k in np.unique(src[atk]).tolist(): lab[k]=1   # attacker = src of any attack flow
    return src,dst,t,lab,atk,n,collections.Counter(lab_raw)

src,dst,t,lab,atk,n,cnt=load_cic('./realdata/cicids2017/friday.csv')
res=anom.analyse('cicids2017-friday',src,dst,t,lab,n)
res['basics']={'n_hosts':n,'n_flows':int(src.size),'n_attacker_hosts':int((lab==1).sum()),
 'attacker_host_rate':float((lab==1).sum()/n),'n_attack_flows':int(atk.sum()),
 'attack_flow_rate':float(atk.mean()),'label_counts':dict(cnt)}
json.dump(res,open('cicids_anom.json','w'),indent=2,default=float)
print(json.dumps({k:res[k] for k in ['basics','contrast']},indent=1,default=float))
print('ANOM',json.dumps({k:v for k,v in res['anomalous'].items()},indent=1,default=float))
print('NORM',json.dumps({k:v for k,v in res['normal'].items()},indent=1,default=float))
