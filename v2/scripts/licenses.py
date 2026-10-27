#!/usr/bin/env python3
"""Copy authentic exact dependency grants; identify missing texts without inventing them."""
import hashlib,json,os,pathlib,subprocess
V2=pathlib.Path(__file__).resolve().parents[1];QA=V2/'.qa';assert pathlib.Path(os.environ['CARGO_HOME']).is_relative_to(QA)
metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--offline','--format-version','1'],cwd=V2));out=V2/'licenses';out.mkdir(exist_ok=True);inventory=[]
for package in metadata['packages']:
 if package['name']=='tezbar-file-info':continue
 root=pathlib.Path(package['manifest_path']).parent;assert root.is_relative_to(QA)
 dest=out/(package['name']+'-'+package['version']);dest.mkdir(exist_ok=True);grants=[]
 for file in sorted(root.iterdir()):
  if file.is_file() and any(word in file.name.upper()for word in ['LICENSE','COPYING','NOTICE','AUTHORS','COPYRIGHT']):
   data=file.read_bytes();(dest/file.name).write_bytes(data);grants.append({'file':file.name,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)})
 inventory.append({'name':package['name'],'version':package['version'],'source':package['source'],'declaredLicense':package['license'],'grants':grants,'missingGrantText':not bool(grants)})
(V2/'fixtures/dependency-licenses.json').write_text(json.dumps(inventory,indent=2)+'\n')
print(json.dumps({'packages':len(inventory),'grantFiles':sum(len(x['grants'])for x in inventory),'missingTexts':[x['name']for x in inventory if x['missingGrantText']],'rEfi':[x for x in inventory if x['name']=='r-efi']},indent=2))
