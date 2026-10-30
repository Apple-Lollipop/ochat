(() => {
  const tauri=window.__TAURI__,surface=document.getElementById('viewer-surface'),renderer=window.OchatVideoRenderer.create(surface,{requestKeyframe:event=>{if(target)tauri.core.invoke('request_visual_keyframe',{room:target.room,peer:target.peer,control:event.control}).catch(()=>{});}}),status=document.getElementById('viewer-status');
  let target=null,lastFrame=0;
  function configure(value){target=value;document.getElementById('viewer-title').textContent=value.title;document.documentElement.dataset.theme=value.theme;renderer.clear();status.textContent='Waiting for the next frame…';lastFrame=Date.now();}
  async function boot(){
    await tauri.event.listen('visual-viewer-status',({payload})=>configure(payload));
    await tauri.event.listen('visual-viewer-frame',({payload})=>{if(!target || payload.token!==target.token)return;renderer.packet(payload.packet);lastFrame=Date.now();status.textContent='Encrypted live video';});
    const initial=await tauri.core.invoke('visual_viewer_status');if(!target)configure(initial);
    document.getElementById('viewer-fullscreen').onclick=()=>window.OchatFullscreen.toggle(surface).catch(()=>{status.textContent='Fullscreen is unavailable. You can resize this window instead.';});
    window.addEventListener('ochat-fullscreen-error',()=>{status.textContent='Could not exit fullscreen. Try the window controls.';});
    document.getElementById('viewer-on-top').onchange=async event=>{try{await tauri.window.getCurrentWindow().setAlwaysOnTop(event.target.checked);}catch{event.target.checked=!event.target.checked;status.textContent='Could not change the window position preference.';}};
    setInterval(()=>{if(lastFrame && Date.now()-lastFrame>3000){renderer.clear('Video paused or sharing ended.');lastFrame=0;status.textContent='Waiting for video…';}},1000);
  }
  window.addEventListener('beforeunload',()=>{renderer.dispose();target=null;});
  boot().catch(()=>{renderer.clear('Sharing ended.');status.textContent='Close this window and open the viewer again from the call.';});
})();
