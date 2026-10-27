/* Discord-style shell: existing controls are moved, never cloned or reimplemented. */
(() => {
  let connected = false;
  const byId = id => document.getElementById(id);
  function relocate(node, parent, before = null) {
    if (!node || !parent || node.parentElement === parent) return;
    const focus = document.activeElement;
    parent.insertBefore(node, before);
    if (focus && node.contains(focus) && focus.isConnected) focus.focus({ preventScroll:true });
  }
  function layout() {
    const wide = window.innerWidth >= 900;
    relocate(byId('conversation-tabs'), byId(wide ? 'conversation-rail' : 'navigation-rail-slot'), wide ? byId('rail-create')?.parentElement : null);
    relocate(byId('rail-home'),byId(wide?'conversation-rail':'navigation-rail-slot'),byId('conversation-tabs'));
    relocate(byId('voice-bar'), byId(wide ? 'sidebar-footer' : 'conversation-panel'), byId(wide ? 'account-controls' : 'reply-draft'));
    document.body.dataset.shellLayout = wide ? 'desktop' : 'compact';
  }
  function modernLayout(context, modern) {
    document.body.classList.toggle('channel-navigation',modern);
    if(!modern)return;
    let home=byId('home-content'),direct=byId('home-direct-tabs');
    if(!home){home=document.createElement('section');home.id='home-content';home.className='home-content';const heading=document.createElement('h1');heading.textContent='Friends';home.append(heading);byId('conversation-panel').prepend(home);}
    if(!direct){direct=document.createElement('nav');direct.id='home-direct-tabs';direct.setAttribute('role','tablist');direct.setAttribute('aria-label','Direct messages');direct.setAttribute('aria-orientation','vertical');byId('navigation-rooms').prepend(direct);}
    relocate(byId('private-tab-category'),direct);
    const isHome=document.body.dataset.navigationSection==='friends';
    document.body.dataset.homeOpen=String(isHome);
    home.hidden=!isHome;direct.hidden=!isHome && !context.direct;
    relocate(byId('navigation-friends'),home);byId('navigation-friends').hidden=!isHome;
    // The sidebar remains usable while Friends occupies the main content area.
    if(isHome)byId('navigation-rooms').hidden=false;
  }
  function reset() {
    delete document.body.dataset.homeOpen;document.body.classList.remove('channel-navigation');
    if(byId('home-content'))byId('home-content').hidden=true;
    if(byId('home-direct-tabs'))byId('home-direct-tabs').hidden=true;
    relocate(byId('navigation-friends'),byId('navigation-dialog'));byId('navigation-friends').hidden=true;
    relocate(byId('private-tab-category'),byId('conversation-tabs'),byId('room-tab-category'));
    document.body.dataset.conversationKind = 'none';
    byId('sidebar-room-name').textContent = 'Chat';
    byId('sidebar-channel-tools').hidden = true;
    byId('sidebar-voice').textContent = 'Join voice';byId('sidebar-voice').removeAttribute('title');
    for (const id of ['rail-home','rail-create','rail-join','sidebar-chat','sidebar-files','sidebar-invite','sidebar-emojis','sidebar-voice']) byId(id).disabled = true;
  }
  window.OchatDiscord = Object.freeze({
    init() {
      if (connected) return;
      connected = true;
      const forward = (id, target) => byId(id).addEventListener('click', () => { if (!byId(target).disabled) byId(target).click(); });
      forward('rail-home','friends-button');
      byId('rail-home').addEventListener('click',()=>{if(window.innerWidth<900 && byId('navigation-dialog').open)byId('navigation-dialog').close();});
      forward('rail-create','new-room');
      forward('rail-join','join-button');
      forward('sidebar-files','shared-items-button');
      forward('sidebar-invite','invite-button');
      forward('sidebar-emojis','shared-emojis-button');
      forward('sidebar-voice','voice-button');
      byId('sidebar-chat').addEventListener('click', () => { if (!byId('message').disabled) byId('message').focus(); });
      window.addEventListener('resize', layout);
      layout();reset();
    },
    sync(context, unlocked, modern=false) {
      layout();
      if (!unlocked) { reset();return; }
      modernLayout(context,modern);
      const kind = !context.room ? 'none' : context.direct ? 'direct' : 'room';
      document.body.dataset.conversationKind = kind;
      byId('sidebar-room-name').textContent = context.title || 'Chat';
      byId('sidebar-channel-tools').hidden = kind !== 'room';
      for (const id of ['rail-home','rail-create','rail-join','sidebar-chat']) byId(id).disabled = false;
      for (const [id,target] of [['sidebar-files','shared-items-button'],['sidebar-invite','invite-button'],['sidebar-emojis','shared-emojis-button'],['sidebar-voice','voice-button']]) byId(id).disabled = byId(target).disabled;
      byId('sidebar-voice').textContent = byId('voice-button').hidden ? 'Voice connected' : byId('voice-button').textContent;
      byId('sidebar-voice').title = byId('voice-button').title;
      byId('rail-home').setAttribute('aria-pressed', byId('friends-button').getAttribute('aria-pressed') || 'false');
    },
    reset,
    layout
  });
})();

/* Owner-managed channels share a room's identity, members and emoji library. */
(() => {
  let api, snapshot, panel, dialog, editing=null, opener=null, busy=false, voicePane, openVoice=false, displayedRoom=null;
  const rows=new Map(), unread=new Map();
  const $=id=>document.getElementById(id);
  function element(tag,cls,text){const node=document.createElement(tag);if(cls)node.className=cls;if(text!==undefined)node.textContent=text;return node;}
  function button(label,callback,cls='text-button'){const node=element('button',cls,label);node.type='button';node.onclick=callback;return node;}
  function valid(context){const current=api.context();return current.unlocked && current.generation===context.generation && current.room===context.room;}
  function updateTimers(){for(const [id,row]of rows){const started=snapshot?.voice_clocks?.[id];if(!row.clock)continue;row.clock.hidden=!started;if(started){const seconds=Math.max(0,Math.floor((Date.now()-started)/1000)),minutes=Math.floor(seconds/60);row.clock.textContent=String(minutes).padStart(2,'0')+':'+String(seconds%60).padStart(2,'0');}}}
  function canManageChannels(){return !!(snapshot?.permissions?.channels??(snapshot?.room?.owner===snapshot?.id));}
  function openEditor(channel,kind,source){
    const context=api.context();if(!context.unlocked || !canManageChannels())return;
    editing={channel,kind,context};opener=source;
    $('channel-editor-title').textContent=channel?'Edit channel':'Create channel';
    $('channel-name').value=channel?.name || '';$('channel-editor-error').textContent='';
    $('channel-delete').hidden=!channel;$('channel-kind').value=kind;$('channel-kind').disabled=!!channel;
    const siblings=(snapshot?.channels || []).filter(c=>c.kind===kind),index=siblings.findIndex(c=>c.id===channel?.id);
    $('channel-move-up').hidden=$('channel-move-down').hidden=!channel;
    $('channel-move-up').disabled=index<=0;$('channel-move-down').disabled=index<0 || index>=siblings.length-1;
    dialog.showModal();$('channel-name').focus();
  }
  async function save(event){
    event.preventDefault();if(busy || !editing)return;
    const choice=editing;if(!valid(choice.context)||!canManageChannels()){dialog.close();return;}
    const name=$('channel-name').value.trim();if(!name)return;
    busy=true;$('channel-save').disabled=true;
    try{
      if(choice.channel)await api.invoke('edit_channel',{room:choice.context.room,channel:choice.channel.id,name,order:choice.channel.order,deleted:false});
      else await api.invoke('create_channel',{room:choice.context.room,name,kind:$('channel-kind').value});
      if(valid(choice.context)){dialog.close();await api.refresh();}
    }catch(error){if(valid(choice.context))$('channel-editor-error').textContent=String(error);}
    finally{busy=false;$('channel-save').disabled=false;}
  }
  async function reorder(channel,direction){
    const context=api.context(), list=(snapshot?.channels || []).filter(c=>c.kind===channel.kind);
    const index=list.findIndex(c=>c.id===channel.id),other=list[index+direction];if(!other || !valid(context)||!canManageChannels())return;
    // Assign distinct positions to both neighbours, including imported equal orders.
    const first=direction<0?channel:other,second=direction<0?other:channel,base=Math.min(index,index+direction);
    try{
      await api.invoke('edit_channel',{room:context.room,channel:first.id,name:first.name,order:base,deleted:false});
      if(!valid(context))return;
      await api.invoke('edit_channel',{room:context.room,channel:second.id,name:second.name,order:base+1,deleted:false});
      if(valid(context))await api.refresh();
    }catch(error){if(valid(context))api.toast(error);}
  }
  function init(callbacks){
    api=callbacks;panel=element('section','room-channels');panel.id='room-channels';panel.hidden=true;panel.setAttribute('aria-label','Room channels');
    $('sidebar-channel-tools').before(panel);
    voicePane=element('section','voice-channel-view');voicePane.id='voice-channel-view';voicePane.hidden=true;
    const voiceTitle=element('h2');voiceTitle.id='voice-channel-title';const voicePeople=element('div','voice-channel-participants');voicePeople.id='voice-channel-participants';
    const back=button('Open text chat',()=>{openVoice=false;document.body.dataset.voiceView='false';const channel=snapshot?.channels?.find(c=>c.kind==='text');if(channel)api.selectText(channel.id).catch(api.toast);});
    voicePane.append(voiceTitle,element('p','field-hint','Use the voice dock to mute, share your screen, enable your camera or disconnect.'),voicePeople,back);
    $('timeline').before(voicePane);
    dialog=element('dialog','channel-editor');dialog.id='channel-editor-dialog';
    const form=element('form');form.id='channel-editor-form';form.onsubmit=save;
    const heading=element('h2','', 'Create channel');heading.id='channel-editor-title';dialog.setAttribute('aria-labelledby',heading.id);
    const label=element('label','','Name');label.htmlFor='channel-name';const input=element('input');input.id='channel-name';input.required=true;input.maxLength=48;input.autocomplete='off';
    const typeLabel=element('label','','Type');typeLabel.htmlFor='channel-kind';const type=element('select');type.id='channel-kind';for(const [value,text]of [['text','Text channel'],['voice','Voice channel']]){const option=element('option','',text);option.value=value;type.append(option);}
    const error=element('p','field-error');error.id='channel-editor-error';error.setAttribute('role','alert');
    const moveUp=button('Move up',async()=>{const choice=editing;if(!choice?.channel || !valid(choice.context))return;await reorder(choice.channel,-1);if(valid(choice.context))dialog.close();});moveUp.id='channel-move-up';
    const moveDown=button('Move down',async()=>{const choice=editing;if(!choice?.channel || !valid(choice.context))return;await reorder(choice.channel,1);if(valid(choice.context))dialog.close();});moveDown.id='channel-move-down';
    input.oninput=()=>{const dirty=editing?.channel && input.value!==editing.channel.name;const siblings=(snapshot?.channels || []).filter(c=>c.kind===editing?.kind),index=siblings.findIndex(c=>c.id===editing?.channel?.id);moveUp.disabled=dirty || index<=0;moveDown.disabled=dirty || index<0 || index>=siblings.length-1;};
    const remove=button('Delete channel',()=>{
      const choice=editing;if(!choice?.channel || !valid(choice.context))return;dialog.close();
      api.confirm('Delete '+choice.channel.name+'?', 'This permanently deletes this channel and its history for every member when they reconnect. Downloaded files and exported copies remain independent.', 'Delete channel',async()=>{
        if(!valid(choice.context)||!canManageChannels())return;
        await api.invoke('edit_channel',{room:choice.context.room,channel:choice.channel.id,name:choice.channel.name,order:choice.channel.order,deleted:true});
        if(valid(choice.context))await api.refresh();
      });
    },'text-button danger');remove.id='channel-delete';
    const actions=element('div','dialog-actions'),cancel=button('Cancel',()=>dialog.close()),submit=element('button','primary','Save');submit.type='submit';submit.id='channel-save';actions.append(cancel,submit);
    form.append(heading,label,input,typeLabel,type,error,moveUp,moveDown,remove,actions);dialog.append(form);document.body.append(dialog);
    dialog.addEventListener('close',()=>{input.value='';editing=null;if(opener?.isConnected)opener.focus({preventScroll:true});opener=null;});
  }
  function sync(next,unlocked){
    if(!api || !panel)return;snapshot=next;if(editing&&!canManageChannels())dialog.close();
    if(displayedRoom!==next?.room?.id){openVoice=false;displayedRoom=next?.room?.id;}
    const enabled=unlocked && next?.room?.capacity!==2 && Array.isArray(next?.channels);
    panel.hidden=!enabled;document.body.classList.toggle('channels-enabled',enabled);
    if(!enabled){if(voicePane)voicePane.hidden=true;document.body.dataset.voiceView='false';return;}
    if(!next.voice)openVoice=false;
    const voiceView=openVoice && next.voice;voicePane.hidden=!voiceView;document.body.dataset.voiceView=String(voiceView);
    if(voiceView){
      const channel=next.channels.find(c=>c.id===next.voice_channel);$('voice-channel-title').textContent=channel?.name || 'Voice';$('room-title').textContent=channel?.name || 'Voice';
      const roster=$('voice-channel-participants'),people=[{id:next.id,name:next.name+' (you)',avatar_hash:next.avatar_hash,tor_enabled:api.context().tor_enabled,route:'self',speaking:next.speaking},...next.peers.filter(p=>p.voice && p.voice_channel===next.voice_channel)];
      const previous=new Map([...roster.children].map(row=>[row.dataset.peer,row]));
      for(const peer of people){let person=previous.get(peer.id);if(!person){person=button('',()=>{},'voice-channel-person');person.dataset.peer=peer.id;person.avatar=element('span','avatar');person.label=element('span');person.dot=element('span','speaking-dot');person.append(person.avatar,person.label,person.dot);}previous.delete(peer.id);person.label.textContent=peer.name;api.avatar?.(person.avatar,peer);person.dot.classList.toggle('speaking',!!peer.speaking);person.onclick=()=>api.profile(peer);window.OchatCommunity?.badge(person,{...peer,online:true});if(person.parentElement!==roster)roster.append(person);}
      for(const stale of previous.values())stale.remove();
    }
    const owner=next.permissions?.channels??(next.room.owner===next.id),keys=new Set();
    for(const kind of ['text','voice']){
      let section=panel.querySelector('[data-kind="'+kind+'"]');
      if(!section){section=element('section','channel-section');section.dataset.kind=kind;const heading=element('div','channel-category',kind==='text'?'Text channels':'Voice channels');const add=button('+',()=>openEditor(null,kind,add),'channel-add');add.setAttribute('aria-label','Create '+kind+' channel');heading.append(add);const list=element('div','channel-entries');section.append(heading,list);panel.append(section);}
      section.querySelector('.channel-add').hidden=!owner;
      section.querySelector('.channel-add').disabled=next.connected===false;
      const list=section.querySelector('.channel-entries');
      for(const channel of next.channels.filter(c=>c.kind===kind)){
        keys.add(channel.id);let row=rows.get(channel.id);
        if(!row){
          row=element('div','channel-entry');row.dataset.channel=channel.id;
          row.main=button('',()=>{},'channel-shortcut channel-select');row.icon=element('span','channel-symbol');row.icon.setAttribute('aria-hidden','true');row.label=element('span','channel-name');row.badge=element('span','channel-unread');row.clock=element('span','channel-clock');row.clock.setAttribute('role','timer');row.clock.setAttribute('aria-label','Voice duration');row.main.append(row.icon,row.label,row.badge,row.clock);
          row.edit=button('•••',()=>{},'channel-manage');row.people=element('div','channel-people');row.append(row.main,row.edit,row.people);rows.set(channel.id,row);
        }
        row.label.textContent=channel.name;
        if(kind==='text')row.icon.textContent='#';else if(!row.icon.firstChild)row.icon.innerHTML='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M11 4 6 8H3v8h3l5 4V4Z"/><path d="M15 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14"/></svg>';
        row.main.setAttribute('aria-label',kind==='text'?'Open text channel '+channel.name:(next.voice_channel===channel.id?'Open':'Join')+' voice channel '+channel.name);
        row.main.setAttribute('aria-pressed',String(kind==='text'?next.text_channel===channel.id:next.voice && next.voice_channel===channel.id));
        row.main.classList.toggle('selected',kind==='text'?next.text_channel===channel.id:next.voice && next.voice_channel===channel.id);
        row.badge.textContent=unread.get(channel.id)||'';
        row.main.onclick=async()=>{
          if(row.main.disabled)return;const context=api.context();row.main.disabled=true;
          try{if(kind==='text'){openVoice=false;document.body.dataset.voiceView='false';await api.selectText(channel.id);unread.delete(channel.id);sync(snapshot,true);}else{const voice=api.context().voice;if(voice?.room?.id===next.room.id && voice.voice_channel===channel.id){openVoice=true;sync(snapshot,true);return;}api.releaseTalk?.();await api.invoke('join_voice_channel',{room:next.room.id,channel:channel.id});if(valid(context)){await api.refresh();}}}
          catch(error){if(valid(context))api.toast(error);}finally{row.main.disabled=false;}
        };
        row.edit.hidden=!owner;row.edit.setAttribute('aria-label','Manage '+channel.name);row.edit.onclick=()=>openEditor(channel,kind,row.edit);
        row.edit.disabled=next.connected===false;
        row.main.disabled=kind==='voice' && next.connected===false;
        if(row.main.disabled)row.main.title='Reconnect to this room to join voice';else row.main.removeAttribute('title');
        row.onkeydown=event=>{if(owner && event.altKey && ['ArrowUp','ArrowDown'].includes(event.key)){event.preventDefault();reorder(channel,event.key==='ArrowUp'?-1:1);}};
        const peers=kind==='voice'?[...(next.voice && next.voice_channel===channel.id?[{id:next.id,name:next.name+' (you)',avatar_hash:next.avatar_hash,tor_enabled:api.context().tor_enabled,route:'self',speaking:next.speaking}]:[]),...next.peers.filter(p=>p.voice && p.voice_channel===channel.id)]:[];
        const existing=new Map([...row.people.children].map(n=>[n.dataset.peer,n]));
        for(const peer of peers){let person=existing.get(peer.id);if(!person){person=button('',()=>{},'channel-person');person.dataset.peer=peer.id;person.avatar=element('span','avatar');person.label=element('span','channel-person-name');person.dot=element('span','speaking-dot');person.append(person.avatar,person.label,person.dot);}existing.delete(peer.id);person.label.textContent=peer.name;api.avatar?.(person.avatar,peer);person.dot.classList.toggle('speaking',!!peer.speaking);person.dot.setAttribute('aria-label',peer.speaking?'Speaking':'Silent');person.onclick=()=>api.profile(peer);window.OchatCommunity?.badge(person,{...peer,online:true});if(person.parentElement!==row.people)row.people.append(person);}
        for(const stale of existing.values())stale.remove();row.people.hidden=!peers.length;
        if(row.parentElement!==list)list.append(row);
        // Reorder existing nodes without replacing focused buttons or voice indicators.
        const expected=next.channels.filter(c=>c.kind===kind).indexOf(channel);if(list.children[expected]!==row)list.insertBefore(row,list.children[expected]||null);
      }
    }
    for(const [id,row]of rows)if(!keys.has(id)){row.remove();rows.delete(id);}
    updateTimers();
  }
  function reset(){snapshot=null;openVoice=false;displayedRoom=null;delete document.body.dataset.voiceView;rows.clear();unread.clear();panel?.replaceChildren();if(panel)panel.hidden=true;if(voicePane)voicePane.hidden=true;$('voice-channel-participants')?.replaceChildren();dialog?.close();document.body.classList.remove('channels-enabled');}
  window.OchatChannels={init,sync,reset,updateTimers,refresh:()=>api?.refresh(),setUnread(counts,channels){for(const channel of channels || []){if(counts?.[channel.id])unread.set(channel.id,counts[channel.id]);else unread.delete(channel.id);}if(snapshot && api?.context().unlocked)sync(snapshot,true);},message(event){if(event.peer!==snapshot?.id && event.channel && (event.channel!==snapshot?.text_channel || document.body.dataset.homeOpen==='true' || document.body.dataset.voiceView==='true')){unread.set(event.channel,(unread.get(event.channel)||0)+1);if(snapshot && api?.context().unlocked)sync(snapshot,true);}}};
})();
