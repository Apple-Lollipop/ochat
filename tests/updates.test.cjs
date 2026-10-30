const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');
async function setup(t,check){
 const {Window}=await import('happy-dom');
 const w=new Window({settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});
 t.after(()=>w.happyDOM.close());
 w.document.body.innerHTML='<section data-settings-panel="account"></section>';
 let context={unlocked:true,generation:1,settings:{}},calls=0,opened=[];
 w.eval(fs.readFileSync('ui/updates.js','utf8'));
 w.OchatUpdates.init({invoke:async command=>{if(command==='app_version')return '0.0.2';assert.equal(command,'check_for_updates');calls++;return check();},context:()=>context,openExternalLink:async url=>opened.push(url),toast:()=>{}});
 return {w,get:id=>w.document.getElementById(id),context,calls:()=>calls,opened};
}
const newer=()=>({available:true,current:'0.0.2',latest:'0.0.3',url:'https://evil.example'});
const flush=()=>new Promise(resolve=>setImmediate(resolve));
test('new stable releases offer a choice once and open only the fixed GitHub page through the Tor-aware opener',async t=>{
 const a=await setup(t,newer);
 await a.w.OchatUpdates.accountOpened();assert.equal(a.get('update-notice').hidden,false);assert.match(a.get('update-status').textContent,/0.0.3/);
 a.get('update-notice').querySelector('.secondary').click();assert.equal(a.get('update-notice').hidden,true);
 await a.w.OchatUpdates.accountOpened();assert.equal(a.calls(),1);
 a.get('check-for-updates').click();await flush();assert.equal(a.calls(),2);
 a.get('update-notice').querySelector('.primary').click();await flush();
 assert.deepEqual(a.opened,['https://github.com/Artist6689/ochat/releases/latest']);assert.equal(a.get('update-notice').hidden,true);
});
test('automatic checks honor saved opt-out while manual checks remain available and report up-to-date versions',async t=>{
 const a=await setup(t,()=>({available:false,current:'0.0.2',latest:'0.0.2'}));a.context.settings.automatic_update_checks=false;
 a.w.OchatUpdates.fill(a.context.settings);assert.equal(a.get('automatic-update-checks').checked,false);assert.equal(a.w.OchatUpdates.read().automatic_update_checks,false);
 await a.w.OchatUpdates.accountOpened();assert.equal(a.calls(),0);
 a.get('check-for-updates').click();await flush();assert.equal(a.calls(),1);assert.match(a.get('update-status').textContent,/up to date/);assert.equal(a.get('update-notice').hidden,true);
});
test('locking discards late results, hides notices and prevents external navigation',async t=>{
 let finish;const a=await setup(t,()=>new Promise(resolve=>finish=resolve));
 const request=a.w.OchatUpdates.accountOpened();await flush();
 a.context.unlocked=false;a.context.generation++;a.w.OchatUpdates.reset();finish(newer());await request;
 assert.equal(a.get('update-notice').hidden,true);assert.equal(a.get('update-status').textContent,'');
 a.get('update-notice').querySelector('.primary').click();await flush();assert.equal(a.opened.length,0);
});
test('failed requests are non-blocking and manual checks can retry without duplicate simultaneous requests',async t=>{
 let finish;const a=await setup(t,()=>new Promise((resolve,reject)=>finish=()=>reject(Error('Offline'))));
 const first=a.w.OchatUpdates.accountOpened();await flush();a.get('check-for-updates').click();await a.w.OchatUpdates.accountOpened();assert.equal(a.calls(),1);
 finish();await first;assert.equal(a.get('update-notice').hidden,true);assert.equal(a.get('check-for-updates').disabled,false);
 a.get('check-for-updates').click();await flush();finish();await flush();assert.equal(a.calls(),2);assert.match(a.get('update-status').textContent,/try again later/);
});
