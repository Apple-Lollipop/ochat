/* Native desktop interactions and ephemeral visual media. No browser storage. */
(() => {
  const $=id=>document.getElementById(id);
  let api,epoch=0,recording=null,recordModifier=null,recordModifierValue=null,recordTimer=null,recordEpoch=0,volumeTarget=null,pendingDrop=null,videoCollapsed=false;
  const recordModifiers=new Set();

  function connectWindowChrome(){
    const header=document.querySelector('.app-header');
    const spacer=document.createElement('div');spacer.className='window-chrome-placeholder';spacer.hidden=true;header.after(spacer);
    let stack=[];
    const modal=dialog=>{try{return dialog.matches(':modal');}catch{return false;}};
    const syncChrome=()=>{
      stack=stack.filter(dialog=>dialog.isConnected && modal(dialog));
      for(const dialog of document.querySelectorAll('dialog[open]'))if(modal(dialog) && !stack.includes(dialog))stack.push(dialog);
      const target=stack.at(-1);
      if(target && header.parentElement!==target){
        try{header.hidePopover?.();}catch{}
        target.prepend(header);spacer.hidden=false;
        if(header.showPopover){header.setAttribute('popover','manual');try{header.showPopover();}catch{header.removeAttribute('popover');}}
      } else if(!target && header.parentElement!==spacer.parentElement){
        try{header.hidePopover?.();}catch{}
        header.removeAttribute('popover');spacer.before(header);spacer.hidden=true;
      }
    };
    // A descendant popover stays outside modal layout/clipping while remaining
    // interactive: a title bar left outside showModal() would become inert.
    new MutationObserver(syncChrome).observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['open']});
    syncChrome();
  }
  const ctx=()=>api?.context() || {};
  function mediaMatches(voice,media){return !!voice?.voice && (voice.call?.id || voice.room.id)===media;}
  function labelShortcut(id,value){$(id).dataset.shortcut=value || '';$(id).textContent='Record…'; const field=$(id+'-input');if(field){field.value=value || '';field.readOnly=false;}$(id).setAttribute('aria-pressed','false');}
  function stopRecording(){recordEpoch++;recordModifier=null;recordModifierValue=null;recordModifiers.clear();clearTimeout(recordTimer);if(recording){const id=recording;recording=null;labelShortcut(id,$(id).dataset.shortcut);api.invoke('pause_shortcuts',{paused:false}).catch(api.toast);}}
  function finishRecording(value){
    const other=recording==='microphone-shortcut'?'deafen-shortcut':'microphone-shortcut';
    if($(other+'-input').value===value){recordModifier=null;$('shortcut-status').textContent='That combination is already assigned.';return;}
    const target=recording;stopRecording();labelShortcut(target,value);$('shortcut-status').textContent='Save Settings to apply.';
  }
  function modifierCode(event){
    if(/^(Control|Alt|Shift|Meta)(Left|Right)$/.test(event.code || ''))return event.code;
    const name=event.key==='AltGraph'?'Alt':event.key;
    return ['Control','Alt','Shift','Meta'].includes(name)?name+(event.location===2?'Right':'Left'):null;
  }
  function rememberModifier(event,code){
    for(const [family,pressed] of [['Control',event.ctrlKey],['Alt',event.altKey],['Shift',event.shiftKey],['Meta',event.metaKey]]){
      if(pressed && !code.startsWith(family) && ![...recordModifiers].some(key=>key.startsWith(family)))recordModifiers.add(family+'Left');
    }
    if(code==='AltRight'){recordModifiers.delete('ControlLeft');}
    recordModifiers.add(code);
    const ordered=['Control','Alt','Shift','Meta'].map(family=>[...recordModifiers].find(key=>key.startsWith(family))).filter(Boolean);
    recordModifier=ordered.pop();
    recordModifierValue=[...ordered.map(key=>key.startsWith('Meta')?'Super':key.replace(/(Left|Right)$/,'')),recordModifier].join('+');
  }
  function sync(){if(!api)return;window.OchatMedia?.sync();const voice=ctx().voice;$('deafen-button').textContent=voice?.deafened?'Undeafen':'Deafen';$('deafen-button').setAttribute('aria-pressed',String(!!voice?.deafened));if(voice?.deafened)$('voice-hint').textContent='Deafened · microphone and playback muted';}
  function mediaEvent(event){window.OchatMedia?.event(event);}
  function drop(payload){
    $('drop-overlay').hidden=true;
    const c=ctx();if(!c.unlocked || !c.room || !Array.isArray(payload.paths) || payload.paths.length===0)return;
    if(payload.paths.length>16){api.toast('Drop up to 16 files at a time.');return;}
    if(pendingDrop){api.toast('Finish or cancel the current file offers first.');return;}
    pendingDrop={paths:[...payload.paths],room:c.room,channel:c.channel || null,replyTo:c.replyTo || null,generation:c.generation};$('drop-send').hidden=false;$('drop-cancel').textContent='Cancel';
    $('drop-room-name').textContent=`Send to ${c.roomTitle(c.room)}`;$('drop-file-list').replaceChildren();$('drop-error').textContent='';
    for(const path of payload.paths){const li=document.createElement('li');li.textContent=path.split(/[\\/]/).pop();$('drop-file-list').append(li);}
    api.show('drop-dialog');
  }
  function openVolume(person){volumeTarget=person;const value=ctx().settings?.peer_volumes?.[person.id]??1;$('peer-volume-name').textContent=`${person.name} · volume`;$('peer-volume').value=Math.round(value*100);$('peer-volume-label').textContent=`${Math.round(value*100)}%`;$('peer-volume-error').textContent='';api.show('peer-volume-dialog');}
  function reset(){epoch++;videoCollapsed=false;stopRecording();window.OchatMedia?.reset();pendingDrop=null;volumeTarget=null;for(const key of ['microphone','deafen'])labelShortcut(key+'-shortcut',null);for(const id of ['shortcut-status','drop-error','peer-volume-error'])$(id).textContent='';$('peer-volume').value=100;$('peer-volume-label').textContent='100%';const volumeButton=$('participant-volume-button');if(volumeButton){volumeButton.hidden=true;volumeButton.onclick=null;}$('visual-grid').replaceChildren();$('visual-stage').hidden=true;$('visual-room-name').textContent='';$('drop-overlay').hidden=true;$('drop-file-list').replaceChildren();$('drop-room-name').textContent='';$('peer-volume-name').textContent='Participant volume';}
  function profile(person){
    let button=$('participant-volume-button');
    if(!button){button=document.createElement('button');button.id='participant-volume-button';button.className='secondary';$('peer-voice').after(button);}
    button.hidden=!person.voice || person.id===ctx().state?.id;button.textContent='Participant volume…';button.onclick=()=>openVolume(person);
  }
  window.OchatFeatures={sync,reset,mediaEvent,profile,drop,fillShortcuts(settings){stopRecording();for(const key of ['microphone','deafen'])labelShortcut(key+'-shortcut',settings[key+'_shortcut']);},readShortcuts(){return {microphone_shortcut:$('microphone-shortcut-input').value.trim() || null,deafen_shortcut:$('deafen-shortcut-input').value.trim() || null};},
    init(callbacks){
      api=callbacks;connectWindowChrome();window.OchatMedia?.init(callbacks);const native=window.__TAURI__?.window?.getCurrentWindow?.();
      for(const [id,method] of [['window-minimize','minimize'],['window-maximize','toggleMaximize'],['window-close','close']])$(id).onclick=()=>native?.[method]().catch(api.toast);
      const header=document.querySelector('.app-header');header.addEventListener('mousedown',event=>{if(event.button===0 && !event.target.closest('button')){event.detail===2?native?.toggleMaximize().catch(api.toast):native?.startDragging().catch(api.toast);}});
      $('deafen-button').onclick=()=>{const voice=ctx().voice;if(voice)api.action($('deafen-button'),()=>api.invoke('set_deafened',{room:voice.room.id,deafened:!voice.deafened}));};
      for(const key of ['microphone','deafen']){
        const id=key+'-shortcut';
        $(id).onclick=async()=>{
          stopRecording();const request=recordEpoch;recording=id;$(id).textContent='Preparing…';$(id).setAttribute('aria-pressed','true');$(id+'-input').readOnly=true;
          try{await api.invoke('pause_shortcuts',{paused:true});}
          catch(error){if(request===recordEpoch){stopRecording();api.toast(error);}return;}
          if(request!==recordEpoch || recording!==id)return;
          $(id).textContent='Press a combination…';$(id+'-input').focus({preventScroll:true});$('shortcut-status').textContent='Listening: press any key or combination. Click another field to cancel.';recordTimer=setTimeout(stopRecording,12000);
        };
        $('clear-'+id).onclick=()=>{stopRecording();labelShortcut(id,null);};
        $(id+'-input').addEventListener('input',()=>{$(id).dataset.shortcut=$(id+'-input').value.trim();});
        $(id+'-input').addEventListener('blur',()=>{if(recording===id)stopRecording();});
      }
      window.addEventListener('keydown',event=>{
        if(!recording)return;event.preventDefault();event.stopImmediatePropagation();
        
        const modifier=modifierCode(event);
        if(modifier){rememberModifier(event,modifier);$('shortcut-status').textContent='Release a key to bind this modifier combination, or press another key to add it.';return;}
        recordModifier=null;recordModifierValue=null;recordModifiers.clear();
        const logicalLetter=!/Mac/i.test(navigator.platform || '') && /^[a-z]$/i.test(event.key);
        const punctuation={186:'Semicolon',187:'Equal',188:'Comma',189:'Minus',190:'Period',191:'Slash',192:'Backquote',219:'BracketLeft',220:'Backslash',221:'BracketRight',222:'Quote'};
        const windowsPunctuation=!/Mac/i.test(navigator.platform||'')?punctuation[event.keyCode]:null;
        const code=windowsPunctuation || (logicalLetter?'Key'+event.key.toUpperCase():(event.code && event.code!=='Unidentified'?event.code:(/^[a-z]$/i.test(event.key)?'Key'+event.key.toUpperCase():/^[0-9]$/.test(event.key)?'Digit'+event.key:event.key===' '?'Space':event.key)));
        if(!/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Arrow(Up|Down|Left|Right)|Home|End|PageUp|PageDown|Insert|Delete|Backspace|Enter|Tab|Numpad[0-9]|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|CapsLock|NumLock|ScrollLock|PrintScreen|Pause|ContextMenu|Escape|AudioVolume(Up|Down|Mute)|Media(Play|Pause|PlayPause|Stop|TrackNext|TrackPrevious)|Numpad(Add|Subtract|Multiply|Divide|Decimal|Enter|Equal))$/.test(code)){ $('shortcut-status').textContent='Choose a letter, number, function or navigation key.';return;}
        const value=[event.ctrlKey?'Control':null,event.altKey?'Alt':null,event.shiftKey?'Shift':null,event.metaKey?'Super':null,code].filter(Boolean).join('+');
        finishRecording(value);
      },true);
      window.addEventListener('keyup',event=>{
        if(!recording || !recordModifierValue || !recordModifiers.has(modifierCode(event)))return;
        event.preventDefault();event.stopImmediatePropagation();
        finishRecording(recordModifierValue);
      },true);
      $('settings-dialog').addEventListener('close',stopRecording);
      $('peer-volume').oninput=()=>$('peer-volume-label').textContent=$('peer-volume').value+'%';
      $('peer-volume-reset').onclick=()=>{$('peer-volume').value=100;$('peer-volume-label').textContent='100%';};
      $('peer-volume-save').onclick=()=>api.action($('peer-volume-save'),async()=>{const c=ctx(),person=volumeTarget;if(!person)return;const volume=Number($('peer-volume').value)/100;try{await api.invoke('set_peer_volume',{peer:person.id,volume});if(!ctx().unlocked || c.generation!==ctx().generation)return;const settings=await api.invoke('get_settings');if(ctx().unlocked && c.generation===ctx().generation){api.refreshSettings(settings);$('peer-volume-dialog').close();}}catch(error){if(ctx().unlocked && c.generation===ctx().generation)$('peer-volume-error').textContent=String(error);}});
      $('drop-send').onclick=()=>api.action($('drop-send'),async()=>{const pending=pendingDrop;if(!pending || ctx().generation!==pending.generation)return;try{const errors=await api.invoke('send_dropped_files',pending);if(!ctx().unlocked || ctx().generation!==pending.generation)return;pendingDrop=null;if(errors?.length){$('drop-error').textContent=errors.join('\n');$('drop-file-list').replaceChildren();$('drop-send').hidden=true;$('drop-cancel').textContent='Close';}else $('drop-dialog').close();}catch(error){if(ctx().unlocked)$('drop-error').textContent=String(error);}});
      $('drop-dialog').addEventListener('close',()=>{pendingDrop=null;$('drop-file-list').replaceChildren();});
      native?.onDragDropEvent(({payload})=>{if(payload.type==='drop')drop(payload);else $('drop-overlay').hidden=!(ctx().unlocked && ctx().room && ['enter','over'].includes(payload.type));}).catch(api.toast);
      // Prevent webview navigation if a backend lets HTML drop events through.
      document.addEventListener('dragover',event=>{if(event.dataTransfer?.types?.includes('Files'))event.preventDefault();});
      document.addEventListener('drop',event=>{if(event.dataTransfer?.types?.includes('Files'))event.preventDefault();});
      window.addEventListener('beforeunload',reset);
      sync();
    }
  };
})();

