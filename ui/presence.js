/* Availability and voice activity deliberately use separate indicators. */
(() => {
  const labels={available:'Available',away:'Away',busy:'Do not disturb',invisible:'Invisible',offline:'Offline'};
  const roles={owner:'Owner',administrator:'Administrator',moderator:'Moderator',member:'Member'};
  function scope(room){
    const snapshot=snapshotFor(room),saved=rooms.find(value=>value.id===room);
    return {snapshot,room:snapshot?.room||saved};
  }
  function member(id,room){
    const {snapshot}=scope(room);
    if(id===state?.id)return snapshot?{...snapshot,online:true}:null;
    const live=snapshot?.peers?.find(peer=>peer.id===id&&peer.online!==false);
    const saved=snapshot?.members?.find(peer=>peer.id===id)||(roomMembers.get(room)||[]).find(peer=>peer.id===id);
    return live?{...live,online:true}:saved?{...saved,online:false,voice:false,speaking:false}:null;
  }
  function roleBadge(parent,person,room){
    let badge=parent?.querySelector(':scope > .member-role');if(!parent)return;
    const context=scope(room).room;
    const label=context&&context.capacity!==2&&Object.prototype.hasOwnProperty.call(roles,person?.role)?(scope(room).snapshot?.role_names?.[person.role]||roles[person.role]):null;
    if(!label){badge?.remove();return;}
    if(!badge){badge=document.createElement('span');badge.className='member-role';parent.append(badge);}
    badge.title=roles[person.role];badge.textContent=label;badge.dataset.role=person.role;badge.setAttribute('aria-label','Room role: '+label);
    const candidate=scope(room).snapshot?.role_colors?.[person.role];
    const color=/^#[0-9a-f]{6}$/i.test(candidate||'')?candidate:null;
    if(color)badge.style.setProperty('--role-color',color);else badge.style.removeProperty('--role-color');
  }
  function paintAvatar(avatar,person,room){
    if(!avatar)return;
    const {snapshot,room:context}=scope(room),raw=context&&context.capacity!==2&&person?.role?snapshot?.role_colors?.[person.role]:null;
    const valid=/^#[0-9a-f]{6}$/i.test(raw||'');
    avatar.classList.toggle('role-avatar',valid);
    if(valid)avatar.style.setProperty('--avatar-role-color',raw);else avatar.style.removeProperty('--avatar-role-color');
  }
  function audioIcon(kind,label){
    const mark=document.createElement('span');mark.className='voice-state-icon voice-state-'+kind;mark.setAttribute('role','img');mark.setAttribute('aria-label',label);mark.title=label;
    const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');
    const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d',kind==='muted'?'M9 9v3a3 3 0 0 0 5 2M15 9V5a3 3 0 0 0-6 0M5 10v2a7 7 0 0 0 12 5M19 10v2M12 19v3M8 22h8M3 3l18 18':'M4 13v-1a8 8 0 0 1 13-6M20 12v5a3 3 0 0 1-3 3h-1v-7h4M4 13h4v7H7a3 3 0 0 1-3-3v-4M3 3l18 18');svg.append(path);mark.append(svg);return mark;
  }
  function audioBadge(row,person){
    const live=person?.voice===true&&person.online!==false;
    const deafened=live&&person.deafened===true,muted=live&&(person.muted===true||deafened);
    let badge=row.querySelector(':scope > .voice-state-badges');
    if(!badge&&(muted||deafened)){badge=document.createElement('span');badge.className='voice-state-badges';badge.append(audioIcon('muted','Microphone muted'),audioIcon('deafened','Deafened'));row.append(badge);}
    if(badge){badge.hidden=!muted&&!deafened;badge.children[0].hidden=!muted;badge.children[1].hidden=!deafened;}
    if(muted||deafened)row.setAttribute('aria-description',[muted?'Microphone muted':null,deafened?'Deafened':null].filter(Boolean).join('. '));else row.removeAttribute('aria-description');
    row.dataset.voiceMuted=String(muted);row.dataset.voiceDeafened=String(deafened);
  }
  function voiceActivity(row,person){
    const speaking=person?.online!==false&&person?.voice===true&&person?.speaking===true&&person?.muted!==true&&person?.deafened!==true;
    row.dataset.voiceSpeaking=String(speaking);
    const dot=row.querySelector('.speaking-dot');
    if(dot&&!dot.firstElementChild){const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d','M11 5 6 9H3v6h3l5 4V5Zm4 3a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14');svg.append(path);dot.append(svg);}
    if(dot){dot.hidden=!speaking;dot.setAttribute('aria-hidden',String(!speaking));dot.classList.toggle('speaking',speaking);dot.setAttribute('aria-label',speaking?'Speaking':'Silent');}
    let ping=row.querySelector(':scope > .voice-peer-ping');
    const measured=person?.id!==state?.id&&person?.online!==false&&person?.voice===true&&Number.isFinite(person?.latency_ms)&&person.latency_ms>0;
    if(measured&&!ping){ping=document.createElement('span');ping.className='voice-peer-ping';row.append(ping);}
    if(ping){ping.hidden=!measured;if(measured){const value=Math.round(person.latency_ms);ping.textContent=value+' ms';ping.title='Connection round-trip time: '+value+' milliseconds';ping.setAttribute('aria-label',ping.title);}}
  }
  function syncMembers(){
    for(const row of document.querySelectorAll('.member,.voice-person,.channel-person,.voice-channel-person')){
      const id=row.dataset.peer||row.querySelector('[data-avatar-peer]')?.dataset.avatarPeer;
      const room=row.closest('#voice-roster')?activeVoice()?.room?.id:selectedRoom;
      const person=id&&room?member(id,room):null;
      paintAvatar(row.querySelector('.avatar'),person,room);
      if(row.classList.contains('member')){roleBadge(row.querySelector('.member-name'),person,room);voiceActivity(row,person);}
      else {audioBadge(row,person);voiceActivity(row,person);}
    }
    const card=document.getElementById('peer-card'),avatar=document.getElementById('peer-avatar');
    if(card){let line=document.getElementById('peer-room-role');if(!line){line=document.createElement('p');line.id='peer-room-role';document.getElementById('peer-name').after(line);}
      const room=inspectedPeer?.room||selectedRoom,person=avatar.dataset.avatarPeer?member(avatar.dataset.avatarPeer,room):null;
      paintAvatar(avatar,card.hidden?null:person,room);roleBadge(line,card.hidden?null:person,room);line.hidden=!line.firstElementChild;
    }
  }
  function sync(){
    if(!unlocked)return;
    for(const avatar of document.querySelectorAll('[data-avatar-peer]')){
      const id=avatar.dataset.avatarPeer;if(!id)continue;
      const self=id===state?.id,row=avatar.closest('.member,.channel-person,.voice-person,.voice-channel-person'),room=row?.closest('#voice-roster')?activeVoice()?.room?.id:selectedRoom;
      const scoped=row&&room?member(id,room):null;
      const person=row?(scoped?{...scoped,online:self?true:!!scope(room).snapshot?.peers?.some(peer=>peer.id===id&&peer.online!==false)}:{online:false}):peerDetails(id,selectedRoom);
      const value=self?(settings?.availability||'available'):person.online?(person.availability==='invisible'?'offline':person.availability||'available'):'offline';
      const status=labels[value]?value:'available';
      let dot=avatar.querySelector('.presence-dot');
      if(!dot){dot=document.createElement('span');dot.className='presence-dot';dot.setAttribute('role','img');avatar.append(dot);}
      avatar.classList.add('status-avatar');dot.dataset.status=status;dot.title=labels[status];dot.setAttribute('aria-label',labels[status]);
      const friendRow=avatar.closest('.friend-row');if(friendRow?.querySelector('.friend-presence'))friendRow.querySelector('.friend-presence').textContent=labels[status];
    }
    const profile=document.getElementById('peer-presence');if(profile&&inspectedPeer){const person=peerDetails(inspectedPeer.id,inspectedRoom);profile.dataset.status=person.online&&person.availability!=='invisible'?person.availability||'available':'offline';}
    syncMembers();
    for(const message of document.querySelectorAll('.message[data-peer][data-room]'))paintAvatar(message.querySelector('.avatar'),member(message.dataset.peer,message.dataset.room),message.dataset.room);
  }
  function reset(){for(const avatar of document.querySelectorAll('.role-avatar'))paintAvatar(avatar,null,null);for(const node of document.querySelectorAll('[data-avatar-peer]')){node.querySelector('.presence-dot')?.remove();node.classList.remove('status-avatar');delete node.dataset.avatarHash;delete node.dataset.avatarName;delete node.dataset.avatarPeer;}document.getElementById('peer-presence')?.removeAttribute('data-status');for(const badge of document.querySelectorAll('.member-role,.voice-state-badges,.voice-peer-ping'))badge.remove();document.getElementById('peer-room-role')?.remove();for(const row of document.querySelectorAll('[data-voice-muted]')){delete row.dataset.voiceMuted;delete row.dataset.voiceDeafened;delete row.dataset.voiceSpeaking;row.removeAttribute('aria-description');}}
  window.OchatPresence={sync,reset,decorateMessage:row=>paintAvatar(row.querySelector('.avatar'),member(row.dataset.peer,row.dataset.room),row.dataset.room)};
})();
