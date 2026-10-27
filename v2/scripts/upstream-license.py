#!/usr/bin/env python3
import hashlib,json,os,pathlib,urllib.request
V2=pathlib.Path(__file__).resolve().parents[1];assert pathlib.Path(os.environ['HOME']).is_relative_to(V2/'.qa')
url='https://raw.githubusercontent.com/raycast/extensions/6734d85992290066fd31a7f5bee3cca3d3545a1a/LICENSE'
with urllib.request.urlopen(url,timeout=30) as r:data=r.read(1024*1024)
assert b'Permission is hereby granted' in data
p=V2/'licenses/raycast-extensions-6734d859/LICENSE';p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
legacy=V2/'licenses/file-info-v1-0700b381/LICENSE';legacy.parent.mkdir(parents=True,exist_ok=True);legacy.write_bytes((V2.parent/'LICENSE').read_bytes())
report={'url':url,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'upstreamGrant':'authentic unmodified repository LICENSE','legacyGrant':'exact V1 LICENSE bytes (literal newline escapes preserved)'}
(V2/'fixtures/upstream-license.json').write_text(json.dumps(report,indent=2)+'\n');print('PASS authentic pinned upstream MIT grant, and exact legacy grant retained')
