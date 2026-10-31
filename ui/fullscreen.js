/* Use the native window in desktop webviews; DOM fullscreen requires transient activation. */
(() => {
  let session=null, busy=false, exitRequested=false;
  const nativeWindow=()=>window.__TAURI__?.window?.getCurrentWindow?.();
  function cleanup(current){
    if(session!==current)return;
    current.target.classList.remove('ochat-fullscreen');
    current.close?.remove();current.observer?.disconnect();current.unlisten?.();
    session=null;
    if(current.focus?.isConnected)current.focus.focus({preventScroll:true});
  }
  async function exit(){
    if(busy){exitRequested=true;return;}
    const current=session;if(!current)return;
    busy=true;
    try{
      if(current.native && !current.wasFullscreen)await current.native.setFullscreen(false);
      else if(!current.native && document.fullscreenElement===current.target)await document.exitFullscreen();
      cleanup(current);
    }finally{busy=false;}
  }
  async function toggle(target){
    if(busy)return;
    if(session){await exit();return;}
    if(!target?.isConnected)return;
    busy=true;exitRequested=false;
    const current={target,focus:document.activeElement,native:nativeWindow()};session=current;
    try{
      if(current.native){
        current.wasFullscreen=await current.native.isFullscreen();
        if(!current.wasFullscreen)await current.native.setFullscreen(true);
        current.seenFullscreen=await current.native.isFullscreen();
        target.classList.add('ochat-fullscreen');
        const close=document.createElement('button');close.type='button';close.className='fullscreen-close';close.textContent='Exit fullscreen';close.onclick=()=>exit().catch(report);document.body.append(close);current.close=close;close.focus({preventScroll:true});
        const resized=async()=>{
          if(busy || session!==current)return;
          try{
            const full=await current.native.isFullscreen();
            if(full)current.seenFullscreen=true;
            else if(current.seenFullscreen)cleanup(current);
          }catch{/* Window may already be closing. */}
        };
        if(current.native.onResized && document.documentElement.dataset.privateViewer!=='true')current.unlisten=await current.native.onResized(resized);
        else { window.addEventListener('resize',resized);current.unlisten=()=>window.removeEventListener('resize',resized); }
      }else{
        if(!target.requestFullscreen)throw Error('Fullscreen is unavailable. Use Pop out to resize the video.');
        // Invoke immediately in the original click, before any awaited operation.
        await target.requestFullscreen();
      }
      current.observer=new MutationObserver(()=>{if(!target.isConnected)exit().catch(report);});
      current.observer.observe(document.body,{childList:true,subtree:true});
      if(!target.isConnected)exitRequested=true;
    }catch(error){
      if(current.native && current.wasFullscreen===false)await current.native.setFullscreen(false).catch(()=>{});
      cleanup(current);throw error;
    }finally{busy=false;}
    if(exitRequested)await exit();
  }
  function report(error){window.dispatchEvent(new CustomEvent('ochat-fullscreen-error',{detail:String(error?.message || error)}));}
  document.addEventListener('keydown',event=>{
    if(event.key==='Escape' && session){event.preventDefault();event.stopImmediatePropagation();exit().catch(report);}
    if(event.key==='Tab' && session?.native && session.close){
      const controls=[...session.target.querySelectorAll('button,input,select,textarea,a[href],[tabindex]'),session.close]
        .filter(node=>!node.disabled && node.tabIndex>=0 && !node.closest('[hidden]') && node.getClientRects().length);
      const index=controls.indexOf(document.activeElement),next=event.shiftKey?index-1:index+1;
      if(controls.length && (index<0 || next<0 || next>=controls.length)){
        event.preventDefault();controls[event.shiftKey?controls.length-1:0].focus({preventScroll:true});
      }
    }
  },true);
  document.addEventListener('fullscreenchange',()=>{if(session && !session.native && !document.fullscreenElement)cleanup(session);});
  window.OchatFullscreen={toggle,exit,active:()=>!!session};
})();
