const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');
async function setup(t,fail=false){
 const {Window}=await import('happy-dom');const w=new Window({settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});t.after(()=>w.happyDOM.close());
 w.document.documentElement.dataset.privateViewer='true';w.document.body.innerHTML='<strong id="viewer-title"></strong><button id="viewer-fullscreen"></button><input id="viewer-on-top" type="checkbox"><main id="viewer-surface"></main><footer id="viewer-status"></footer>';
 const initial={room:'room-a',peer:'peer-a',media:'voice-a',session:'session-a',token:'token-a',title:'Private video',theme:'dark'},calls=[],packets=[],states=[];let channel,request,disposed=false;
 const renderer={packet:p=>packets.push(p),clear:s=>states.push(s),dispose:()=>disposed=true};w.OchatVideoRenderer={create:(_surface,options)=>{request=options.requestKeyframe;return renderer;}};w.OchatFullscreen={toggle:async target=>calls.push(['fullscreen',target.id])};
 class Channel {constructor(){channel=this;}}
 w.__TAURI__={core:{Channel,invoke:async(command,args)=>{calls.push([command,args]);if(command==='visual_viewer_status'){assert.ok(args.channel instanceof Channel);if(fail)throw Error('Ended');return initial;}assert.equal(command,'request_visual_keyframe');}},event:{listen:()=>{throw Error('General events must be denied');}},window:{getCurrentWindow:()=>({setAlwaysOnTop:async value=>calls.push(['on-top',value])})}};
 w.eval(fs.readFileSync('ui/viewer.js','utf8'));await new Promise(resolve=>setTimeout(resolve,0));return {w,initial,calls,packets,states,get channel(){return channel;},request:event=>request(event),get disposed(){return disposed;}};
}
test('detached viewer uses a private IPC channel and retains scoped frame, keyframe and window controls',async t=>{
 const s=await setup(t);assert.equal(s.w.document.getElementById('viewer-title').textContent,'Private video');
 s.channel.onmessage({kind:'frame',payload:{token:'wrong',packet:{sequence:0}}});assert.equal(s.packets.length,0);
 s.channel.onmessage({kind:'frame',payload:{token:'token-a',packet:{sequence:1}}});assert.equal(s.packets[0].sequence,1);
 s.request({control:{session:'session-a'}});assert.equal(s.calls.at(-1)[0],'request_visual_keyframe');assert.equal(s.calls.at(-1)[1].room,'room-a');
 s.channel.onmessage({kind:'status',payload:{...s.initial,room:'room-b',peer:'peer-b',token:'token-b',title:'Other selected video'}});
 s.channel.onmessage({kind:'frame',payload:{token:'token-a',packet:{sequence:2}}});assert.equal(s.packets.length,1);
 s.channel.onmessage({kind:'frame',payload:{token:'token-b',packet:{sequence:3}}});assert.equal(s.packets.length,2);
 s.request({control:{session:'session-b'}});assert.equal(s.calls.at(-1)[1].peer,'peer-b');
 s.w.document.getElementById('viewer-fullscreen').click();s.w.document.getElementById('viewer-on-top').dispatchEvent(new s.w.Event('change'));await new Promise(resolve=>setTimeout(resolve,0));assert.ok(s.calls.some(c=>c[0]==='fullscreen'));assert.ok(s.calls.some(c=>c[0]==='on-top'));
 s.w.dispatchEvent(new s.w.Event('beforeunload'));assert.equal(s.disposed,true);s.channel.onmessage({kind:'frame',payload:{token:'token-b',packet:{sequence:4}}});assert.equal(s.packets.length,2);
});
test('a viewer opened after sharing ends fails closed without subscribing to general events',async t=>{const s=await setup(t,true);assert.match(s.w.document.getElementById('viewer-status').textContent,/Sharing|Close this window/);assert.equal(s.packets.length,0);assert.equal(s.calls.length,1);});
