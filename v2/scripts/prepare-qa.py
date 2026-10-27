#!/usr/bin/env python3
import hashlib,io,json,os,pathlib,subprocess,tarfile,urllib.request
V2=pathlib.Path(__file__).resolve().parents[1]; QA=V2/'.qa'; ROOT=V2.parent
assert pathlib.Path(os.environ['HOME']).is_relative_to(QA)
def fetch(url):
 with urllib.request.urlopen(url,timeout=60) as response: data=response.read(32*1024*1024+1)
 if len(data)>32*1024*1024: raise ValueError('download over bound')
 return data
icon_url='https://raw.githubusercontent.com/raycast/extensions/6734d85992290066fd31a7f5bee3cca3d3545a1a/extensions/file-info/assets/extension-icon.png'
icon=fetch(icon_url);assert hashlib.sha256(icon).hexdigest()=='b88c6a3186889071715f11d86a69f29fd234e6fdaac0c28795570039898401e6';(V2/'resources').mkdir(exist_ok=True);(V2/'resources'/'extension-icon.png').write_bytes(icon)
notice={'upstreamIcon':{'url':icon_url,'sha256':hashlib.sha256(icon).hexdigest(),'bytes':len(icon),'ownV1Sha256':hashlib.sha256((ROOT/'assets/extension-icon.png').read_bytes()).hexdigest()}}
archive=subprocess.check_output(['git','-C',str(ROOT),'archive','0700b381975cccdfdadc2cd91c83171782875fa7'])
def unpack(data,destination):
 destination.mkdir(parents=True,exist_ok=True)
 with tarfile.open(fileobj=io.BytesIO(data)) as tar:
  for member in tar.getmembers():
   p=pathlib.PurePosixPath(member.name)
   if p.is_absolute() or '..' in p.parts or not(member.isfile() or member.isdir()):raise ValueError('unsafe archive member')
   target=destination.joinpath(*p.parts)
   if member.isdir():target.mkdir(parents=True,exist_ok=True)
   else:
    target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(tar.extractfile(member).read());target.chmod(member.mode&0o777)
unpack(archive,QA/'v1')
url='https://codeload.github.com/exiftool/exiftool/tar.gz/refs/tags/13.40'
data=fetch(url);assert hashlib.sha256(data).hexdigest()=='a650e70322cef78f5ddd7037dc5e1d372207b2c5f2b5a79500e87abb3a40499e';unpack(data,QA/'tools');notice['qaExifTool']={'url':url,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'version':'13.40','role':'QA only; not packaged or installed'}
(V2/'fixtures'/'download-provenance.json').write_text(json.dumps(notice,indent=2)+'\n')
print(json.dumps(notice,indent=2))
