const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
async function setup(t,native){
 const {Window}=await import('happy-dom');const w=new Window({settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});
 t.after(()=>w.happyDOM.close());w.document.body.innerHTML='<button id="start">Expand</button><figure id="video"></figure>';
 if(native)w.__TAURI__={window:{getCurrentWindow:()=>native}};
 w.eval(fs.readFileSync('ui/fullscreen.js','utf8'));return w;
}
test('desktop fullscreen uses its native window, restores focus and leaves a pre-existing fullscreen intact',async t=>{
 for(const initial of [false,true]){
  let full=initial;const calls=[];const w=await setup(t,{isFullscreen:async()=>full,setFullscreen:async value=>{full=value;calls.push(value);}}),start=w.document.getElementById('start'),target=w.document.getElementById('video');
  target.requestFullscreen=()=>{throw Error('DOM fullscreen must not run in a native webview');};start.focus();
  await w.OchatFullscreen.toggle(target);assert.equal(full,true);assert.equal(target.classList.contains('ochat-fullscreen'),true);
  // A renderer clearing its surface must not remove the escape control.
  target.replaceChildren();assert.ok(w.document.querySelector('.fullscreen-close'));
  await w.OchatFullscreen.exit();assert.equal(full,initial);assert.equal(w.document.activeElement,start);assert.equal(w.OchatFullscreen.active(),false);assert.deepEqual(calls,initial?[]:[true,false]);
 }
});
test('fullscreen restores the window when sharing ends or when Escape arrives during entry',async t=>{
 let full=false,finish;const w=await setup(t,{isFullscreen:async()=>false,setFullscreen:value=>{full=value;return value?new Promise(resolve=>finish=resolve):Promise.resolve();}}),target=w.document.getElementById('video');
 const entering=w.OchatFullscreen.toggle(target);await new Promise(resolve=>setTimeout(resolve,0));
 w.document.dispatchEvent(new w.KeyboardEvent('keydown',{key:'Escape'}));finish();await entering;
 assert.equal(full,false);assert.equal(w.OchatFullscreen.active(),false);
 w.__TAURI__.window.getCurrentWindow=()=>({isFullscreen:async()=>false,setFullscreen:async value=>{full=value;}});
 await w.OchatFullscreen.toggle(target);target.remove();await new Promise(resolve=>setTimeout(resolve,20));assert.equal(full,false);assert.equal(w.document.querySelector('.fullscreen-close'),null);
});
test('native failure cleans up and DOM-only fullscreen starts synchronously inside the click',async t=>{
 const w=await setup(t,{isFullscreen:async()=>false,setFullscreen:async()=>{throw Error('Unavailable');}}),target=w.document.getElementById('video');
 await assert.rejects(w.OchatFullscreen.toggle(target),/Unavailable/);assert.equal(w.OchatFullscreen.active(),false);assert.equal(target.classList.contains('ochat-fullscreen'),false);
 delete w.__TAURI__;let requested=false;target.requestFullscreen=()=>{requested=true;return Promise.reject(Error('No activation'));};
 const result=w.OchatFullscreen.toggle(target);assert.equal(requested,true);await assert.rejects(result,/No activation/);assert.equal(w.OchatFullscreen.active(),false);
});
test('native resize during a macOS fullscreen transition does not exit early and OS exit restores the layout',async t=>{
 let full=false,resize,unsubscribed=false;
 const w=await setup(t,{isFullscreen:async()=>full,setFullscreen:async()=>{},onResized:async callback=>{resize=callback;return ()=>unsubscribed=true;}});
 const target=w.document.getElementById('video');await w.OchatFullscreen.toggle(target);
 await resize();assert.equal(w.OchatFullscreen.active(),true);
 full=true;await resize();full=false;await resize();
 assert.equal(w.OchatFullscreen.active(),false);assert.equal(target.classList.contains('ochat-fullscreen'),false);assert.equal(unsubscribed,true);
});

test('the private viewer uses DOM resize rather than the privileged general event bus',async t=>{
 let full=false;const w=await setup(t,{isFullscreen:async()=>full,setFullscreen:async value=>{full=value;},onResized:()=>{throw Error('Event bus is intentionally unavailable');}});w.document.documentElement.dataset.privateViewer='true';
 const target=w.document.getElementById('video');await w.OchatFullscreen.toggle(target);assert.equal(full,true);full=false;w.dispatchEvent(new w.Event('resize'));await new Promise(resolve=>setTimeout(resolve,0));assert.equal(w.OchatFullscreen.active(),false);
});
