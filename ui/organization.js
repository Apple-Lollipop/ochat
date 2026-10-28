/* Local organization never changes remote identities or room names. */
(() => {
  const $ = id => document.getElementById(id);
  const make = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text !== undefined) n.textContent = text; return n; };
  let target = null, generation = 0, origin = null;
  const dialog = make('dialog', 'organization-dialog');
  dialog.id = 'conversation-organization'; dialog.setAttribute('aria-labelledby', 'organization-title');
  dialog.innerHTML = `<form method="dialog"><header><h2 id="organization-title">Organize conversation</h2><button type="button" id="organization-close" class="icon-button" aria-label="Close">×</button></header><p id="organization-original" class="field-hint"></p><label>Local nickname<input id="organization-nickname" maxlength="80" placeholder="Use original name" autocomplete="off"></label><label>Folder<input id="organization-folder" maxlength="80" list="organization-folders" placeholder="No folder" autocomplete="off"></label><datalist id="organization-folders"></datalist><label class="organization-color-row">Conversation color<input id="organization-color" type="color" value="#007AFF"><input id="organization-use-color" type="checkbox"> Use color</label><p class="field-hint">Only visible to you. Included in your encrypted backup.</p><p id="organization-error" class="error" role="alert"></p><footer><button type="button" id="organization-reset" class="secondary">Reset</button><button type="button" id="organization-cancel" class="secondary">Cancel</button><button type="submit" id="organization-save" class="primary">Save</button></footer></form>`;
  document.body.append(dialog);
  const trigger = make('button', 'secondary full', 'Organize conversation…'); trigger.type = 'button'; trigger.id = 'organize-conversation';
  $('room-actions-dialog').append(trigger);
  const close = () => dialog.close();
  $('organization-close').onclick = close; $('organization-cancel').onclick = close;
  dialog.addEventListener('click', e => { if (e.target === dialog) { const r = dialog.getBoundingClientRect(); if(e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) close(); } });
  dialog.addEventListener('close', () => { target = null; for (const id of ['organization-nickname','organization-folder','organization-original','organization-error']) { const n=$(id); if('value' in n)n.value=''; else n.textContent=''; } $('organization-folders').replaceChildren(); if (origin?.isConnected)origin.focus({preventScroll:true}); origin=null; });
  function preference(id) { return conversationLayout.preferences?.[id] || {}; }
  trigger.onclick = () => {
    if(!unlocked || !selectedRoom) return;
    target=selectedRoom; generation=sessionGeneration; origin=document.activeElement;
    const room=allRooms().find(r=>r.id===target), current=preference(target);
    $('organization-original').textContent=`Original name: ${room?.name || conversationContext(target).title}`;
    $('organization-nickname').value=current.nickname || ''; $('organization-folder').value=current.folder || '';
    $('organization-use-color').checked=!!current.color; $('organization-color').value=current.color || '#007AFF';
    $('organization-folders').replaceChildren();
    for(const name of [...new Set(Object.values(conversationLayout.preferences || {}).map(p=>p.folder).filter(Boolean))].sort()) {const option=make('option');option.value=name;$('organization-folders').append(option);}
    $('room-actions-dialog').close();show(dialog.id);$('organization-nickname').focus();
  };
  $('organization-reset').onclick=()=>{$('organization-nickname').value='';$('organization-folder').value='';$('organization-use-color').checked=false;};
  dialog.querySelector('form').onsubmit = async e => {
    e.preventDefault();if(!target || !unlocked || generation!==sessionGeneration)return;
    const room=target, epoch=generation, value={nickname:$('organization-nickname').value.trim(),folder:$('organization-folder').value.trim(),color:$('organization-use-color').checked?$('organization-color').value.toUpperCase():null};
    if([value.nickname,value.folder].some(v=>[...v].length>80 || /[\x00-\x1f\x7f]/.test(v))) {$('organization-error').textContent='Use up to 80 characters without control characters.';return;}
    const preferences={...(conversationLayout.preferences || {})}; if(value.nickname || value.folder || value.color)preferences[room]=value;else delete preferences[room];
    $('organization-save').disabled=true;
    try { await saveLayout({...conversationLayout,preferences}); if(!unlocked || epoch!==sessionGeneration)return; const saved=preference(room);if(JSON.stringify(saved)!==JSON.stringify(preferences[room] || {})){$('organization-error').textContent='Could not save these changes. Please try again.';return;}render();close(); }
    finally {$('organization-save').disabled=false;}
  };
  function decorate() {
    trigger.disabled=!unlocked || !selectedRoom;
    for(const node of document.querySelectorAll('.room-item,.conversation-tab-entry')) {
      const id=node.dataset.key, value=preference(id); node.style.removeProperty('--conversation-color');node.classList.toggle('has-conversation-color',!!value.color);
      if(/^#[0-9a-f]{6}$/i.test(value.color || ''))node.style.setProperty('--conversation-color',value.color);
    }
    for(const container of [$('direct-room-list'),$('group-room-list')]) {
      container.querySelectorAll('.conversation-folder-heading').forEach(n=>n.remove());
      const rows=[...container.children].filter(n=>n.matches('.room-item'));
      const groups=new Map(); for(const row of rows){const folder=preference(row.dataset.key).folder || '';if(!groups.has(folder))groups.set(folder,[]);groups.get(folder).push(row);}
      // Folder order follows the first conversation; keyed rows and drag order remain intact.
      for(const [folder,items] of groups){if(folder){const heading=make('h4','conversation-folder-heading',folder);container.append(heading);}for(const row of items)container.append(row);}
    }
  }
  function reset() {if(dialog.open)dialog.close();target=null;origin=null;decorate();}
  window.OchatOrganization={title:(id,fallback)=>preference(id).nickname || fallback,decorate,reset};
})();
