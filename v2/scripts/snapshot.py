#!/usr/bin/env python3
"""Read immutable Git objects; write only this package's QA/fixture area."""
import hashlib,json,pathlib,subprocess
V2=pathlib.Path(__file__).resolve().parents[1]
ROOT=V2.parent
PIN='0700b381975cccdfdadc2cd91c83171782875fa7'
SDK='e955911f0bf5b558f9d3eb5183df0adc95c22af2'
HOST='ea100cee3f69711de12485ba061950c5fc65258f'
sources={
 'source':(ROOT,PIN,['src/show-file-info.tsx','src/clear-file-metadata.tsx','src/utils.ts','test/smoke.test.ts','package.json','package-lock.json','LICENSE']),
 'sdk':(ROOT.parent/'tezbar-extension-sdk-rust',SDK,['SDK_API.md','PROTOCOL.md','HOST_INTEGRATION.md','src/runner.rs','src/context.rs','src/stdio.rs','src/lib.rs','LICENSE','NOTICE']),
 'host':(ROOT.parents[1]/'Tezbar',HOST,['docs/EXTERNAL_RUST_EXTENSIONS.md','schemas/extension-v3.schema.json','src-tauri/src/native_core/extensions/process_host.rs','src-tauri/src/native_core/extensions/registry.rs','src-tauri/src/native_core/platform.rs','src/main/extension-platform.ts','src/renderer/ExtensionRuntimeView.tsx','src/shared/extensionRuntime.ts','src/renderer/src/raycast-api/index.tsx','src/renderer/src/raycast-api/list-runtime.tsx','src/renderer/src/raycast-api/detail-runtime.tsx','src-tauri/src/native_core/launcher/clipboard.rs','src-tauri/src/native_core/launcher/mod.rs','src-tauri/src/native_core/launcher/providers.rs','src-tauri/src/native_core/extensions/loved_extensions.json','src-tauri/src/native_core/extensions/native.rs','src-tauri/src/native_core/extensions/v2_runtime.rs','src-tauri/src/native_core/extensions/process.rs','src/renderer/tauri-bridge.ts'])}
records=[]
for kind,(repo,rev,paths) in sources.items():
 for p in paths:
  result=subprocess.run(['git','-C',str(repo),'show',f'{rev}:{p}'],capture_output=True)
  if result.returncode:
   raise RuntimeError(result.stderr.decode())
  data=result.stdout; dest=V2/'.qa'/'pinned'/kind/p;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
  records.append(dict(kind=kind,commit=rev,path=p,sha256=hashlib.sha256(data).hexdigest(),gitBlob=subprocess.check_output(['git','-C',str(repo),'rev-parse',f'{rev}:{p}'],text=True).strip()))
(V2/'fixtures').mkdir(exist_ok=True)
(V2/'fixtures'/'pinned-sources.json').write_text(json.dumps(records,indent=2)+'\n')
print(f'{len(records)} pinned Git objects captured without reading sibling working files')
