const {test}=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {prepare,expectedFiles,hash}=require('../scripts/release/prepare.cjs');
const {publish,notesForVersion}=require('../scripts/release/publish.cjs');
const {validateVersions,validateCargoLock}=require('../scripts/release/check-version.cjs');
const {verifyBuildRun}=require('../scripts/release/verify-build-run.cjs');
const {spawnSync}=require('node:child_process');
const VERSION='0.0.1',TAG='v0.0.1',COMMIT='a'.repeat(40);
function fixture(t){
 const temp=fs.realpathSync(os.tmpdir()),root=fs.mkdtempSync(path.join(temp,'ochat-autorelease-test-')),input=path.join(root,'input'),output=path.join(root,'output');
 t.after(()=>{const resolved=fs.realpathSync(root),relative=path.relative(temp,resolved);assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative)&&path.basename(resolved).startsWith('ochat-autorelease-test-'));fs.rmSync(resolved,{recursive:true,force:true});});
 for(const [folder,names] of Object.entries(expectedFiles(VERSION))){
  const directory=path.join(input,folder);fs.mkdirSync(directory,{recursive:true});
  const files=names.map(name=>{const file=path.join(directory,name);fs.writeFileSync(file,'fixture '+name);return {name,bytes:fs.statSync(file).size,sha256:hash(file)};});
  fs.writeFileSync(path.join(directory,'release.json'),JSON.stringify({version:VERSION,platform:folder==='macOS'?'macos':folder.toLowerCase(),status:'built',files}));
 }
 fs.writeFileSync(path.join(input,'macOS/validation.json'),JSON.stringify({verification:'success',signing:'ad-hoc',notarized:false}));
 return {root,input,output,make:()=>prepare(input,output,TAG,VERSION),planFile:path.join(output,'release-plan.json')};
}
function service(plan,{existing=null,annotated=false,paged=false}={}){
 const state={release:existing,calls:[],uploadFailure:false,corruptUpload:false,moveAfterUpload:false,uploaded:false};
 const assets=files=>files.map(f=>({name:f.name,size:f.bytes,digest:'sha256:'+f.sha256,state:'uploaded'}));
 state.assets=assets;
 state.run=args=>{
  state.calls.push(args);const ok=value=>({status:0,stdout:value===undefined?'':JSON.stringify(value),stderr:''});
  if(args[0]==='api'){
   if(args[1].includes('/git/ref/'))return ok({object:annotated?{type:'tag',sha:'c'.repeat(40)}:{type:'commit',sha:state.moveAfterUpload&&state.uploaded?'b'.repeat(40):COMMIT}});
   if(args[1].includes('/git/tags/'))return ok({object:{type:'commit',sha:COMMIT}});
   const value=state.release?{id:42,tag_name:plan.tag,...state.release}:null;
   if(args[1].includes('/releases?')){
    assert.ok(args.includes('--paginate')&&args.includes('--slurp'));
    return ok(paged?[[{id:41,tag_name:'v9.9.9',draft:true}],value?[value]:[]]:[value?[value]:[]]);
   }
   if(args[1].includes('/releases/tags/')&&value?.draft)return {status:1,stdout:'',stderr:'gh: Not Found (HTTP 404)'};
   return value?ok(value):{status:1,stdout:'',stderr:'gh: Not Found (HTTP 404)'};
  }
  if(args[1]==='create'){state.release={draft:true,target_commitish:COMMIT,html_url:'https://github.com/example/ochat/releases/tag/'+TAG,assets:[]};return ok();}
  if(args[1]==='upload'){
   assert.ok(!args.includes('--clobber'),'Release files must never be deleted or overwritten');
   const uploaded=args.slice(3).map(file=>plan.files.find(f=>f.name===path.basename(file)));assert.ok(uploaded.every(Boolean));state.uploaded=true;
   state.release.assets.push(...assets(state.uploadFailure?uploaded.slice(0,1):uploaded));
   if(state.uploadFailure)return {status:1,stderr:'Upload connection interrupted'};
   if(state.corruptUpload)state.release.assets[0].digest='sha256:'+'0'.repeat(64);
   return ok();
  }
  if(args[1]==='edit'){state.release.draft=false;return ok();}
  throw Error('Unexpected CLI call: '+JSON.stringify(args));
 };
 return state;
}
function context(state){return {tag:TAG,version:VERSION,repository:'example/ochat',commit:COMMIT,run:state.run};}
test('release preparation verifies five platform files and generates a combined SHA-256 file',t=>{
 const f=fixture(t),plan=f.make();assert.equal(plan.files.length,6);
 const checksums=fs.readFileSync(path.join(f.output,'SHA256SUMS.txt'),'utf8').trim().split('\n');assert.equal(checksums.length,5);
 for(const entry of plan.files)assert.equal(hash(path.join(f.output,entry.name)),entry.sha256);
 assert.equal(plan.files.some(entry=>entry.name.endsWith('.json')),false);
});
test('recovery preparation reads the original tag version rather than the automation checkout',t=>{
 const f=fixture(t),source=path.join(f.root,'original-source');fs.mkdirSync(source);fs.writeFileSync(path.join(source,'package.json'),JSON.stringify({version:VERSION}));
 const result=spawnSync(process.execPath,[path.resolve(__dirname,'../scripts/release/prepare.cjs'),f.input,f.output,TAG],{encoding:'utf8',env:{...process.env,RELEASE_SOURCE_DIR:source}});
 assert.equal(result.status,0,result.stderr);assert.equal(JSON.parse(fs.readFileSync(f.planFile)).version,VERSION);
});
test('recovery accepts only the original completed desktop run with three successful builders',()=>{
 const run={repository:{full_name:'example/ochat'},head_sha:COMMIT,status:'completed',event:'push',path:'.github/workflows/build.yml'};
 const jobs=[{name:'desktop (windows-latest, nsis, windows)',conclusion:'success'},{name:'desktop (ubuntu-24.04, linux)',conclusion:'success'},{name:'macOS (Apple Silicon) / macos',conclusion:'success'},{name:'Publish GitHub release',conclusion:'failure'}];
 assert.equal(verifyBuildRun(run,jobs,{repository:'example/ochat',commit:COMMIT}),true);
});
test('recovery rejects foreign, incomplete or pull-request artifacts and failed platform checks',()=>{
 const run={repository:{full_name:'example/ochat'},head_sha:COMMIT,status:'completed',event:'push',path:'.github/workflows/build.yml'};
 const jobs=[{name:'windows-latest',conclusion:'success'},{name:'ubuntu-24.04',conclusion:'success'},{name:'Apple Silicon',conclusion:'success'}],context={repository:'example/ochat',commit:COMMIT};
 for(const change of [{head_sha:'b'.repeat(40)},{repository:{full_name:'foreign/repo'}},{status:'in_progress'},{event:'pull_request'},{path:'.github/workflows/untrusted.yml'}])assert.throws(()=>verifyBuildRun({...run,...change},jobs,context));
 assert.throws(()=>verifyBuildRun(run,jobs.map((j,i)=>i===1?{...j,conclusion:'failure'}:j),context));
 assert.throws(()=>verifyBuildRun(run,[...jobs,{name:'Security audit',conclusion:'failure'}],context));
});
test('preparation rejects a wrong tag, missing platform and damaged artifact before staging',t=>{
 for(const failure of ['tag','missing','hash']){
  const f=fixture(t);if(failure==='missing')fs.unlinkSync(path.join(f.input,'Linux',expectedFiles(VERSION).Linux[0]));if(failure==='hash')fs.appendFileSync(path.join(f.input,'Windows',expectedFiles(VERSION).Windows[0]),'changed');
  assert.throws(()=>prepare(f.input,f.output,failure==='tag'?'v0.0.2':TAG,VERSION));assert.equal(fs.existsSync(f.output),false);
 }
});
test('preparation rejects unverified Mac builds and path traversal in manifests',t=>{
 for(const failure of ['signature','path']){
  const f=fixture(t);
  if(failure==='signature')fs.writeFileSync(path.join(f.input,'macOS/validation.json'),'{"verification":"failure"}');
  else{const file=path.join(f.input,'Windows/release.json'),data=JSON.parse(fs.readFileSync(file));data.files[0].name='../private.txt';fs.writeFileSync(file,JSON.stringify(data));}
  assert.throws(f.make);assert.equal(fs.existsSync(f.output),false);
 }
});
test('project versions must agree across npm, locks, Tauri and both Rust packages',()=>{
 const versions={npm:VERSION,lock:VERSION,lockRoot:VERSION,tauri:VERSION,ochat:VERSION,'ochat-core':VERSION};assert.equal(validateVersions(versions,TAG),VERSION);
 assert.throws(()=>validateVersions(versions,'v0.0.2'),/tag/);
 for(const key of ['lock','lockRoot','tauri','ochat','ochat-core'])assert.throws(()=>validateVersions({...versions,[key]:'0.0.2'},TAG),/mismatch/);
});
test('a stale Cargo lock is rejected even when manifest metadata has the new version',()=>{
 const lock='version = 4\n\n[[package]]\nname = "ochat"\nversion = "0.0.1"\n\n[[package]]\nname = "ochat-core"\nversion = "0.0.1"\n';
 assert.doesNotThrow(()=>validateCargoLock(lock,'0.0.1'));
 assert.throws(()=>validateCargoLock(lock,'0.0.2'),/Cargo.lock/);
 assert.throws(()=>validateCargoLock(lock.replace('name = "ochat-core"','name = "another-package"'),'0.0.1'),/ochat-core/);
});
test('Cargo lock verification accepts synchronized local packages and ignores registry name collisions',()=>{
 const lock='version = 4\r\n\r\n[[package]]\r\nname = "ochat"\r\nversion = "0.0.2"\r\n\r\n[[package]]\r\nname = "ochat-core"\r\nversion = "0.0.2"\r\n\r\n[[package]]\r\nname = "ochat-core"\r\nversion = "1.0.0"\r\nsource = "registry+https://example.invalid"\r\n';
 assert.doesNotThrow(()=>validateCargoLock(lock,'0.0.2'));
});
test('publication verifies remote fingerprints before publishing a complete draft',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan,{annotated:true}),result=publish(f.planFile,context(state));
 assert.equal(result.alreadyPublished,false);assert.equal(state.release.draft,false);assert.equal(state.release.assets.length,6);
 assert.ok(state.calls.findIndex(a=>a[1]==='upload')<state.calls.findIndex(a=>a[1]==='edit'));
 assert.ok(state.calls.some(a=>a[1]==='create'&&a.includes('--verify-tag')&&a.includes('--draft')));
});
test('an existing draft hidden from the by-tag endpoint is found on a later page and reused by ID',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan,{paged:true});state.release={draft:true,target_commitish:COMMIT,html_url:'draft',assets:[]};
 publish(f.planFile,context(state));assert.equal(state.release.draft,false);
 assert.equal(state.calls.some(a=>a[1]==='create'),false);
 assert.ok(state.calls.some(a=>a[0]==='api'&&a[1].endsWith('/releases/42')));
});
test('an interrupted upload stays a draft and resumes only missing matching files',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan);state.uploadFailure=true;
 assert.throws(()=>publish(f.planFile,context(state)),/interrupted/);assert.equal(state.release.draft,true);assert.equal(state.release.assets.length,1);assert.equal(state.calls.some(a=>a[1]==='edit'),false);
 state.uploadFailure=false;state.calls=[];publish(f.planFile,context(state));assert.equal(state.release.assets.length,6);assert.equal(state.calls.find(a=>a[1]==='upload').length,8);
});

test('curated notes are read from the built source and prepended as one literal CLI argument',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan),directory=path.join(f.root,'scripts/release');fs.mkdirSync(directory,{recursive:true});
 const notes='Discord online announcement\n\nKeeps :name: and $(literal) unchanged.';
 fs.writeFileSync(path.join(directory,'notes.json'),JSON.stringify({[VERSION]:notes}));
 assert.equal(notesForVersion(f.root,VERSION),notes);assert.equal(notesForVersion(f.root,'9.9.9'),undefined);
 publish(f.planFile,{...context(state),notes});const create=state.calls.find(args=>args[1]==='create');
 assert.equal(create[create.indexOf('--notes')+1],notes);assert.ok(create.includes('--generate-notes'));
});
test('malformed curated notes stop publication before touching GitHub',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan);
 for(const notes of ['',{},'a'.repeat(10001)])assert.throws(()=>publish(f.planFile,{...context(state),notes}),/Invalid curated release notes/);
 assert.equal(state.calls.length,0);
});
test('bad server digests prevent publishing even after an upload succeeds',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan);state.corruptUpload=true;
 assert.throws(()=>publish(f.planFile,context(state)),/verification/);assert.equal(state.release.draft,true);assert.equal(state.calls.some(a=>a[1]==='edit'),false);
});
test('an identical already-published release is idempotent and never modified',t=>{
 const f=fixture(t),plan=f.make(),state=service(plan);state.release={draft:false,target_commitish:COMMIT,html_url:'published',assets:state.assets(plan.files)};
 assert.equal(publish(f.planFile,context(state)).alreadyPublished,true);assert.ok(state.calls.every(a=>a[0]==='api'));
});
test('a conflicting published release or draft cannot have its assets overwritten',t=>{
 for(const draft of [false,true]){
  const f=fixture(t),plan=f.make(),state=service(plan);state.release={draft,target_commitish:COMMIT,assets:state.assets(plan.files)};state.release.assets[0].digest='sha256:'+'0'.repeat(64);
  assert.throws(()=>publish(f.planFile,context(state)));assert.ok(state.calls.every(a=>a[0]==='api'));
 }
});
test('changed local files and a moved remote tag stop publication',t=>{
 for(const failure of ['local','tag']){
  const f=fixture(t),plan=f.make(),state=service(plan);
  if(failure==='local')fs.appendFileSync(path.join(f.output,plan.files[0].name),'altered');else state.moveAfterUpload=true;
  assert.throws(()=>publish(f.planFile,context(state)),failure==='local'?/asset changed/:/remote tag/);assert.equal(state.calls.some(a=>a[1]==='edit'),false);
 }
});
