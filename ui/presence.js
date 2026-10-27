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
    return snapshot?.peers?.find(peer=>peer.id===id)||snapshot?.members?.find(peer=>peer.id===id)||(roomMembers.get(room)||[]).find(peer=>peer.id===id);
  }
  function roleBadge(parent,person,room){
    let badge=parent?.querySelector(':scope > .member-role');if(!parent)return;
    const context=scope(room).room;
    const label=context&&context.capacity!==2&&Object.prototype.hasOwnProperty.call(roles,person?.role)?roles[person.role]:null;
    if(!label){badge?.remove();return;}
    if(!badge){badge=document.createElement('span');badge.className='member-role';parent.append(badge);}
    badge.textContent=label;badge.dataset.role=person.role;badge.setAttribute('aria-label','Room role: '+label);
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
  function syncMembers(){
    for(const row of document.querySelectorAll('.member,.voice-person,.channel-person,.voice-channel-person')){
      const id=row.dataset.peer||row.querySelector('[data-avatar-peer]')?.dataset.avatarPeer;
      const room=row.closest('#voice-roster')?activeVoice()?.room?.id:selectedRoom;
      const person=id&&room?member(id,room):null;
      if(row.classList.contains('member'))roleBadge(row.querySelector('.member-name'),person,room);
      else audioBadge(row,person);
    }
    const card=document.getElementById('peer-card'),avatar=document.getElementById('peer-avatar');
    if(card){let line=document.getElementById('peer-room-role');if(!line){line=document.createElement('p');line.id='peer-room-role';document.getElementById('peer-name').after(line);}
      const room=inspectedPeer?.room||selectedRoom,person=avatar.dataset.avatarPeer?member(avatar.dataset.avatarPeer,room):null;
      roleBadge(line,card.hidden?null:person,room);line.hidden=!line.firstElementChild;
    }
  }
  function sync(){
    if(!unlocked)return;
    for(const avatar of document.querySelectorAll('[data-avatar-peer]')){
      const id=avatar.dataset.avatarPeer;if(!id)continue;
      const self=id===state?.id,person=peerDetails(id,selectedRoom);
      const value=self?(settings?.availability||'available'):person.online?(person.availability==='invisible'?'offline':person.availability||'available'):'offline';
      const status=labels[value]?value:'available';
      let dot=avatar.querySelector('.presence-dot');
      if(!dot){dot=document.createElement('span');dot.className='presence-dot';dot.setAttribute('role','img');avatar.append(dot);}
      avatar.classList.add('status-avatar');dot.dataset.status=status;dot.title=labels[status];dot.setAttribute('aria-label',labels[status]);
      const row=avatar.closest('.friend-row');if(row?.querySelector('.friend-presence'))row.querySelector('.friend-presence').textContent=labels[status];
    }
    const profile=document.getElementById('peer-presence');if(profile&&inspectedPeer){const person=peerDetails(inspectedPeer.id,inspectedRoom);profile.dataset.status=person.online?person.availability||'available':'offline';}
    syncMembers();
  }
  function reset(){for(const node of document.querySelectorAll('[data-avatar-peer]')){node.querySelector('.presence-dot')?.remove();node.classList.remove('status-avatar');delete node.dataset.avatarHash;delete node.dataset.avatarName;delete node.dataset.avatarPeer;}document.getElementById('peer-presence')?.removeAttribute('data-status');for(const badge of document.querySelectorAll('.member-role,.voice-state-badges'))badge.remove();document.getElementById('peer-room-role')?.remove();for(const row of document.querySelectorAll('[data-voice-muted]')){delete row.dataset.voiceMuted;delete row.dataset.voiceDeafened;row.removeAttribute('aria-description');}}
  window.OchatPresence={sync,reset};
})();
