#!/usr/bin/env python3
"""Create/verify the full review inventory; --staged is for the parent after review.
This script never stages, commits, pushes, deletes or edits another checkout.
"""
import hashlib,json,pathlib,subprocess,sys
V2=pathlib.Path(__file__).resolve().parents[1];ROOT=V2.parent;BASE='0700b381975cccdfdadc2cd91c83171782875fa7';SELF='v2/REVIEW-INVENTORY.json'
def git(*args):return subprocess.check_output(['git','-C',str(ROOT),*args])
def sha(b):return hashlib.sha256(b).hexdigest()
assert git('rev-parse','--show-toplevel').decode().strip()==str(ROOT)
assert git('rev-parse','HEAD').decode().strip()==BASE
assert git('branch','--show-current').decode().strip()=='feat/native-v2-rust'
original=[]
for p in git('ls-tree','-r','--name-only','-z',BASE).decode().split('\0'):
 if not p:continue
 b=git('show',BASE+':'+p);original.append({'path':p,'sha256':sha(b),'gitBlob':git('rev-parse',BASE+':'+p).decode().strip()})
 if p!='README.md':assert (ROOT/p).read_bytes()==b,'V1 bytes changed: '+p
 else:
  new=(ROOT/p).read_bytes();assert new==b+b'\nStandalone Rust v2: [implementation and verification](v2/README.md).\n','unexpected root README change'
(V2/'fixtures/v1-tracked-files.json').write_text(json.dumps(original,indent=2)+'\n') if '--staged'not in sys.argv else None
visible=set(p for p in git('ls-files','--cached','--others','--exclude-standard','-z').decode().split('\0') if p=='README.md'or p.startswith('v2/'));visible.discard(SELF)
for p in visible:
 parts=pathlib.PurePosixPath(p).parts;assert not any(x in {'.qa','target','dist','node_modules','__pycache__'}for x in parts) and not p.endswith(('.zip','.pyc','.pyo')),'forbidden Git-visible artifact: '+p
if '--staged'in sys.argv:
 inventory=json.loads(git('show',':'+SELF));entries=inventory['files'];assert set(x['path']for x in entries)==visible
 for record in entries:assert sha(git('show',':'+record['path']))==record['sha256'],'staged mismatch: '+record['path']
 print('PASS parent staged-byte rehash:',len(entries),'files; inventory itself excluded by design');sys.exit(0)
if '--verify'in sys.argv:
 inventory=json.loads((ROOT/SELF).read_text());entries=inventory['files'];assert set(x['path']for x in entries)==visible
 for record in entries:
  p=ROOT/record['path'];assert not p.is_symlink() and sha(p.read_bytes())==record['sha256'],'review mismatch: '+record['path']
 print('PASS full review inventory:',len(entries),'files; SHA256',sha((ROOT/SELF).read_bytes()));sys.exit(0)
assert git('diff','--cached','--name-only')==b'','implementation index must remain empty'
entries=[]
for name in sorted(visible):
 p=ROOT/name;assert not p.is_symlink() and p.is_file();entries.append({'path':name,'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size,'executable':bool(p.stat().st_mode&0o111)})
report={'schemaVersion':1,'root':str(ROOT),'branch':'feat/native-v2-rust','sourceHead':BASE,'inventoryExcludesOnlyItself':SELF,'indexEmptyAtFinalization':True,'files':entries,'fileCount':len(entries),'originalTrackedFiles':len(original),'v1Preservation':'all original tracked bytes except exact short README link','parentBeforeCommit':'stage reviewed bytes, then run scripts/review.py --staged; this implementation never stages'}
(ROOT/SELF).write_text(json.dumps(report,indent=2)+'\n');print('PASS inventory created:',len(entries),'files; SHA256',sha((ROOT/SELF).read_bytes()))
