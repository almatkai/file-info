#!/usr/bin/env python3
"""Build a generic UNAPPLIED patch from exact pinned producers; test only QA copies."""
import difflib,hashlib,json,pathlib,subprocess
V2=pathlib.Path(__file__).resolve().parents[1];relative='src-tauri/src/native_core/extensions/process_host.rs'
original=(V2/'.qa/pinned/host'/relative).read_text();start=original.index('async fn selected_files()');end=original.index('\nfn scoped_migration_directory',start)
old=original[start:end]
new=old.replace("selection().slice(0,128)","selection()").replace(' | Select-Object -First 128','')
a=new.index('        let output = tokio::time::timeout');b=new.index('        if !output.status.success()',a)
new=new[:a]+'''        use tokio::io::AsyncReadExt;
        command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().ok_or("File selection stdout unavailable")?;
        let stderr = child.stderr.take().ok_or("File selection stderr unavailable")?;
        let output = tokio::time::timeout(Duration::from_secs(10), async {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let mut stdout=stdout.take(1024 * 1024 + 1);
            let mut stderr=stderr.take(16 * 1024 + 1);
            let (status, _, _) = tokio::try_join!(
                child.wait(),
                stdout.read_to_end(&mut out),
                stderr.read_to_end(&mut err)
            ).map_err(|e| e.to_string())?;
            if out.len() > 1024 * 1024 || err.len() > 16 * 1024 {
                return Err("File selection output exceeds bounds".to_string());
            }
            Ok(std::process::Output { status, stdout: out, stderr: err })
        }).await.map_err(|_| "File selection timed out")??;
'''+new[b:]
new=new.replace('            if path.is_absolute() && path.is_file() {\n                files.push(path);\n            }','            if !path.is_absolute() {\n                return Err("Selection contains a non-absolute path".into());\n            }\n            // Preserve the complete snapshot; the command validates ordinary-file policy.\n            files.push(path);')
patched=original[:start]+new+original[end:]
diff=''.join(difflib.unified_diff(original.splitlines(True),patched.splitlines(True),fromfile='a/'+relative,tofile='b/'+relative))
(V2/'patches').mkdir(exist_ok=True);patch=V2/'patches/0001-bounded-complete-file-selection.patch';patch.write_text(diff)
copy=V2/'.qa/host-patch'/relative;copy.parent.mkdir(parents=True,exist_ok=True);copy.write_text(original)
subprocess.run(['git','apply','--check','--directory=v2/.qa/host-patch',str(patch)],cwd=V2.parent,check=True)
subprocess.run(['git','apply','--directory=v2/.qa/host-patch',str(patch)],cwd=V2.parent,check=True)
assert copy.read_text()==patched
# Symbol resolution is replaced before evaluation: all original process builders
# resolve to a closed in-memory fake, never live osascript/PowerShell.
fixture='// Generated from host pin ea100cee3f69711de12485ba061950c5fc65258f. QA ONLY.\n'
for name,body in [('original',old),('patched',new)]:
 fixture+='pub mod '+name+' {\nuse crate::{CoreResult, Duration, json, reqwest, tokio};\n'+body.replace('async fn selected_files','pub async fn selected_files',1)+'\n}\n'
(V2/'fixtures/native-selection.rs').write_text(fixture)
subprocess.run(['rustfmt','--edition','2021',str(V2/'fixtures/native-selection.rs')],check=True)
fixture=(V2/'fixtures/native-selection.rs').read_text()
# Extract actual closed ACK producers, substituting only their effect receiver.
clipboard=(V2/'.qa/pinned/host/src-tauri/src/native_core/launcher/clipboard.rs').read_text()
platform=(V2/'.qa/pinned/host/src-tauri/src/native_core/platform.rs').read_text()
def arm(source,label,next_label):
 at=source.index('"'+label+'" => {')+len('"'+label+'" => {')
 stop=source.index('"'+next_label+'"',at)
 return source[at:stop].rsplit('}',1)[0]
copy=arm(clipboard,'clipboard:write','clipboard:write-png').replace('self.write_text','fake.write_text')
reveal=platform[platform.index('"core:platform:reveal-file" => {')+len('"core:platform:reveal-file" => {'):platform.index('            _ => Err(format!("Unknown platform operation: {channel}"')].rsplit('}',1)[0]
confirm=arm(platform,'core:platform:confirm','core:platform:clear-webview-cache').replace('self.confirm','fake.confirm')
acks='// Exact pinned ACK producer bodies with owned effect receivers. QA ONLY.\nuse crate::{CoreResult, Value, json, output, ClipboardFake, DialogFake, text_arg};\n'
acks+='#[allow(clippy::needless_return)] // Preserve the actual pinned early-return producer.\npub fn clipboard(fake:&ClipboardFake,a:&[Value])->CoreResult {'+copy+'}\n'
acks+='pub async fn reveal(text:&str)->CoreResult {'+reveal+'}\n'
acks+='pub async fn confirm(fake:&DialogFake,value:Value)->CoreResult {'+confirm+'}\n'
(V2/'fixtures/native-acks.rs').write_text(acks)
subprocess.run(['rustfmt','--edition','2021',str(V2/'fixtures/native-acks.rs')],check=True)
registry=json.loads((V2/'.qa/pinned/host/src-tauri/src/native_core/extensions/loved_extensions.json').read_text())
def find(value):
 if isinstance(value,dict):
  if value.get('id')=='raycast.file-info':return value
  for item in value.values():
   found=find(item)
   if found:return found
 if isinstance(value,list):
  for item in value:
   found=find(item)
   if found:return found
row=find(registry);assert row and {x['name']for x in row['commands']}=={'show-file-info','clear-file-metadata'}
(V2/'fixtures/registry-file-info.json').write_text(json.dumps(row,indent=2)+'\n')
report={'hostPin':'ea100cee3f69711de12485ba061950c5fc65258f','sourceSha256':hashlib.sha256(original.encode()).hexdigest(),'patchSha256':hashlib.sha256(diff.encode()).hexdigest(),'extractedFixtureSha256':hashlib.sha256(fixture.encode()).hexdigest(),'appliedTo':'ignored .qa/host-patch copy ONLY','hostWorkingTreeEdited':False,'check':'passed'}
(V2/'evidence/host-patch.json').write_text(json.dumps(report,indent=2)+'\n');print('PASS patch applies cleanly to pinned source copy; live host untouched')
