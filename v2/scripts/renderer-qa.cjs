// Execute actual pinned renderer parsing code with closed, inert React/UI modules.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const V2=path.resolve(__dirname,'..'),QA=path.join(V2,'.qa');assert(process.env.HOME.startsWith(QA));
const ts=require(path.join(QA,'v1/node_modules/typescript'));const base=path.join(QA,'pinned/host/src/renderer/src/raycast-api');const modules={};
function load(name){
 if(modules[name])return modules[name].exports;
 let src=fs.readFileSync(path.join(base,name+'.tsx'),'utf8');
 if(name==='list-runtime')src+='\nexport {parseListRows, clientSideFilter, metadataFromListDetail};\n';
 if(name==='detail-runtime')src+='\nexport {normalizeMetadataType, metadataText};\n';
 const js=ts.transpileModule(src,{compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX,target:ts.ScriptTarget.ES2022}}).outputText;
 const module={exports:{}};modules[name]=module;
 const jsx=(type,props)=>({type,props});const react={createElement:jsx,useMemo:()=>{throw Error('DENIED hook')},useState:()=>{throw Error('DENIED hook')},useEffect:()=>{throw Error('DENIED hook')},useRef:()=>{throw Error('DENIED hook')}};
 const context=vm.createContext({module,exports:module.exports,require:key=>{
  if(key==='react')return react;if(key==='react/jsx-runtime')return{jsx,jsxs:jsx};if(key==='./detail-runtime')return load('detail-runtime');
  if(['../../ui/primitives','../../ui/Markdown','../../colorConverter'].includes(key))return{};
  throw Error('DENIED require '+key);
 },console:{log(){},warn(){},error(){}}});
 vm.runInContext(js,context,{timeout:1000});return module.exports;
}
const list=load('list-runtime'),detail=load('detail-runtime');const view=JSON.parse(fs.readFileSync(path.join(QA,'production-view.json')));
const rows=list.parseListRows(view.root);assert.equal(rows[0].title,'File Info');assert.equal(rows[0].section,'File Overview');assert.deepEqual(Array.from(rows[0].actionIds),view.actions.map(a=>a.id));
const metadata=list.metadataFromListDetail(rows[0].detail);assert(metadata);const titles=metadata.children.filter(n=>n.type.endsWith('Label')).map(n=>n.props.title);assert.deepEqual(titles.slice(0,5),['General Information','Name','Type','Size','Extension']);
assert.equal(detail.normalizeMetadataType('List.Item.Detail.Metadata.Label'),'Detail.Metadata.Label');assert.equal(detail.metadataText({text:'owned QA'}),'owned QA');
assert(list.clientSideFilter(rows,'FILE OVERVIEW').some(r=>r.id==='overview'));assert.equal(list.clientSideFilter(rows,'no such text').length,0);assert.equal(list.clientSideFilter(rows,'').length,rows.length);
const pinned=fs.readFileSync(path.join(QA,'pinned/host/src-tauri/src/native_core/extensions/process_host.rs'),'utf8');const patched=fs.readFileSync(path.join(QA,'host-patch/src-tauri/src/native_core/extensions/process_host.rs'),'utf8');
function script(source){return source.match(/r#"(JSON.stringify\(Application\('Finder'\)[\s\S]*?)"#/)[1];}
function selected(source){return JSON.parse(vm.runInNewContext(script(source),{JSON,Application:name=>{assert.equal(name,'Finder');return{selection:()=>Array.from({length:129},(_,i)=>({url:()=>`file://${QA}/generated-${i}`}))};}},{timeout:1000}));}
assert.equal(selected(pinned).length,128);assert.equal(selected(patched).length,129);
// Exact pinned producer ACK literals, evaluated with owned action stubs only.
const clipboard=fs.readFileSync(path.join(QA,'pinned/host/src-tauri/src/native_core/launcher/clipboard.rs'),'utf8');assert(clipboard.includes('self.write_text(text_arg(a, 0)?.into())?;\n                return Ok(json!({"ok":true}));'));
const native=fs.readFileSync(path.join(QA,'pinned/host/src-tauri/src/native_core/extensions/native.rs'),'utf8');assert(native.includes('Err(message) => Some(Ok(json!({"ok":false,"message":message})))'));
const platform=fs.readFileSync(path.join(QA,'pinned/host/src-tauri/src/native_core/platform.rs'),'utf8');assert(platform.includes('"core:platform:reveal-file"'));assert(platform.includes('output("/usr/bin/open", &["-R", text], 10).await?;'));assert(platform.includes('Ok(json!({"ok":true}))'));assert(platform.includes('"response":if self.confirm(&value).await?{1}else{0}'));
fs.writeFileSync(path.join(V2,'evidence/renderer-producers.json'),JSON.stringify({renderer:'actual pinned ListRuntime/detail parsing functions executed in closed VM',rows:rows.length,metadataLabels:titles,clientFilter:'passed',finderProducer:{original129Selection:128,patched129Selection:129},clipboardAck:'closed {ok:true}',revealAck:'closed {ok:true}',confirmation:'closed {response:0|1}',windowsPowerShell:'source extraction only; not executed on this macOS host'},null,2)+'\n');
console.log('PASS pinned renderer, metadata, local filter and original/patched Finder scripts; OS services inert');
// Exact pinned action callback: expired errors preserve the view and never show
// the copy fallback. An unknown-outcome production response has failure tone.
(async()=>{
 const source=fs.readFileSync(path.join(QA,'pinned/host/src/renderer/ExtensionRuntimeView.tsx'),'utf8');
 const body=source.match(/onInvokeAction=\{async \(actionId, formValues\) => \{([\s\S]*?)\n            \}\}/)[1];
 const helpers=source.slice(source.indexOf('function fallbackActionNotice'),source.indexOf('function fromRunResult'));
 const js=ts.transpileModule(helpers+'\nexport async function invoke(actionId,formValues){'+body+'\n}',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 async function invoke(reply){
  const trace={state:[],notices:[],failures:[],errors:[]};const module={exports:{}};
  const context=vm.createContext({module,exports:module.exports,state:{sessionId:'owned-session',actions:[{id:'copy',kind:'copy',title:'Copy File Path'}]},currentSession:{current:'owner'},sessionOwner:'owner',isFormSurface:false,window:{tezbar:{extensionInvokeAction:async()=>reply}},setError:x=>trace.errors.push(x),setState:x=>trace.state.push(x),showActionNotice:(...x)=>trace.notices.push(x),showActionFailure:x=>trace.failures.push(x)});
  vm.runInContext(js,context,{timeout:1000});await module.exports.invoke('copy',{});return trace;
 }
 const expired=await invoke({ok:false,message:'Action expired; no operation was performed'});assert.equal(expired.state.length,0);assert.equal(expired.notices.length,0);assert.equal(expired.failures.length,1);
 const unknown=JSON.parse(fs.readFileSync(path.join(QA,'production-unknown-view.json')));const trace=await invoke(unknown);assert.equal(trace.state.length,1);assert.equal(trace.failures.length,0);assert.equal(trace.notices[0][0],'Failed · Outcome unknown · '+unknown.message);
 const reportPath=path.join(V2,'evidence/renderer-producers.json');const report=JSON.parse(fs.readFileSync(reportPath));report.actionCallback={expired:'no view replacement or copied notice',unknown:'actual production failure effect displayed',effects:'all host callbacks remain owned fakes'};fs.writeFileSync(reportPath,JSON.stringify(report,null,2)+'\n');console.log('PASS exact pinned action callback error/unknown-outcome pipeline');
})().catch(e=>{console.error(e);process.exitCode=1});
