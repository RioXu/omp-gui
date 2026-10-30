"""Extract public UI metadata from the OMP compact catalog (no credentials).

Usage: python scripts/update_service_catalog.py <oh-my-pi checkout>
The installed CLI's available models are merged at runtime.
"""
import json
import sys
from pathlib import Path
from functools import cache

root = Path(sys.argv[1])
data = json.loads((root/'packages/catalog/src/models.json').read_text(encoding='utf-8'))
assert data['version'] == 1

@cache
def node(index):
    n=data['nodes'][index]
    if n[0] == 1:
        return [decode(v) for v in n[1]]
    result={data['keys'][n[1][i]]:decode(n[1][i+1]) for i in range(0,len(n[1]),2)}
    for i,key in enumerate(data['booleanShapes'][n[2]]):
        result[data['keys'][key]]=bool((n[3][i//32] if i//32 < len(n[3]) else 0) >> (i%32) & 1)
    return result

def decode(value):
    return (node(value[0]) if value[0]>=0 else None) if isinstance(value,list) else value

models=[]
keys=['id','provider','name','api','baseUrl','kind','webSearch','contextWindow','maxTokens','reasoning','input','cost']
for provider,constants,rows in data['providers']:
    for row in rows:
        model={'id':row[0],**decode(constants),**decode(row[1]),'provider':provider}
        if model.get('kind','chat') not in ['chat','search']:
            continue
        models.append({k:model[k] for k in keys if k in model})
version=json.loads((root/'packages/coding-agent/package.json').read_text(encoding='utf-8'))['version']
providers=json.loads((root/'packages/catalog/src/compat/rules.json').read_text(encoding='utf-8'))['providers']
out=Path('resources/omp-services.json');out.parent.mkdir(exist_ok=True)
out.write_text(json.dumps({'version':version,'providers':sorted(providers),'models':models},ensure_ascii=False,separators=(',',':')),encoding='utf-8')
print(f'{len(models)} models; {len(providers)} provider entries; {sum(m.get("kind")=="search" for m in models)} search engines')
