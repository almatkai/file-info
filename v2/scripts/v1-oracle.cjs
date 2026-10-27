// Closed VM: original TSX/helpers execute with every IO producer denied/stubbed
// before module evaluation. No native child, network, real fs or UI enters VM.
const fs=require('node:fs');const path=require('node:path');const vm=require('node:vm');const assert=require('node:assert/strict');
const V2=path.resolve(__dirname,'..');const QA=path.join(V2,'.qa');assert(process.env.HOME.startsWith(QA));process.env.TZ='UTC';
const ts=require(path.join(QA,'v1/node_modules/typescript'));
const sourceDir=path.join(QA,'pinned/source/src');let runs=[];
const selected=path.join(QA,'oracle','owned.png');
const stats={size:5432,mode:0o100644,uid:501,gid:20,birthtime:new Date('2024-01-02T03:04:05Z'),mtime:new Date('2025-06-07T13:14:15Z'),atime:new Date('2025-07-08T00:01:02Z'),isDirectory:()=>false};
function scope(platform='darwin',options={}){
 let calls=[],toasts=[],confirms=[],promises=[],selectionReads=0,cache={};let promiseData;let settled=false;
 function owned(p){assert(typeof p==='string'&&p.startsWith(QA+path.sep),'oracle path outside QA: '+p);}
 const denied=new Proxy(Object.create(null),{get(_,key){throw Error('DENIED IO '+String(key));}});
 const execFile=async(file,args)=>{
  calls.push({file,args});
  if(['ls','file','ffprobe','mdls','sips','afinfo'].includes(file))owned(args.at(-1));
  if(file==='ls')return{stdout:'-rw-r--r-- 1 qa-owner qa-group 5432 owned'};
  if(file==='file')return{stdout:'application/octet-stream\n'};
  if(file==='which'||file==='where.exe')return{stdout:'ffprobe\n'};
  if(file==='ffprobe')return{stdout:args.includes('stream=width,height')?'2x2\n':(options.durationOutput||'3661.9\n')};
  if(file.endsWith('exiftool')||file.endsWith('exiftool.exe')){
   if(args.includes('-ver'))return{stdout:'13.40'};owned(args.at(-1));
   if(options.failAll)throw Error('owned failure');
   if(options.fallback&&args.includes('-EXIF:all='))throw Error('owned detailed failure');return{stdout:'',stderr:options.warning?'Warning: arbitrary native success warning':''};
  }
  if(['xattr','rm','chown','powershell','takeown','icacls'].includes(file)){
   const last=file==='icacls'?args[0]:file==='powershell'?args[4]:file==='takeown'?args[1]:args.at(-1);owned(last.replace('/..namedfork/rsrc',''));
   if(options.systemFails)throw Error('owned attribute failure');return{stdout:''};
  }
  throw Error('DENIED child '+file);
 };
 const api={getSelectedFinderItems:async()=>{selectionReads++;return options.empty?[]:[{path:options.selected||selected}]},showToast:async x=>{toasts.push(x)},confirmAlert:async x=>{confirms.push(x);return !options.cancel},Toast:{Style:{Failure:'failure',Success:'success',Animated:'animated'}},Alert:{ActionStyle:{Default:'default',Cancel:'cancel',Destructive:'destructive'}},Clipboard:{copy:async()=>{throw Error('DENIED clipboard')}},open:async()=>{throw Error('DENIED open')},Color:new Proxy({},{get:(_,k)=>k}),Icon:new Proxy({},{get:(_,k)=>k})};
 function component(name){const f=()=>{};return new Proxy(f,{get:(_,k)=>k==='__name'?name:component(name+'.'+String(k))});}
 for(const name of ['List','Action','ActionPanel'])api[name]=component(name);
 const jsx=(type,props)=>({type:typeof type==='string'?type:type.__name,props:props||{}});
 const fsAsync=new Proxy({constants:{X_OK:1},stat:async p=>{owned(p);return {...stats,isDirectory:()=>Boolean(options.directory)};},access:async()=>{throw Error('DENIED access')},utimes:async(p,a,m)=>{owned(p);calls.push({file:'utimes',args:[p,'now','now']});}}, {get(t,k){if(k in t)return t[k];if(k==='__esModule')return false;throw Error('DENIED fs/promises '+String(k));}});
 const fsSync=new Proxy({existsSync:p=>{calls.push({file:'existsSync',args:[p]});return p===(platform==='win32'?'C:\\Program Files\\ExifTool\\exiftool.exe':'/usr/local/bin/exiftool');}}, {get(t,k){if(k in t)return t[k];if(k==='__esModule')return false;throw Error('DENIED fs '+String(k));}});
 const modules={'@raycast/api':api,'@raycast/utils':{usePromise:fn=>{if(!promises.length){promises.push(fn().then(x=>{promiseData=x;settled=true;}));}return{isLoading:!settled,data:promiseData,error:null};}},'react/jsx-runtime':{jsx,jsxs:jsx,Fragment:'Fragment'},'child_process':{execFile},'node:child_process':{execFile},'fs':fsSync,'node:fs/promises':fsAsync,'fs/promises':fsAsync,'util':{promisify:fn=>fn},'node:util':{promisify:fn=>fn},'path':path,'node:path':path};
 function load(name){
  if(cache[name])return cache[name].exports;
  const filename=path.join(sourceDir,name+'.tsx');const file=fs.existsSync(filename)?filename:path.join(sourceDir,name+'.ts');
  const source=fs.readFileSync(file,'utf8');const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX,target:ts.ScriptTarget.ES2022}}).outputText;
  const module={exports:{}};cache[name]=module;
  const context=vm.createContext({module,exports:module.exports,require:key=>{if(key==='./utils')return load('utils');if(!(key in modules))throw Error('DENIED require '+key);return modules[key];},process:{platform,env:{USER:'qa-owner',USERNAME:'qa-owner'}},console:{log(){},warn(){},error(){}},Date,Buffer:undefined,fetch:()=>{throw Error('DENIED network')},setTimeout:()=>{throw Error('DENIED timer')},setInterval:()=>{throw Error('DENIED timer')},global:denied});
  vm.runInContext(js,context,{filename:file,timeout:1000});return module.exports;
 }
 return{load,calls,toasts,confirms,promises,get selectionReads(){return selectionReads;}};
}
function walk(root,predicate,out=[]){if(!root||typeof root!=='object')return out;if(Array.isArray(root)){for(const n of root)walk(n,predicate,out);return out;}if(predicate(root))out.push(root);for(const value of Object.values(root.props||{}))if(value&&typeof value==='object')walk(value,predicate,out);return out;}
(async()=>{
 const s=scope();const utils=s.load('utils');const Command=s.load('show-file-info').default;Command();await Promise.all(s.promises);const tree=Command();assert.equal(s.selectionReads,1);
 const info=await utils.getFileInfo(selected);const labels=walk(tree,n=>n.type==='List.Item.Detail.Metadata.Label').map(n=>({title:n.props.title,text:n.props.text}));const copy=walk(tree,n=>n.type==='Action.CopyToClipboard').map(n=>({title:n.props.title,content:n.props.content}));
 assert.deepEqual(labels.slice(0,5).map(x=>x.title),['General Information','Name','Type','Size','Extension']);
 const coercions=[];for(const text of ['1.2.3','1e','  +1.2seconds','0x10','Infinity','NaN','-0.1','1e-2tail']){const c=scope('darwin',{durationOutput:text});const got=await c.load('utils').getFileInfo(selected.replace('.png','.mp3'));coercions.push([text,got.duration]);}
 const fixture={info,labels,copy,coercions,formatBytes:[0,1,1024,1049600,123456789].map(n=>[n,utils.formatBytes(n)]),formatDuration:[0,59.99,60,3661.99].map(n=>[n,utils.formatDuration(n)])};
 fs.writeFileSync(path.join(V2,'fixtures/v1-oracle.json'),JSON.stringify(fixture,null,2)+'\n');runs.push({name:'actual-show-tsx-and-helper',selectionReads:1,labels:labels.length,copyActions:copy.length});
 for(const platform of ['darwin','win32'])for(const options of [{},{fallback:true},{failAll:true},{systemFails:true},{empty:true},{cancel:true}]){
  const s=scope(platform,options);await s.load('clear-file-metadata').default();assert.equal(s.selectionReads,1);
  const exifs=s.calls.filter(c=>(c.file.endsWith('exiftool')||c.file.endsWith('exiftool.exe'))&&c.args.includes('-overwrite_original'));
  if(options.empty||options.cancel)assert.equal(exifs.length,0);else{assert(exifs[0].args.includes('-ICC_Profile:all='));assert(exifs[0].args.includes('-QuickTime:all='));assert.equal(exifs.at(-1).args.at(-1),selected);if(options.fallback||options.failAll)assert.equal(exifs.length,2);}
  if(options.systemFails)assert.equal(s.toasts.at(-1).style,'success'); // documented V1 swallowed-attribute-error behavior
  if(options.failAll)assert.equal(s.toasts.at(-1).style,'failure');
  runs.push({name:'actual-clear-tsx',platform,options,confirmationCount:s.confirms.length,calls:s.calls.map(c=>({file:c.file,args:c.args.map(x=>typeof x==='string'?x.replace(QA,'$QA'):x)})),lastToast:s.toasts.at(-1)});
 }
 const directoryCases=[];
 for(const suffix of ['empty','nonempty.png']){
  const dir=path.join(QA,'oracle',suffix);const s=scope('darwin',{directory:true,selected:dir});const utils=s.load('utils');const Command=s.load('show-file-info').default;Command();await Promise.all(s.promises);const tree=Command();
  const labels=walk(tree,n=>n.type==='List.Item.Detail.Metadata.Label').map(n=>({title:n.props.title,text:n.props.text}));const info=await utils.getFileInfo(dir);assert.equal(info.type,'Directory');assert.equal(utils.getFileIcon(info),'Folder');assert.equal(s.selectionReads,1);
  const copies=walk(tree,n=>n.type==='Action.CopyToClipboard').map(n=>({title:n.props.title,content:n.props.content}));assert(copies[1].content.includes('Type: Directory'));
  directoryCases.push({info,labels,copies,icon:utils.getFileIcon(info)});runs.push({name:'actual-v1-directory-view',suffix,selectionReads:s.selectionReads,type:info.type,icon:'Folder'});
 }
 for(const ext of ['xmp','heic','avif','psd','jp2','tiff']){
  const p=selected.replace('.png','.'+ext),s=scope('darwin',{selected:p,warning:true});await s.load('clear-file-metadata').default();
  const writes=s.calls.filter(c=>c.args.includes('-overwrite_original'));assert.equal(writes.length,1);assert.equal(writes[0].args.at(-1),p);assert.equal(s.toasts.at(-1).style,'success');
  runs.push({name:'actual-v1-generic-clear-path-and-successful-stderr',format:ext,detailedAttempts:1,success:true,selectedPathPassedDirectly:true});
 }
 fs.writeFileSync(path.join(V2,'fixtures/v1-round2-oracle.json'),JSON.stringify({directories:directoryCases},null,2)+'\n');
 fs.writeFileSync(path.join(V2,'evidence/v1-oracles.json'),JSON.stringify({closedVM:true,cases:runs,caseCount:runs.length},null,2)+'\n');console.log('PASS',runs.length,'actual-source oracle cases; all IO closed before evaluation');
})().catch(e=>{console.error(e);process.exitCode=1});
