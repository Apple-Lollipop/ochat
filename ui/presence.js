/* Availability and voice activity deliberately use separate indicators. */
(() => {
  const labels={available:'Available',away:'Away',busy:'Do not disturb',invisible:'Invisible',offline:'Offline'};
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
  }
  function reset(){for(const node of document.querySelectorAll('[data-avatar-peer]')){node.querySelector('.presence-dot')?.remove();node.classList.remove('status-avatar');delete node.dataset.avatarHash;delete node.dataset.avatarName;delete node.dataset.avatarPeer;}document.getElementById('peer-presence')?.removeAttribute('data-status');}
  window.OchatPresence={sync,reset};
})();
