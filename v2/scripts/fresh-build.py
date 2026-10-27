#!/usr/bin/env python3
import hashlib,json,os,pathlib,subprocess,uuid
V2=pathlib.Path(__file__).resolve().parents[1];QA=V2/'.qa';assert pathlib.Path(os.environ['HOME']).is_relative_to(QA)
def source_hash():
 h=hashlib.sha256()
 for p in sorted([V2/'Cargo.toml',V2/'Cargo.lock',*V2.joinpath('src').rglob('*.rs')]):
  h.update(str(p.relative_to(V2)).encode()+b'\0'+hashlib.sha256(p.read_bytes()).digest())
 return h.hexdigest()
source_before=source_hash()
name=str(uuid.uuid4());home=QA/('fresh-cargo-'+name);target=QA/('fresh-target-'+name);assert not home.exists() and not target.exists();home.mkdir();target.mkdir();assert not list(home.iterdir()) and not list(target.iterdir())
env=dict(os.environ,CARGO_HOME=str(home),CARGO_TARGET_DIR=str(target),CARGO_ENCODED_RUSTFLAGS='\x1f'.join([f'--remap-path-prefix={V2}=/source/file-info',f'--remap-path-prefix={home}=/source/cargo']))
env.pop('RUSTFLAGS',None)
lock=hashlib.sha256((V2/'Cargo.lock').read_bytes()).hexdigest()
with (V2/'evidence/fresh-build.txt').open('w') as log:
 result=subprocess.run(['cargo','build','--locked','--release','--target','aarch64-apple-darwin'],cwd=V2,env=env,stdout=log,stderr=subprocess.STDOUT)
assert result.returncode==0,'fresh source build failed; see evidence/fresh-build.txt'
assert hashlib.sha256((V2/'Cargo.lock').read_bytes()).hexdigest()==lock
assert source_hash()==source_before,'production sources changed during fresh build'
checkouts=list((home/'git/checkouts').glob('tezbar-extension-sdk-rust-*/*'));assert len(checkouts)==1
sdk=checkouts[0];sha=subprocess.check_output(['git','-C',str(sdk),'rev-parse','HEAD'],text=True,env=env).strip();assert sha=='e955911f0bf5b558f9d3eb5183df0adc95c22af2'
pins=json.loads((V2/'fixtures/pinned-sources.json').read_text());compared=[]
for record in pins:
 if record['kind']=='sdk':
  assert hashlib.sha256((sdk/record['path']).read_bytes()).hexdigest()==record['sha256'];compared.append(record['path'])
binary=target/'aarch64-apple-darwin/release/tezbar-file-info';info=json.loads(subprocess.check_output([str(binary),'--protocol-info'],env=env));test=json.loads(subprocess.check_output([str(binary),'--self-test'],env=env))
report={'state':'passed','productionSourceSha256':source_before,'initialCargoHome':'empty','initialTarget':'empty','cargoHome':str(home),'targetDir':str(target),'sdkSource':'git+https://github.com/almatkai/tezbar-extension-sdk-rust?rev='+sha,'sdkHead':sha,'sdkFilesMatched':compared,'lockSha256':lock,'binary':str(binary),'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'protocolInfo':info,'selfTest':test}
(V2/'evidence/fresh-build.json').write_text(json.dumps(report,indent=2)+'\n');print('PASS locked HTTPS SDK build from empty Cargo home/target; immutable SDK objects matched')
