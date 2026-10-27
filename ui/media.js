/* Capture controls, compressed media and ephemeral viewers. No browser storage. */
(() => {
  const $=id=>document.getElementById(id),captures=new Map(),pending=new Map(),remote=new Map(),audioStates=new Map();
  let api,epoch=0,options=null,collapsed=false,soundAvailable=false,soundMedia=null,nativeScreen=false,focusedCard=null,layoutFrame=0;
  const ctx=()=>api?.context() || {};
  const mediaId=voice=>voice?.call?.id || voice?.voice_channel || voice?.room?.id;
  const voiceLease=voice=>voice?.call?.id || voice?.voice_session || voice?.room?.id;
  const matches=(voice,media)=>!!voice?.voice && mediaId(voice)===media;
  const defaults=()=>({height:720,fps:30,quality:'balanced',share_audio:false,compatibility:false});
  function preferences(kind){return {...defaults(),...ctx().settings?.video?.[kind]};}
  function bytes64(bytes){let binary='';for(let i=0;i<bytes.length;i+=32768)binary+=String.fromCharCode(...bytes.subarray(i,i+32768));return btoa(binary);}
  async function savePreferences(kind,prefs,generation){
    const video={camera:preferences('camera'),screen:preferences('screen'),[kind]:prefs};
    await api.invoke('save_video_preferences',{video});
    if(ctx().unlocked && ctx().generation===generation)api.refreshSettings({...ctx().settings,video});
  }
  function openOptions(kind){
    const c=ctx(),voice=c.voice;
    if(!c.unlocked || !voice?.voice){api.toast('Join voice or accept the call before sharing.');return;}
    if(!voice.peers.some(p=>p.visual_media && p.visual_revision>=3)){api.toast('Watching on request requires Ochat 0.9.1 or later on both devices.');return;}
    const prefs=captures.get(kind)?.prefs || preferences(kind);
    options={kind,room:voice.room.id,media:mediaId(voice),lease:voiceLease(voice),generation:c.generation};
    $('capture-options-title').textContent=kind==='camera'?'Camera settings':'Screen sharing settings';
    $('capture-resolution').value=prefs.height;$('capture-fps').value=prefs.fps;$('capture-quality').value=prefs.quality;$('capture-compatibility').checked=!!prefs.compatibility;
    $('capture-audio-row').hidden=kind!=='screen';$('capture-system-audio').checked=kind==='screen' && !!prefs.share_audio;
    $('capture-system-audio').disabled=!soundAvailable;
    $('capture-options-hint').textContent='Targets depend on the source, device and connection. Compatibility encoding is limited to 15 FPS. Both devices need Ochat 0.9.1 or later to watch on request.';
    if(kind==='screen' && nativeScreen)$('capture-options-hint').textContent='The desktop portal lets you choose a screen or window. Native compatibility capture uses JPEG up to 15 FPS. Applying quality asks you to choose a source again.';
    if(kind==='screen' && !soundAvailable)$('capture-options-hint').textContent+=' System sound capture is unavailable on this platform.';
    $('capture-options-error').textContent='';$('capture-start').textContent=captures.has(kind)?'Apply changes':kind==='camera'?'Start camera':'Choose screen or window…';
    api.show('capture-options-dialog');
    const choice=options;const button=$('capture-start');button.disabled=true;
    api.invoke('prepare_capture',{kind,room:choice.room,media:choice.media}).then(()=>{
      if(options===choice && ctx().unlocked && ctx().generation===choice.generation)button.disabled=false;
    }).catch(error=>{if(options===choice && ctx().generation===choice.generation)$('capture-options-error').textContent=String(error);});
  }
  function captureError(error,kind){
    switch(error?.name){
      case 'NotAllowedError':return 'Capture was cancelled or access was denied. You can try again.';
      case 'NotFoundError':return kind==='camera'?'No camera was found. Connect a camera and try again.':'No screen capture source is available.';
      case 'NotReadableError':return kind==='camera'?'The camera could not be opened. Close other camera apps and try again.':'The system could not start screen capture. Check the desktop sharing service.';
      case 'OverconstrainedError':return 'This source does not support these capture settings. Try a lower resolution or frame rate.';
      case 'AbortError':return 'Capture was interrupted. You can try again.';
      case 'InvalidStateError':return 'Return to the main window and choose the capture source again.';
      default:return error?.message || String(error);
    }
  }
  function layoutStage(){
    layoutFrame=0;const stage=$('visual-stage');if(stage.hidden)return;
    const panel=stage.parentElement,panelStyle=getComputedStyle(panel);let occupied=0,count=0;
    for(const sibling of panel.children){
      if(sibling===stage||sibling.id==='timeline')continue;
      const style=getComputedStyle(sibling);if(style.display==='none'||['absolute','fixed'].includes(style.position))continue;
      occupied+=sibling.getBoundingClientRect().height+(parseFloat(style.marginTop)||0)+(parseFloat(style.marginBottom)||0);count++;
    }
    const budget=Math.max(0,panel.clientHeight-occupied-(parseFloat(panelStyle.paddingTop)||0)-(parseFloat(panelStyle.paddingBottom)||0)-(parseFloat(panelStyle.rowGap)||0)*count);
    const reserve=document.body.dataset.voiceView==='true'?0:Math.max(0,Math.min(120,budget-290));
    const padding=(parseFloat(getComputedStyle(stage).paddingTop)||0)+(parseFloat(getComputedStyle(stage).paddingBottom)||0);
    const height=collapsed?stage.querySelector('.visual-heading').getBoundingClientRect().height+padding:Math.min(focusedCard?520:360,Math.max(0,budget-reserve));
    stage.style.setProperty('--stream-stage-height',Math.floor(height)+'px');
  }
  function queueLayout(){if(!layoutFrame)layoutFrame=requestAnimationFrame(layoutStage);}
  function focusCard(card){
    focusedCard=focusedCard===card?null:card;
    for(const item of $('visual-grid').children){const active=item===focusedCard;item.classList.toggle('visual-focused',active);const button=item.querySelector('.visual-focus');if(button){button.textContent=active?'Show grid':'Focus';button.setAttribute('aria-pressed',String(active));}}
    $('visual-grid').classList.toggle('has-focus',!!focusedCard);if(focusedCard)$('visual-grid').scrollTop=0;stage();
  }
  function makeCard(name,kind,local,source){
    const card=document.createElement('figure');card.className='visual-card';card.dataset.kind=kind;card.dataset.watching=String(local);if(local)card.classList.add('local-'+kind);
    const surface=document.createElement('div');surface.className='visual-surface';
    const caption=document.createElement('figcaption'),label=document.createElement('span'),tools=document.createElement('span');tools.className='visual-card-tools';label.textContent=`${name} · ${kind==='screen'?'Screen':'Camera'}`;
    const button=(text,fn)=>{const b=document.createElement('button');b.type='button';b.className='visual-expand';b.textContent=text;b.onclick=fn;tools.append(b);return b;};
    button('Pop out',()=>{const current=source();if(current)api.invoke('open_visual_viewer',{room:current.room,peer:local?ctx().voice?.id:current.peer,kind,session:current.control?.session || current.session}).catch(api.toast);});
    button('Expand',()=>card.requestFullscreen?.().catch(api.toast));const focus=button('Focus',()=>focusCard(card));focus.classList.add('visual-focus');focus.setAttribute('aria-pressed','false');focus.disabled=!local;if(local)button('Quality…',()=>openOptions(kind));
    const audio=kind==='screen'?button(local?'Share sound':'Sound off',()=>{if(local){const c=captures.get('screen');if(c)toggleAudio(c,!c.audioActive);}}):null;
    if(audio && (!local || !soundAvailable))audio.disabled=true;
    caption.append(label,tools);card.append(surface,caption);$('visual-grid').append(card);card.streamLabel=label;return {card,surface,label,audio,focus};
  }
  function updateCaption(c){
    const profile=c.control?.profile;c.label.textContent=`You · ${c.kind==='screen'?'Screen':'Camera'}${profile?` · ${profile.height}p / ${profile.fps} FPS`:''}`;
    if(c.audio){c.audio.textContent=c.audioBusy?'Starting sound…':c.audioActive?'Stop sound':'Share sound';c.audio.disabled=!!c.audioBusy || !soundAvailable;}
  }
  async function toggleAudio(c,enabled){
    if(c.kind!=='screen' || c.audioBusy || c.stopped)return;
    const parent=c.control;c.audioBusy=true;updateCaption(c);
    try{
      if(enabled){await api.invoke('start_system_audio',{room:c.room,parent});if(c.stopped || c.control.session!==parent.session){api.invoke('stop_system_audio',{room:c.room,parent}).catch(()=>{});return;}c.audioActive=true;}
      else{await api.invoke('stop_system_audio',{room:c.room,parent});if(c.control.session===parent.session)c.audioActive=false;}
      c.prefs.share_audio=enabled;savePreferences('screen',c.prefs,c.generation).catch(error=>{if(ctx().unlocked && c.generation===ctx().generation)api.toast(error);});
    }catch(error){c.audioActive=false;if(ctx().unlocked && c.generation===ctx().generation)api.toast(`System sound is not being shared: ${error}`);}
    finally{c.audioBusy=false;if(!c.stopped)updateCaption(c);}
  }
  async function watch(item, enabled){
    if(item.busy)return;item.busy=true;item.watch.disabled=true;
    try{
      await api.invoke('watch_visual',{room:item.room,peer:item.peer,control:item.control,watching:enabled,volume:item.volume});
      if(!ctx().unlocked || ctx().generation!==item.generation || remote.get(item.peer+':'+item.kind)!==item || !matches(ctx().voice,item.media))return;
      item.watching=enabled;item.card.dataset.watching=String(enabled);item.focus.disabled=!enabled;item.watch.textContent=enabled?'Stop watching':'Watch';if(enabled&&item.kind==='screen')focusCard(item.card);else if(!enabled&&focusedCard===item.card)focusCard(item.card);
      [...item.card.querySelectorAll('.visual-expand')].slice(0,2).forEach(button=>button.disabled=!enabled);
      item.renderer.clear(enabled?'Waiting for video…':'Choose Watch to receive this stream.');
    }catch(error){api.toast(error);}finally{item.busy=false;item.watch.disabled=false;}
  }
  function offer(value){
    const voice=ctx().voice,control=value.control,key=value.peer+':'+control.kind;
    if(!matches(voice,control.media)||value.room!==voice.room.id||!voice.peers.some(p=>p.id===value.peer&&p.voice))return;
    let item=remote.get(key);
    if(!item){
      const peer=voice.peers.find(p=>p.id===value.peer);let current;
      const ui=makeCard(peer.name,control.kind,false,()=>current);
      [...ui.card.querySelectorAll('.visual-expand')].slice(0,2).forEach(button=>button.disabled=true);
      const watchButton=document.createElement('button');watchButton.className='visual-watch';watchButton.textContent='Watch';
      const initialVolume=control.kind==='screen'?1:(ctx().settings?.peer_volumes?.[value.peer] ?? 1);
      const volume=document.createElement('label');volume.className='stream-volume';volume.textContent=control.kind==='screen'?'Stream volume ':'Voice volume ';const slider=document.createElement('input');slider.type='range';slider.min=0;slider.max=200;slider.value=Math.round(initialVolume*100);slider.step=5;slider.setAttribute('aria-label',`${peer.name} ${control.kind==='screen'?'stream':'voice'} volume`);const output=document.createElement('output');output.textContent=slider.value+'%';volume.append(slider,output);
      ui.card.querySelector('.visual-card-tools').prepend(watchButton);ui.card.querySelector('figcaption').append(volume);
      item=current={...ui,generation:ctx().generation,peer:value.peer,room:value.room,kind:control.kind,session:control.session,media:control.media,control,volume:initialVolume,watching:false,watch:watchButton,renderer:window.OchatVideoRenderer.create(ui.surface,{requestKeyframe:event=>api.invoke('request_visual_keyframe',{room:value.room,peer:value.peer,control:event.control}).catch(()=>{})})};
      item.renderer.clear('Choose Watch to receive this stream.');watchButton.onclick=()=>watch(item,!item.watching);
      slider.oninput=()=>{item.volume=Number(slider.value)/100;output.textContent=slider.value+'%';};
      slider.onchange=()=>{if(item.kind==='camera'){api.invoke('set_peer_volume',{peer:item.peer,volume:item.volume}).then(()=>{if(ctx().unlocked && ctx().generation===item.generation)api.refreshSettings({...ctx().settings,peer_volumes:{...ctx().settings?.peer_volumes,[item.peer]:item.volume}});}).catch(api.toast);}else if(item.watching)api.invoke('watch_visual',{room:item.room,peer:item.peer,control:item.control,watching:true,volume:item.volume}).catch(api.toast);};
      remote.set(key,item);
    }
    if(item.session!==control.session)item.renderer.clear(item.watching?'Resuming video…':'Choose Watch to receive this stream.');
    item.session=control.session;item.control=control;item.media=control.media;stage();
  }
  function stop(kind,expected){
    if(expected && captures.get(kind)!==expected)return;
    const waiting=pending.get(kind);if(waiting){pending.delete(kind);if(waiting.nativeRequest)api.invoke('stop_linux_screen',{request:waiting.nativeRequest}).catch(()=>{});waiting.stream?.getTracks().forEach(t=>t.stop());waiting.video && (waiting.video.srcObject=null);waiting.card?.remove();}
    const c=captures.get(kind);if(!c){sync();return;}
    captures.delete(kind);c.stopped=true;try{c.encoder?.close();}catch{}if(c.native){c.renderer?.dispose();api.invoke('stop_linux_screen',{request:c.request}).catch(()=>{});}c.stream?.getTracks().forEach(t=>{t.onended=null;t.stop();});if(c.video)c.video.srcObject=null;c.card.remove();
    api.invoke('stop_visual',{room:c.room,control:c.control}).catch(()=>{});sync();
  }
  async function configure(c,prefs){
    if(c.audioBusy)throw Error('Wait for system sound to finish starting before changing quality.');
    c.reconfiguring=true;
    try{
      if(c.encoder){try{await c.encoder.flush();}catch{}await c.network?.catch(()=>{});c.encoder.close();c.encoder=null;}
      try{await c.stream.getVideoTracks()[0].applyConstraints?.({width:{ideal:Math.round(prefs.height*16/9)},height:{ideal:prefs.height},frameRate:{ideal:prefs.fps,max:prefs.fps}});}catch{}
      if(c.stopped || !matches(ctx().voice,c.media))throw Error('This call has ended.');
      const legacy=false; // Revision 3 peers explicitly opt in before receiving.
      let height=legacy?Math.min(540,prefs.height):prefs.height,fps=legacy?Math.min(10,prefs.fps):prefs.fps;
      const scale=Math.min(height/c.video.videoHeight,(height*16/9)/c.video.videoWidth,1),width=Math.max(2,Math.floor(c.video.videoWidth*scale/2)*2),actualHeight=Math.max(2,Math.floor(c.video.videoHeight*scale/2)*2);
      let codec='jpeg',encoderConfig;
      const bitrate=Math.round(width*actualHeight*(prefs.quality==='economy'?1.1:prefs.quality==='detail'?4:2.2)*Math.max(1,fps/30));
      if(!legacy && !prefs.compatibility && window.VideoEncoder && window.VideoFrame){
        encoderConfig={codec:'vp8',width,height:actualHeight,framerate:fps,bitrate:Math.max(150000,Math.min(24000000,bitrate)),latencyMode:'realtime'};
        try{if((await VideoEncoder.isConfigSupported(encoderConfig)).supported)codec='vp8';}catch{}
      }
      if(codec==='jpeg' && !legacy)fps=Math.min(fps,15);
      const profile={codec,width,height:actualHeight,fps};
      const control=await api.invoke('start_visual',{room:c.room,kind:c.kind,profile});
      if(c.stopped || !matches(ctx().voice,c.media) || ctx().generation!==c.generation){api.invoke('stop_visual',{room:c.room,control}).catch(()=>{});throw Error('This call has ended.');}
      c.control=control;c.control.profile ||= profile;c.prefs={...prefs};c.sequence=0;c.nextFrame=0;c.lastKey=0;c.forceKey=true;c.encoding=false;c.networkBusy=false;c.audioActive=false;
      c.canvas.width=width;c.canvas.height=actualHeight;
      if(codec==='vp8'){
        const identity=control.session;
        c.encoder=new VideoEncoder({output(chunk){
          c.encoding=false;if(c.stopped || c.control.session!==identity)return;
          const bytes=new Uint8Array(chunk.byteLength);chunk.copyTo(bytes);
          if(bytes.length>512*1024){c.forceKey=true;return;}
          if(chunk.type==='key'){c.lastKey=Date.now();c.forceKey=false;}
          transmit(c,bytes64(bytes),chunk.timestamp,chunk.type==='key');
        },error(error){if(!c.stopped){api.toast(`Video encoder stopped: ${error}. Try Compatibility mode.`);stop(c.kind,c);}}});
        c.encoder.configure({...encoderConfig,framerate:fps});
      }
      updateCaption(c);
      if(legacy)api.toast('An older participant is connected. Video uses compatibility quality up to 540p / 10 FPS.');
      if(c.kind==='screen' && prefs.share_audio)toggleAudio(c,true);
    }finally{c.reconfiguring=false;}
  }
  function transmit(c,data,timestamp,key){
    const sequence=++c.sequence;if(c.stopped || c.networkBusy){c.forceKey=true;return;}c.networkBusy=true;
    c.network=api.invoke('send_visual_packet',{room:c.room,control:c.control,sequence,timestamp:Math.round(timestamp),key,data}).catch(error=>{if(!c.stopped && ctx().generation===c.generation){api.toast(`Sharing stopped: ${error}`);stop(c.kind,c);}}).finally(()=>{c.networkBusy=false;});
  }
  function frame(c){
    if(c.native)return;
    if(c.stopped || c.reconfiguring || c.encoding || c.networkBusy || !c.control)return;const now=performance.now(),period=1000/c.control.profile.fps;if(now+1<(c.nextFrame || 0))return;
    if(!ctx().unlocked || ctx().generation!==c.generation || !matches(ctx().voice,c.media)){stop(c.kind,c);return;}
    if(c.video.readyState<2)return;c.nextFrame=!c.nextFrame || now-c.nextFrame>period*2?now+period:c.nextFrame+period;
    if(c.encoder){const key=c.forceKey || Date.now()-c.lastKey>=500;c.painter.drawImage(c.video,0,0,c.canvas.width,c.canvas.height);const input=new VideoFrame(c.canvas,{timestamp:Math.round(performance.now()*1000)});c.encoding=true;try{c.encoder.encode(input,{keyFrame:key});}catch{c.encoding=false;c.forceKey=true;}finally{input.close();}}
    else{
      c.painter.drawImage(c.video,0,0,c.canvas.width,c.canvas.height);
      const max=c.control.profile.width<=960 && c.control.profile.height<=540 && c.control.profile.fps<=10?131072:699052;
      let quality=c.prefs.quality==='detail'?.85:c.prefs.quality==='economy'?.4:.65,data;
      do{data=c.canvas.toDataURL('image/jpeg',quality).split(',')[1];quality-=.15;}while(data.length>max && quality>=.05);
      if(data.length<=max)transmit(c,data,performance.now()*1000,true);
    }
  }
  async function startNative(prefs,choice){
    const token={epoch,generation:ctx().generation,lease:choice.lease,nativeRequest:crypto.randomUUID()};pending.set('screen',token);sync();
    try{
      const control=await api.invoke('start_linux_screen',{room:choice.room,media:choice.media,lease:choice.lease,request:token.nativeRequest,prefs});
      if(pending.get('screen')!==token || epoch!==token.epoch || !ctx().unlocked || ctx().generation!==token.generation || voiceLease(ctx().voice)!==choice.lease){api.invoke('stop_linux_screen',{request:token.nativeRequest}).catch(()=>{});return;}
      let c;const ui=makeCard('You','screen',true,()=>c);c={...ui,native:true,request:token.nativeRequest,room:choice.room,media:choice.media,lease:choice.lease,generation:token.generation,kind:'screen',control,prefs,stopped:false};c.renderer=window.OchatVideoRenderer.create(ui.surface);captures.set('screen',c);pending.delete('screen');updateCaption(c);await savePreferences('screen',prefs,c.generation);sync();
    }finally{if(pending.get('screen')===token)pending.delete('screen');sync();}
  }
  async function start(kind,prefs,choice){
    const context=ctx(),voice=context.voice;if(!context.unlocked || choice.generation!==context.generation || (!matches(voice,choice.media) || voiceLease(voice)!==choice.lease))throw Error('This call has ended.');
    if(captures.has(kind)){const c=captures.get(kind);if(c.native){stop(kind,c);return startNative(prefs,choice);}await configure(c,prefs);savePreferences(kind,prefs,c.generation).catch(api.toast);return;}
    if(kind==='screen' && nativeScreen)return startNative(prefs,choice);
    const method=kind==='camera'?'getUserMedia':'getDisplayMedia';if(!navigator.mediaDevices?.[method])throw Error('Capture is unavailable in this system webview.');
    const token={epoch,generation:context.generation,lease:choice.lease};pending.set(kind,token);sync();
    try{
      // Keep the OS source chooser directly inside the Start button gesture.
      const stream=await navigator.mediaDevices[method](kind==='camera'?{video:{width:{ideal:Math.round(prefs.height*16/9)},height:{ideal:prefs.height},frameRate:{ideal:prefs.fps,max:prefs.fps}},audio:false}:{video:{frameRate:{ideal:prefs.fps,max:prefs.fps}},audio:false});
      token.stream=stream;
      const valid=()=>pending.get(kind)===token && epoch===token.epoch && ctx().unlocked && ctx().generation===token.generation && matches(ctx().voice,choice.media) && voiceLease(ctx().voice)===choice.lease;
      if(!valid()){stream.getTracks().forEach(t=>t.stop());return;}
      const video=document.createElement('video');video.muted=video.autoplay=video.playsInline=true;video.srcObject=stream;token.video=video;
      let c;const ui=makeCard('You',kind,true,()=>c);token.card=ui.card;ui.surface.append(video);stage();
      await video.play();if(!valid()){stream.getTracks().forEach(t=>t.stop());video.srcObject=null;ui.card.remove();return;}
      const canvas=document.createElement('canvas');c={...ui,video,stream,canvas,painter:canvas.getContext('2d',{alpha:false}),kind,room:choice.room,media:choice.media,generation:token.generation,lease:token.lease,stopped:false,prefs};
      token.capture=c;captures.set(kind,c);pending.delete(kind);
      stream.getVideoTracks().forEach(track=>track.onended=()=>stop(kind,c));
      await configure(c,prefs);
      await savePreferences(kind,prefs,c.generation);sync();frame(c);
    }catch(error){const c=captures.get(kind);if(c && c===token.capture)stop(kind);else{token.stream?.getTracks().forEach(t=>t.stop());if(token.video)token.video.srcObject=null;token.card?.remove();}throw error;}
    finally{if(pending.get(kind)===token)pending.delete(kind);sync();}
  }
  function stage(){
    const c=ctx(),voice=c.voice;
    const activeMedia=voice?.voice?mediaId(voice):null;if(activeMedia!==soundMedia){audioStates.clear();soundMedia=activeMedia;}
    for(const [key,item] of remote)if(!matches(voice,item.media) || !voice.peers.some(p=>p.id===item.peer && p.voice && (!item.control?.connection || p.voice_session===item.control.connection))){item.renderer.dispose();item.card.remove();remote.delete(key);}
    if(focusedCard&&!focusedCard.isConnected){focusedCard=null;$('visual-grid').classList.remove('has-focus');}
    $('visual-stage').hidden=!ctx().unlocked || !$('visual-grid').children.length;document.body.dataset.visualActive=String(!$('visual-stage').hidden);document.body.dataset.streamFocused=String(!!focusedCard);$('visual-grid').hidden=collapsed;$('collapse-video').textContent=collapsed?'Show video':'Hide video';$('collapse-video').setAttribute('aria-expanded',String(!collapsed));$('visual-room-name').textContent=focusedCard?.streamLabel?.textContent || (voice?c.roomTitle(voice.room.id):'');queueLayout();
  }
  function sync(){
    if(!api)return;for(const [kind,token]of [...pending])if(!ctx().unlocked || voiceLease(ctx().voice)!==token.lease)stop(kind);for(const [kind,c] of captures)if(!ctx().unlocked || (!matches(ctx().voice,c.media) || voiceLease(ctx().voice)!==c.lease))stop(kind);
    for(const kind of ['camera','screen']){const active=captures.has(kind),waiting=pending.has(kind);$(kind+'-button').disabled=!ctx().unlocked || !ctx().voice?.voice;$(kind+'-button').setAttribute('aria-pressed',String(active));$(kind+'-button').textContent=waiting?'Cancel capture…':active?(kind==='camera'?'Stop camera':'Stop sharing'):(kind==='camera'?'Camera':'Share screen…');}
    stage();
  }
  function event(value){
    const context=ctx(),voice=context.voice;if(!context.unlocked)return;
    if(value.type==='native_screen_frame'){const c=captures.get('screen');if(c?.native && c.request===value.request && matches(voice,c.media) && voiceLease(voice)===c.lease)c.renderer.packet({...value,type:'visual_packet'});api.invoke('acknowledge_screen_preview',{request:value.request}).catch(()=>{});return;}
    if(value.type==='native_screen_stopped'){const c=captures.get('screen');if(c?.native && c.request===value.request){stop('screen',c);api.toast('Screen sharing ended.');}return;}
    if(value.type==='visual_available'){offer(value);return;}
    if(value.type==='visual_keyframe'){const c=captures.get(value.control.kind);if(c?.control?.session===value.control.session&&c.room===value.room)c.forceKey=true;return;}
    if(value.type==='visual_tick'){for(const c of captures.values())if(c.room===value.room && c.media===value.media)frame(c);return;}
    if(value.type==='system_audio_state'){if(value.room!==voice?.room.id || !voice?.voice)return;audioStates.set(value.peer,value.active);if(value.peer===voice?.id){const c=captures.get('screen');if(c){c.audioActive=value.active;updateCaption(c);}}for(const item of remote.values())if(item.peer===value.peer && item.audio)item.audio.textContent=value.active?'Sound shared':'Sound off';return;}
    const control=value.control,kind=value.kind || control?.kind,session=value.session || control?.session,media=value.media || control?.media,key=value.peer+':'+kind;
    const old=remote.get(key);
    if(value.type==='visual_stopped'){if(old?.session===session){old.renderer.dispose();old.card.remove();remote.delete(key);}stage();return;}
    if(!['camera','screen'].includes(kind) || value.room!==voice?.room.id || !matches(voice,media) || !voice.peers.some(p=>p.id===value.peer && p.voice && p.visual_media))return;
    let item=old;
    if(!item || !item.watching)return;
    item.session=session;item.media=media;item.at=Date.now();if(item.audio)item.audio.textContent=audioStates.get(item.peer)?'Sound shared':'Sound off';item.renderer.packet(value);stage();
  }
  function reset(){epoch++;if(layoutFrame)cancelAnimationFrame(layoutFrame);layoutFrame=0;for(const kind of [...pending.keys(),...captures.keys()])stop(kind);for(const item of remote.values()){item.renderer.dispose();item.card.remove();}remote.clear();audioStates.clear();options=null;collapsed=false;focusedCard=null;$('visual-grid').classList.remove('has-focus');$('capture-options-dialog').close();$('visual-grid').replaceChildren();$('visual-stage').hidden=true;document.body.dataset.visualActive='false';document.body.dataset.streamFocused='false';}
  if(typeof ResizeObserver==='function'){const geometry=new ResizeObserver(queueLayout);for(const id of ['conversation-panel','voice-bar','composer','reply-draft','typing-status','voice-channel-view']){const target=$(id);if(target)geometry.observe(target);}}window.addEventListener('resize',queueLayout);
  window.OchatMedia={sync,event,reset,openOptions,
    init(callbacks){api=callbacks;api.invoke('screen_capture_capabilities').then(value=>nativeScreen=!!value?.native).catch(()=>{});api.invoke('system_audio_capabilities').then(value=>soundAvailable=!!value?.available).catch(()=>{});
      for(const kind of ['camera','screen'])$(kind+'-button').onclick=()=>captures.has(kind)||pending.has(kind)?stop(kind):openOptions(kind);
      $('collapse-video').onclick=()=>{collapsed=!collapsed;stage();};
      $('capture-options-form').onsubmit=async event=>{event.preventDefault();const choice=options;if(!choice)return;choice.newCapture=!captures.has(choice.kind);choice.busy=true;choice.completed=false;const button=$('capture-start');if(button.disabled)return;button.disabled=true;$('capture-options-error').textContent='';
        const prefs={height:Number($('capture-resolution').value),fps:Number($('capture-fps').value),quality:$('capture-quality').value,compatibility:$('capture-compatibility').checked,share_audio:choice.kind==='screen' && !$('capture-system-audio').disabled && $('capture-system-audio').checked};
        // Release the modal before invoking the OS picker, within the same user gesture.
        choice.pickerOpen=true;$('capture-options-dialog').close();
        try{await start(choice.kind,prefs,choice);choice.completed=true;}
        catch(error){if(options===choice && ctx().unlocked && ctx().generation===choice.generation && voiceLease(ctx().voice)===choice.lease && matches(ctx().voice,choice.media)){
          openOptions(choice.kind);$('capture-options-error').textContent=captureError(error,choice.kind);
        }}
        finally{choice.busy=false;choice.pickerOpen=false;if(options===choice)button.disabled=false;}
      };$('capture-options-dialog').addEventListener('close',()=>{if(options?.busy && !options.pickerOpen && !options.completed && options.newCapture)stop(options.kind);});setInterval(stage,1000);sync();
    }
  };
})();
