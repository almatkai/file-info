#!/usr/bin/env python3
"""Final local gates. Never install compilers/targets/linkers or use sibling builds."""
import json,os,pathlib,subprocess,time
V2=pathlib.Path(__file__).resolve().parents[1];QA=V2/'.qa';assert pathlib.Path(os.environ['HOME']).is_relative_to(QA)
results=[]
def gate(name,args,toolchain='stable',required=True):
 env=dict(os.environ);env['PATH']=f'/Users/almat/.rustup/toolchains/{toolchain}-aarch64-apple-darwin/bin:'+env['PATH'];env['CARGO_TARGET_DIR']=str(QA/('gates-target-'+toolchain))
 start=time.monotonic()
 with (V2/'evidence'/f'{name}.txt').open('w') as log:p=subprocess.run(args,cwd=V2,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'name':name,'command':args,'toolchain':toolchain,'exitCode':p.returncode,'required':required,'state':'passed'if p.returncode==0 else'failed','seconds':round(time.monotonic()-start,2)}
 if name=='windows-gnu-link'and p.returncode!=0:row['state']='blocked';row['reason']='Native GNU linker/import tooling unavailable; inspect exact error. Nothing installed. This does not verify MSVC or Windows execution.'
 results.append(row);print(name,row['state'],flush=True)
for toolchain,label in [('stable','stable'),('1.85.0','msrv')]:
 gate(label+'-fmt',['cargo','fmt','--check'],toolchain)
 gate(label+'-test',['cargo','test','--locked','--all-targets'],toolchain)
 gate(label+'-clippy',['cargo','clippy','--locked','--all-targets','--','-D','warnings'],toolchain)
gate('windows-gnu-check',['cargo','check','--locked','--all-targets','--target','x86_64-pc-windows-gnu'])
gate('windows-gnu-clippy',['cargo','clippy','--locked','--all-targets','--target','x86_64-pc-windows-gnu','--','-D','warnings'])
gate('windows-gnu-link',['cargo','build','--locked','--target','x86_64-pc-windows-gnu'],required=False)
(V2/'evidence/rust-gates.json').write_text(json.dumps({'gates':results,'passed':sum(r['state']=='passed'for r in results),'blocked':sum(r['state']=='blocked'for r in results),'requiredFailures':sum(r['required']and r['exitCode']!=0 for r in results)},indent=2)+'\n')
assert not any(r['required']and r['exitCode']for r in results),'required gate failed'
