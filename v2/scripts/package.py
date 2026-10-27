#!/usr/bin/env python3
"""Deterministic closed development ZIP + owned extraction/production-process QA."""
import hashlib,io,json,os,pathlib,stat,subprocess,uuid,zipfile
V2=pathlib.Path(__file__).resolve().parents[1];QA=V2/'.qa';assert pathlib.Path(os.environ['HOME']).is_relative_to(QA)
def sha(b):return hashlib.sha256(b).hexdigest()
def source_hash():
 h=hashlib.sha256()
 for p in sorted([V2/'Cargo.toml',V2/'Cargo.lock',*V2.joinpath('src').rglob('*.rs')]):h.update(str(p.relative_to(V2)).encode()+b'\0'+hashlib.sha256(p.read_bytes()).digest())
 return h.hexdigest()
fresh=json.loads((V2/'evidence/fresh-build.json').read_text());assert fresh['state']=='passed';assert fresh['productionSourceSha256']==source_hash(),'fresh build is stale'
binary=pathlib.Path(fresh['binary']);assert binary.is_relative_to(QA) and not binary.is_symlink();assert sha(binary.read_bytes())==fresh['binarySha256']
manifest=json.loads((V2/'extension.json').read_text());assert manifest['artifacts']=={};assert all(x['state']=='not-run'for x in manifest['verification'])
inputs={'bin/tezbar-file-info':binary,'resources/extension-icon.png':V2/'resources/extension-icon.png','LICENSE':V2/'LICENSE','NOTICE':V2/'NOTICE','README.md':V2/'PACKAGE-README.md'}
for p in sorted(V2.joinpath('licenses').rglob('*')):
 if p.is_file():inputs[str(p.relative_to(V2))]=p
members={}
for name,p in inputs.items():
 assert not p.is_symlink() and p.is_file();parts=pathlib.PurePosixPath(name);assert not parts.is_absolute() and '..'not in parts.parts and '\\'not in name and ':'not in name
 members[name]=p.read_bytes()
assert sha(members['resources/extension-icon.png'])=='b88c6a3186889071715f11d86a69f29fd234e6fdaac0c28795570039898401e6'
manifest['files']={name:sha(data)for name,data in sorted(members.items())};members['extension.json']=(json.dumps(manifest,indent=2,ensure_ascii=False)+'\n').encode()
assert len(members)<4096 and sum(map(len,members.values()))<64*1024*1024

def archive():
 out=io.BytesIO()
 with zipfile.ZipFile(out,'w',compression=zipfile.ZIP_STORED,allowZip64=False) as z:
  for name,data in sorted(members.items()):
   entry=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));entry.create_system=3;entry.compress_type=zipfile.ZIP_STORED;entry.external_attr=(0o100755 if name.startswith('bin/') else 0o100644)<<16;z.writestr(entry,data)
 return out.getvalue()
first=archive();second=archive();assert first==second,'non-deterministic ZIP serialization'
dist=V2/'dist';dist.mkdir(exist_ok=True);destination=dist/'file-info-aarch64-apple-darwin-dev.zip';destination.write_bytes(first)
extracted=QA/'extracted'/str(uuid.uuid4());extracted.mkdir(parents=True)
with zipfile.ZipFile(io.BytesIO(first)) as z:
 names=z.namelist();assert len(names)==len(set(names)) and set(names)==set(members)
 for entry in z.infolist():
  assert stat.S_ISREG(entry.external_attr>>16);data=z.read(entry);assert data==members[entry.filename]
  target=extracted.joinpath(*pathlib.PurePosixPath(entry.filename).parts);assert target.is_relative_to(extracted);target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data);target.chmod(0o755 if entry.filename.startswith('bin/') else 0o644)
assert set(manifest['files'])==set(members)-{'extension.json'}
for name,digest in manifest['files'].items():assert sha((extracted/name).read_bytes())==digest
subprocess.run(['node','scripts/schema-qa.cjs',str(extracted/'extension.json')],cwd=V2,check=True)
with (V2/'evidence/extracted-process-qa.txt').open('w') as log:
 subprocess.run(['python3','scripts/process-qa.py',str(extracted/'bin/tezbar-file-info')],cwd=V2,stdout=log,stderr=subprocess.STDOUT,check=True)
report={'state':'passed','target':'aarch64-apple-darwin','archive':str(destination),'archiveSha256':sha(first),'archiveBytes':len(first),'deterministicSerializations':2,'closedMembers':len(members),'executable':'bin/tezbar-file-info','executableSha256':manifest['files']['bin/tezbar-file-info'],'sourceSha256':source_hash(),'extractedRoot':str(extracted),'members':[{'path':n,'sha256':sha(b),'bytes':len(b)}for n,b in sorted(members.items())],'extractedProductionQA':'passed','installed':False,'published':False}
(V2/'evidence/archive.json').write_text(json.dumps(report,indent=2)+'\n');print('PASS deterministic closed ARM64 archive and extracted production commands; SHA256',report['archiveSha256'])
