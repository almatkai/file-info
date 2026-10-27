#!/usr/bin/env python3
"""Real production SDK pipes; all host services are owned fakes, all files generated."""
import hashlib,json,os,plistlib,pathlib,select,shutil,subprocess,sys,time,uuid
V2=pathlib.Path(__file__).resolve().parents[1];QA=V2/'.qa';assert pathlib.Path(os.environ['HOME']).is_relative_to(QA)
BINARY=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else QA/'target/debug/tezbar-file-info'
RUN=QA/'runs'/str(uuid.uuid4());RUN.mkdir(parents=True);RESULTS=[]
CAPS=['selection.read','clipboard.write','shell.reveal','shell.openWithApp','apps.list','dialog.confirm']
def owned(p):
 p=pathlib.Path(p);assert p.is_absolute() and p.is_relative_to(QA),f'not QA-owned: {p}';return p
def setxattr(p,name,value):
 subprocess.check_call(['/usr/bin/xattr','-wx',name,value.hex(),str(owned(p))])
def listxattr(p):
 return subprocess.check_output(['/usr/bin/xattr',str(owned(p))]).decode().splitlines()
def getxattr(p,name):
 return subprocess.check_output(['/usr/bin/xattr','-p',name,str(owned(p))]).rstrip(b'\n')
class Host:
 def __init__(self,tool_dir=None):
  self.buffer=b'';self.paths=[];self.calls=[];self.seq=0;self.ack={'ok':True};self.confirm=1;self.pending={}
  env=dict(os.environ,PATH=str(tool_dir or QA/'bin'),TZ='UTC')
  self.proc=subprocess.Popen([str(BINARY)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=RUN,env=env)
  self.hello=self.read();assert self.hello['id']=='com.tezbar.native.file-info';assert self.hello['sdkVersion']=='0.1.1'
  self.request('initialize',{'protocol':1,'extensionId':self.hello['id'],'platform':'macOS','packageDir':str(BINARY.parent.parent if BINARY.parent.name=='bin' else V2),'dataDir':str(RUN/'data'),'tempDir':str(RUN/'temp'),'grantedCapabilities':CAPS,'migrationPaths':{},'reason':'command'})
 def read(self):
  # Select the descriptor only when our own bounded buffer has no complete line.
  # BufferedReader.readline may prefetch a response behind an SDK cancel event;
  # selecting that empty OS pipe again would invent a 15-second protocol timeout.
  deadline=time.monotonic()+15
  while b'\n'not in self.buffer:
   assert len(self.buffer)<=16*1024*1024,'protocol frame bound'
   ready=select.select([self.proc.stdout],[],[],max(0,deadline-time.monotonic()))[0]
   if not ready:raise AssertionError('protocol deadline')
   chunk=os.read(self.proc.stdout.fileno(),65536)
   if not chunk:raise AssertionError('protocol EOF: '+self.proc.stderr.read().decode())
   self.buffer+=chunk
  line,self.buffer=self.buffer.split(b'\n',1);assert len(line)<=16*1024*1024
  return json.loads(line)
 def send(self,obj):self.proc.stdin.write(json.dumps(obj,ensure_ascii=False).encode()+b'\n');self.proc.stdin.flush()
 def start(self,method,params):
  self.seq+=1;i=f'h:{uuid.uuid4()}';self.send({'type':'request','id':i,'method':method,'params':params});return i
 def reply(self,call,result):self.send({'type':'response','id':call['id'],'result':result})
 def service(self,m):
  method=m['method'];self.calls.append((method,m['params']))
  if method=='host.selection.read':
   assert m['params']=={'kind':'files'}
   for p in self.paths:
    if isinstance(p,str) and pathlib.Path(p).is_absolute():owned(p)
   value=self.paths
  elif method=='host.dialog.confirm':value={'response':self.confirm}
  elif method=='host.apps.list':value=[{'name':'Owned App','path':str(RUN/'Owned App.app')}]
  elif method in ['host.clipboard.write','host.shell.reveal','host.shell.openWithApp']:value=self.ack
  else:raise AssertionError('unexpected live service '+method)
  self.reply(m,value)
 def await_response(self,i):
  if i in self.pending:return self.pending.pop(i)
  while True:
   m=self.read()
   if m['type']=='request':self.service(m)
   elif m['type']=='response':
    if m['id']==i:return m
    self.pending[m['id']]=m
   elif m['type'] in ['cancel','event']:pass
   else:raise AssertionError(m)
 def request(self,method,params):
  m=self.await_response(self.start(method,params))
  if 'error'in m:raise AssertionError(m['error'])
  return m['result']
 def open(self,s,command='show-file-info'):return self.request('session.open',{'sessionId':s,'commandName':command,'preferences':{},'argumentValues':{}})
 def action(self,s,a):return self.request('session.call',{'sessionId':s,'operation':'extension:invoke-action','payload':{'actionId':a}})
 def expired(self,s,a):
  r=self.await_response(self.start('session.call',{'sessionId':s,'operation':'extension:invoke-action','payload':{'actionId':a}}));assert 'error'in r,r;return r
 def refresh(self,s):return self.request('session.call',{'sessionId':s,'operation':'extension:refresh-session','payload':{}})
 def dispose(self,s):self.request('session.dispose',{'sessionId':s})
 def close(self):
  self.request('shutdown',{});self.proc.stdin.close();self.proc.wait(timeout=5);assert self.proc.returncode==0

def check(name,fn):
 start=time.monotonic();detail=fn();RESULTS.append({'name':name,'state':'passed','seconds':round(time.monotonic()-start,3),'detail':detail});print('PASS',name,flush=True)
def copy(ext):
 p=RUN/f'{uuid.uuid4()}.{ext}';shutil.copyfile(QA/'fixtures'/f'own.{ext}',p);return p

def views_actions():
 h=Host();p=RUN/'unicode\nфайл ; $().txt';shutil.copyfile(QA/'fixtures'/'plain.txt',p);h.paths=[str(p)]
 v=h.open('view');assert v['mode']=='view' and v['refreshMs'] is None
 fields=v['root']['children'][0]['children'][0]['children'][0]['children'][0]['children'];labels=[x['props'].get('title')for x in fields if x['type'].endswith('Label')]
 assert labels[:5]==['General Information','Name','Type','Size','Extension'];assert labels[-3:]==['Mode','Owner','Group']
 assert v['root']['children'][1]['props']['title']=='Quick Stats'
 original=v['actions'][0]['id'];v=h.action('view',original);assert h.calls[-1]==('host.clipboard.write',{'text':str(p)});assert v['actions'][0]['id']!=original
 count=len(h.calls);h.expired('view',original);assert len(h.calls)==count
 h.paths=[str(RUN/'not-selected-now.txt')]
 for invalid in [None,True,False,{}, {'ok':True,'error':'no'}, {'ok':False}, {'ok':True,'message':None}, {'ok':True,'extra':1}]:
  h.ack=invalid;v=h.action('view',v['actions'][0]['id']);
  if invalid is None:(QA/'production-unknown-view.json').write_text(json.dumps(v))
  assert 'unknown' in v['message'].lower();assert v['actions'][0]['title'].startswith('Retry ')
  assert h.calls[-1]==('host.clipboard.write',{'text':str(p)})
 h.ack={'ok':True,'message':'Copied'};v=h.action('view',v['actions'][0]['id']);assert v['actions'][0]['title']=='Copy File Path'
 reveal=v['actions'][1]['id'];v=h.action('view',reveal);assert h.calls[-1]==('host.shell.reveal',{'path':str(p)})
 v=h.action('view',v['actions'][2]['id']);assert v['root']['props']['navigationTitle']=='Open With'
 app_id=v['root']['children'][0]['props']['actionIds'][0];h.ack=None;v=h.action('view',app_id);assert h.calls[-1][0]=='host.shell.openWithApp';assert 'backActionId'not in v['root']['props'];assert v['root']['children'][0]['props']['title'].startswith('Retry ');assert v['effects'][0]['style']=='failure';h.ack={'ok':True};v=h.action('view',v['actions'][0]['id'])
 back=v['root']['props']['backActionId'];v=h.action('view',back);all_id=v['actions'][3]['id'];h.action('view',all_id);assert 'Name: unicode\nфайл' in h.calls[-1][1]['text']
 # New session and stale/cross-session ids cannot issue effects.
 h.paths=[str(p)];other=h.open('other');count=len(h.calls);h.expired('other',all_id);assert len(h.calls)==count
 for q in ['A','B','A','\u0000', '']:
  assert h.request('session.call',{'sessionId':'view','operation':'extension:search-text-changed','payload':{'searchText':q}})['mode']=='unchanged'
 fresh=h.refresh('view');assert 'searchText' not in fresh['root']['props'];(QA/'production-view.json').write_text(json.dumps(fresh))
 h.dispose('view');h.dispose('other');h.close();return {'labels':labels,'hostCalls':len(h.calls),'strictAckNegatives':8,'selectionReads':sum(m=='host.selection.read'for m,_ in h.calls)}
check('view-fields-copy-reveal-openwith-strict-acks-stale-input',views_actions)

def cancel_effect():
 h=Host();p=copy('png');h.paths=[str(p)];v=h.open('effect');a=v['actions'][0]['id']
 request=h.start('session.call',{'sessionId':'effect','operation':'extension:invoke-action','payload':{'actionId':a}});call=h.read();assert call['method']=='host.clipboard.write'
 # An overlapping queued old id is rejected or inert, never a second effect.
 queued=h.start('session.call',{'sessionId':'effect','operation':'extension:invoke-action','payload':{'actionId':a}})
 h.send({'type':'cancel','requestId':request,'reason':'QA cancellation'})
 response=h.await_response(request);assert 'error'in response
 h.reply(call,{'ok':True});h.await_response(queued)
 before=len(h.calls);v=h.action('effect',a);assert len(h.calls)==before;assert v['actions'][0]['title'].startswith('Retry ');assert 'unknown' in v['message'].lower()
 v=h.action('effect',v['actions'][0]['id']);assert h.calls[-1]==('host.clipboard.write',{'text':str(p)});assert v['actions'][0]['title']=='Copy File Path'
 h.dispose('effect');h.close();return 'cancelled effect retained frozen payload; late ACK inert; explicit retry only'
check('real-sdk-effect-cancel-and-frozen-retry',cancel_effect)

def canceled_opens():
 h=Host()
 for n in range(140):
  i=h.start('session.open',{'sessionId':f'cancel-{n}','commandName':'show-file-info','preferences':{},'argumentValues':{}});call=h.read();assert call['method']=='host.selection.read';h.send({'type':'cancel','requestId':i,'reason':'owned QA'});assert 'error'in h.await_response(i)
  h.reply(call,[])
 count=len(h.calls);r=h.await_response(h.start('session.open',{'sessionId':'cancel-0','commandName':'show-file-info','preferences':{},'argumentValues':{}}));assert 'error'in r;assert len(h.calls)==count
 h.paths=[str(copy('png'))];assert h.open('survivor')['mode']=='view';h.dispose('survivor');h.close();return {'canceledOpens':140,'newSession':'accepted'}
check('real-sdk-140-canceled-opens-without-dispose',canceled_opens)

def decoded(p):
 out=subprocess.run(['/opt/homebrew/bin/ffmpeg','-v','error','-i',str(owned(p)),'-map','0','-f','framemd5','-'],capture_output=True,timeout=30)
 if out.returncode:raise AssertionError(out.stderr.decode())
 return [line for line in out.stdout.decode().splitlines() if not line.startswith('#')]
def exif(p):
 out=subprocess.check_output(['/usr/bin/perl',str(QA/'tools/exiftool-13.40/exiftool'),'-config','','-j',str(owned(p))],timeout=30)
 return json.loads(out)[0]
def formats():
 h=Host();details=[]
 for ext in ['jpg','png','gif','webp','tiff','wav','flac','mp3','mp4','mov','m4a','pdf']:
  p=copy(ext);before=p.read_bytes();decode=decoded(p) if ext!='pdf' else hashlib.sha256(subprocess.check_output(['/opt/homebrew/bin/pdftoppm','-f','1','-singlefile','-scale-to','100',str(owned(p))],timeout=30)).hexdigest()
  setxattr(p,'com.tezbar.file-info.qa',b'owned QA');
  if ext=='png':
   setxattr(p,'com.apple.ResourceFork',b'owned QA fork');setxattr(p,'com.apple.quarantine',b'0001;0;TezbarQA;');setxattr(p,'com.apple.metadata:kMDItemFinderComment',plistlib.dumps('owned QA comment',fmt=plistlib.FMT_BINARY))
  birth=p.stat().st_birthtime;mode=p.stat().st_mode&0o777
  h.paths=[str(p)];v=h.open('clear-'+ext,'clear-file-metadata');print(ext,v['message'].splitlines()[0],flush=True)
  if ext in ['wav','flac','mp3']:
   assert v['results'][0]['state']=='failed',v;assert p.read_bytes()==before;assert getxattr(p,'com.tezbar.file-info.qa')==b'owned QA';h.dispose('clear-'+ext);details.append({'format':ext,'state':'explicit native-tool read-only failure; original retained'});continue
  assert v['results'][0]['state'] in ['committed','partial'],v
  assert p.stat().st_mode&0o777==mode;assert abs(p.stat().st_birthtime-birth)<1e-3
  assert not [a for a in listxattr(p) if a!='com.apple.provenance'];assert (v['results'][0]['state']=='partial')==bool(listxattr(p))
  if ext!='pdf':assert decoded(p)==decode,ext
  else:
   assert hashlib.sha256(subprocess.check_output(['/opt/homebrew/bin/pdftoppm','-f','1','-singlefile','-scale-to','100',str(owned(p))],timeout=30)).hexdigest()==decode
   base=before.split(b'%BeginExifToolUpdate',1)[0];assert p.read_bytes().startswith(base)
  tags=exif(p);assert tags.get('Creator') is None, (ext,tags)
  assert tags.get('Artist') is None,(ext,tags)
  details.append({'format':ext,'bytesBefore':len(before),'bytesAfter':p.stat().st_size,'decodedPayloadParity':True,'xattrsAfter':len(listxattr(p)),'sha256After':hashlib.sha256(p.read_bytes()).hexdigest()});h.dispose('clear-'+ext)
 h.close();return details
check('authentic-exiftool-twelve-containers-payload-xattrs-atomicity',formats)

def failures():
 h=Host();ok=copy('png');bad=RUN/'unsupported.txt';bad.write_text('QA private text');setxattr(bad,'com.tezbar.file-info.qa',b'keep on failure')
 readonly=copy('png');readonly.chmod(0o444);hard=copy('png');alias=RUN/'hard-alias.png';os.link(hard,alias);link=RUN/'symlink.png';link.symlink_to(ok)
 broken=RUN/'bad.png';shutil.copyfile(QA/'fixtures'/'bad.png',broken)
 originals={p:p.read_bytes() for p in [bad,readonly,hard,broken]}
 h.paths=[str(x)for x in [ok,bad,readonly,hard,link,broken,RUN/'absent.png',RUN]];v=h.open('partial','clear-file-metadata');assert 'Failed 7' in v['message'] and 'Partially completed' in v['message'],v
 assert v['results'][0]['state']in ['committed','partial'];assert [r['state']for r in v['results'][1:]]==['failed']*7
 for p,data in originals.items():assert p.read_bytes()==data
 assert getxattr(bad,'com.tezbar.file-info.qa')==b'keep on failure'
 h.dispose('partial');h.paths=[];assert h.open('empty','clear-file-metadata')['effects'][0]['style']=='failure';h.dispose('empty')
 h.confirm=0;h.paths=[str(bad)];before=bad.read_bytes();assert h.open('deny','clear-file-metadata')['message'].startswith('Cancelled');assert bad.read_bytes()==before;h.dispose('deny')
 h.close();return '1 committed + 7 failed; rejected files retain exact bytes/attrs; confirmation cancellation has zero changes'
check('batch-partials-malformed-unsupported-hardlink-symlink-readonly',failures)

def identity_race():
 h=Host();p=copy('png');h.paths=[str(p)];i=h.start('session.open',{'sessionId':'identity','commandName':'clear-file-metadata','preferences':{},'argumentValues':{}})
 while True:
  call=h.read()
  if call['method']=='host.dialog.confirm':break
  h.service(call)
 # Rename selected original away and place an unrelated generated file at name.
 old=p.with_suffix('.old');p.rename(old);p.write_bytes(b'Unrelated owned QA replacement')
 h.reply(call,{'response':1});v=h.await_response(i)['result'];assert v['results'][0]['state']=='failed';assert p.read_bytes()==b'Unrelated owned QA replacement';assert old.read_bytes().startswith(b'\x89PNG')
 h.dispose('identity');h.close();return 'frozen pre-confirmation identity rejects replacement; both QA files retained'
check('frozen-plan-identity-race',identity_race)


def actual_native_inspection():
 native=RUN/'native-inspection-bin';native.mkdir()
 for name in ['ls','file']:
  shutil.copyfile(QA/'bin'/name,native/name);(native/name).chmod(0o700)
 probe=native/'ffprobe';probe.write_text('#!/bin/sh\nexec /opt/homebrew/bin/ffprobe "$@"\n');probe.chmod(0o700)
 h=Host(native);records=[]
 for ext in ['png','mp4','m4a']:
  p=copy(ext);h.paths=[str(p)];v=h.open('native-'+ext);metadata=v['root']['children'][0]['children'][0]['children'][0]['children'][0]['children'];fields={n['props'].get('title'):n['props'].get('text')for n in metadata}
  if ext!='m4a':assert fields['Dimensions']=='2 × 2',fields
  if ext in ['mp4','m4a']:assert 'Duration'in fields,fields
  records.append({'format':ext,'dimensions':fields.get('Dimensions'),'duration':fields.get('Duration')});h.dispose('native-'+ext)
 disguised=RUN/'owned-playlist.mp4';disguised.write_text('#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1\n'+str(RUN/'unselected.wav')+'\n');h.paths=[str(disguised)];v=h.open('disguised');metadata=v['root']['children'][0]['children'][0]['children'][0]['children'][0]['children'];assert not any(n['props'].get('title')in ['Dimensions','Duration']for n in metadata);h.dispose('disguised');h.close();return records
check('authentic-native-ffprobe-inspection-and-playlist-denial',actual_native_inspection)

def capacity_and_invalid():
 h=Host();p=copy('png');h.paths=[str(p)]
 for n in range(128):assert h.open(f'capacity-{n}')['mode']=='view'
 over=h.await_response(h.start('session.open',{'sessionId':'over','commandName':'show-file-info','preferences':{},'argumentValues':{}}));assert 'error'in over
 for n in range(128):h.dispose(f'capacity-{n}')
 assert h.open('capacity-recovered')['mode']=='view';h.dispose('capacity-recovered')
 for n,bad in enumerate([[{'path':str(p)}],['relative'],[str(p)]*129,[str(p),str(p)]]):
  h.paths=bad;i=h.start('session.open',{'sessionId':f'invalid-{n}','commandName':'clear-file-metadata','preferences':{},'argumentValues':{}});r=h.await_response(i);assert 'error'in r
 h.close();return {'capacity':128,'overflow':'rejected','afterDisposal':'recovered','badSelectionCases':4}
check('real-sdk-capacity-and-selection-contract',capacity_and_invalid)

def partial_cancel():
 slow=RUN/'slow-bin';slow.mkdir();marker=RUN/'helper-entered';exif=slow/'exiftool'
 exif.write_text('#!/usr/bin/python3\nimport os,sys,time,pathlib\nassert pathlib.Path.cwd().is_relative_to(pathlib.Path('+repr(str(QA))+'))\nif sys.argv[-1].endswith(".jpg"):\n pathlib.Path('+repr(str(marker))+').write_text("owned helper entered")\n time.sleep(10)\nos.execv("/usr/bin/perl",["/usr/bin/perl",'+repr(str(QA/'tools/exiftool-13.40/exiftool'))+',*sys.argv[1:]])\n');exif.chmod(0o700)
 h=Host(slow);first=copy('png');second=copy('jpg');setxattr(first,'com.tezbar.file-info.qa',b'owned QA');before=second.read_bytes();h.paths=[str(first),str(second)]
 i=h.start('session.open',{'sessionId':'partial-cancel','commandName':'clear-file-metadata','preferences':{},'argumentValues':{}})
 while True:
  m=h.read();h.service(m)
  if m['method']=='host.dialog.confirm':break
 deadline=time.monotonic()+5
 while not marker.exists():
  assert time.monotonic()<deadline,'helper did not enter second stage';time.sleep(.01)
 assert 'com.tezbar.file-info.qa' not in listxattr(first)
 h.send({'type':'cancel','requestId':i,'reason':'after first file committed'})
 assert 'error'in h.await_response(i);assert second.read_bytes()==before
 count=len(h.calls);r=h.await_response(h.start('session.open',{'sessionId':'partial-cancel','commandName':'clear-file-metadata','preferences':{},'argumentValues':{}}));assert 'error'in r;assert len(h.calls)==count;assert 'Frozen outcomes' in r['error']['message'];assert 'partial' in r['error']['message'] and 'unknown' in r['error']['message']
 h.close();return 'first committed, second unchanged; cancelled opening cannot re-read/replay same identity; no automatic retry'
check('partial-batch-cancel-with-immediate-checkpoint',partial_cancel)

def directory_views():
 h=Host();records=[]
 for nonempty in [False,True]:
  d=RUN/('nonempty.png' if nonempty else 'empty');d.mkdir();child=d/'unselected.xmp'
  if nonempty:child.write_bytes((QA/'round2-fixtures/own.xmp').read_bytes());setxattr(child,'com.tezbar.file-info.qa',b'untouched child')
  snapshot=child.read_bytes() if nonempty else None;stat=d.stat();h.paths=[str(d),str(copy('png'))]
  v=h.open('dir-'+str(nonempty));fields={n['props'].get('title'):n['props'].get('text')for n in v['root']['children'][0]['children'][0]['children'][0]['children'][0]['children']}
  assert fields['Type']=='Directory' and fields['Path']==str(d);assert v['root']['children'][0]['children'][0]['props']['icon']['source']=='Folder';assert 'Dimensions'not in fields and 'Duration'not in fields
  assert len(v['actions'])==4
  v=h.action('dir-'+str(nonempty),v['actions'][3]['id']);assert 'Type: Directory' in h.calls[-1][1]['text'] and str(d) in h.calls[-1][1]['text']
  v=h.action('dir-'+str(nonempty),v['actions'][0]['id']);assert h.calls[-1]==('host.clipboard.write',{'text':str(d)})
  v=h.action('dir-'+str(nonempty),v['actions'][1]['id']);assert h.calls[-1]==('host.shell.reveal',{'path':str(d)})
  v=h.action('dir-'+str(nonempty),v['actions'][2]['id']);app=v['root']['children'][0]['props']['actionIds'][0];h.action('dir-'+str(nonempty),app);assert h.calls[-1][1]['path']==str(d)
  h.dispose('dir-'+str(nonempty));h.paths=[str(d)];v=h.open('dir-clear-'+str(nonempty),'clear-file-metadata');assert v['results'][0]['state']=='failed';h.dispose('dir-clear-'+str(nonempty))
  assert d.stat().st_mtime_ns==stat.st_mtime_ns
  if nonempty:assert child.read_bytes()==snapshot and getxattr(child,'com.tezbar.file-info.qa')==b'untouched child'
  else:assert not list(d.iterdir())
  records.append({'directory':'nonempty'if nonempty else'empty','firstOfMultiple':True,'allFourActions':'owned host ACKs','clear':'rejected','children':'untouched'})
 h.close();return records
check('directory-first-selection-stat-folder-all-actions-clear-denial',directory_views)

def independent_image(p):
 try:return {'decoder':'FFmpeg','frames':decoded(p)}
 except AssertionError:
  raster=RUN/(str(uuid.uuid4())+'.png');r=subprocess.run(['/usr/bin/sips','-s','format','png',str(owned(p)),'--out',str(raster)],capture_output=True,timeout=30)
  assert r.returncode==0,r.stderr.decode();return {'decoder':'Sips then FFmpeg','frames':decoded(raster)}
def tiff_images(p):
 # Independent finite fixture reader, including both root and SubIFD raster bytes.
 import struct
 b=owned(p).read_bytes();assert b[:4]==b'II*\0';seen=set();images=[]
 def visit(offset):
  assert offset not in seen;seen.add(offset);assert len(seen)<=4
  count=struct.unpack_from('<H',b,offset)[0];tags={}
  for i in range(count):
   at=offset+2+i*12;tag,ty,n,value=struct.unpack_from('<HHII',b,at)
   if tag in [273,279,330]:
    assert ty in [3,4];unit=2 if ty==3 else 4;start=at+8 if n*unit<=4 else value
    tags[tag]=list(struct.unpack_from('<'+('H'if ty==3 else'I')*n,b,start))
  if 273 in tags:images.append([b[x:x+n].hex()for x,n in zip(tags[273],tags[279])])
  for child in tags.get(330,[]):visit(child)
  next_ifd=struct.unpack_from('<I',b,offset+2+count*12)[0]
  if next_ifd:visit(next_ifd)
 visit(struct.unpack_from('<I',b,4)[0]);return images
def additional_families():
 h=Host();records=[]
 source=json.loads((V2/'evidence/v1-oracles.json').read_text())
 detailed=next(c['args'][:-1]for row in source['cases']for c in row.get('calls',[])if '-EXIF:all='in c['args'])
 for fixture in sorted((QA/'round2-fixtures').iterdir()):
  if not fixture.is_file():continue
  p=RUN/(str(uuid.uuid4())+'-'+fixture.name);shutil.copyfile(fixture,p);before=p.read_bytes();decode=independent_image(p)if fixture.suffix not in ['.xmp']else None;subimages=tiff_images(p)if 'subifd'in fixture.name else None
  direct=[];native=None
  for index,args in enumerate([detailed,['-all=','-overwrite_original','-q']]):
   candidate=RUN/(str(uuid.uuid4())+'-'+fixture.name);candidate.write_bytes(before)
   out=subprocess.run(['/usr/bin/perl',str(QA/'tools/exiftool-13.40/exiftool'),'-config','',*args,str(candidate)],capture_output=True,timeout=60)
   direct.append({'attempt':index,'exitCode':out.returncode,'stderr':out.stderr.decode(errors='replace')})
   if out.returncode==0:native=candidate;break
  setxattr(p,'com.tezbar.file-info.qa',b'owned generic fixture');sidecar=p.with_name(p.name+'.unselected.xmp');sidecar.write_bytes((QA/'round2-fixtures/own.xmp').read_bytes());sidecar_before=sidecar.read_bytes()
  h.paths=[str(p)];v=h.open('additional-'+fixture.name,'clear-file-metadata');result=v['results'][0]
  if native is None:
   assert result['state']=='failed',v;assert p.read_bytes()==before;assert getxattr(p,'com.tezbar.file-info.qa')==b'owned generic fixture'
  else:
   assert result['state']in ['committed','partial'],v;assert p.stat().st_size>0;assert b'owned private metadata'not in p.read_bytes(),fixture.name
   assert 'trusts the native ExifTool writer'in result['detail'],result
   if decode is not None:assert independent_image(p)==decode,fixture.name
   else:assert exif(p).get('Description') is None
   if subimages is not None:assert len(subimages)==2 and tiff_images(p)==subimages
  assert sidecar.read_bytes()==sidecar_before
  records.append({'fixture':fixture.name,'nativeAttempts':direct,'production':result,'independentPayloadComparison':decode['decoder']if decode else'XMP metadata-only: native parse + private value removed; no pixel payload','sha256Before':hashlib.sha256(before).hexdigest(),'sha256After':hashlib.sha256(p.read_bytes()).hexdigest(),'unselectedSidecar':'unchanged','subIfdRasterImagesCompared':len(subimages)if subimages is not None else None})
  h.dispose('additional-'+fixture.name)
 h.close();return records
check('generic-authentic-exiftool-xmp-raster-heif-avif-subifd-fragmented',additional_families)

def native_writer_edges():
 records=[]
 for mode in ['warning','fallback','failure','empty','symlink','wrong-format','flood']:
  root=RUN/('writer-'+mode);root.mkdir();log=root/'calls.jsonl';marker=root/'first';p=copy('png');before=p.read_bytes();tool=root/'exiftool'
  # Fault injection wraps the authentic native writer; no fake positive writer.
  body=r"""#!/usr/bin/python3
import os,sys,pathlib,json
args=sys.argv[1:];root=pathlib.Path(ROOT);leaf=pathlib.Path(args[-1]);mode=MODE
with (root/'calls.jsonl').open('a') as log:log.write(json.dumps(args)+'\n')
writer='-overwrite_original'in args
if writer and mode=='failure':sys.stderr.write('owned writer failed\n');sys.exit(1)
if writer and mode=='flood':sys.stderr.write('X'*70000);sys.exit(1)
if writer and mode=='fallback' and '-EXIF:all='in args:
 leaf.write_bytes(b'failed detailed attempt poison');sys.exit(1)
if writer and mode=='fallback':assert leaf.read_bytes()==ORIGINAL
import subprocess
p=subprocess.run(['/usr/bin/perl',EXIF_TOKEN,*args]);
if p.returncode:sys.exit(p.returncode)
if writer and mode=='warning':sys.stderr.write('Warning: owned arbitrary successful diagnostic\n')
if writer and mode=='empty':leaf.write_bytes(b'')
if writer and mode=='symlink':leaf.rename(leaf.with_suffix('.kept'));leaf.symlink_to(root/'not-a-selected-file')
if writer and mode=='wrong-format':leaf.write_bytes(XMP)
sys.exit(0)
""".replace('ROOT',repr(str(root))).replace('MODE',repr(mode)).replace('EXIF_TOKEN',repr(str(QA/'tools/exiftool-13.40/exiftool'))).replace('ORIGINAL',repr(before)).replace('XMP',repr((QA/'round2-fixtures/own.xmp').read_bytes()))
  tool.write_text(body);tool.chmod(0o700);h=Host(root);setxattr(p,'com.tezbar.file-info.qa',b'retain on failure');h.paths=[str(p)];v=h.open('edge-'+mode,'clear-file-metadata');result=v['results'][0];calls=[json.loads(l)for l in log.read_text().splitlines()];writes=[a for a in calls if '-overwrite_original'in a]
  if mode in ['warning','fallback']:
   assert result['state']=='partial',v;assert decoded(p)==decoded(QA/'fixtures/own.png');assert len(writes)==(1 if mode=='warning'else 2)
   if mode=='warning':assert 'owned arbitrary successful diagnostic'in result['detail']
  else:
   assert result['state']=='failed',v;assert p.read_bytes()==before;assert getxattr(p,'com.tezbar.file-info.qa')==b'retain on failure'
  assert all(str(p)not in a for a in calls),'selected original passed to helper'
  records.append({'mode':mode,'state':result['state'],'nativeWriterAttempts':len(writes),'error':result.get('error')});h.dispose('edge-'+mode);h.close()
 return records
check('authentic-writer-warning-fresh-fallback-failure-candidate-bounds',native_writer_edges)

def framing():
 env=dict(os.environ,PATH=str(QA/'bin'))
 p=subprocess.Popen([str(BINARY)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=RUN,env=env);assert json.loads(p.stdout.readline())['protocol']==1
 try:p.stdin.write(b'x'*(16*1024*1024+1)+b'\n');p.stdin.flush()
 except BrokenPipeError:pass
 p.stdin.close();p.wait(timeout=5);assert p.returncode!=0;error=p.stderr.read().decode();assert 'large'in error.lower() or 'frame'in error.lower(),error
 return 'real SDK rejects >16 MiB before JSON parse, terminates and owns EOF cleanup'
check('real-sdk-16-mib-frame-boundary',framing)

(V2/('evidence/extracted-process-qa.json' if len(sys.argv)>1 else 'evidence/process-qa.json')).write_text(json.dumps({'binary':str(BINARY),'cases':RESULTS,'caseCount':len(RESULTS)},indent=2)+'\n')
print('PASS all',len(RESULTS),'process cases')
