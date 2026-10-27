/* Presentation-only grouping; original controls, handlers and IDs stay intact. */
(() => {
 const settings=document.getElementById('settings-dialog');
 settings.classList.add('native-preferences');
 const title=settings.querySelector('h2');
 title.id='settings-title';settings.setAttribute('aria-labelledby',title.id);
 const description=document.createElement('p');description.className='preferences-subtitle';description.textContent='Your preferences stay with your encrypted account.';title.after(description);
 for(const panel of settings.querySelectorAll('[data-settings-panel]')){
  const children=[...panel.children];let group=null;
  for(const child of children){
   if(child.classList.contains('settings-divider')){child.remove();group=null;continue;}
   if(child.tagName==='H3'||!group){group=document.createElement('div');group.className='preference-group';panel.append(group);}
   group.append(child);
  }
 }
 for(const id of ['room-actions-dialog','room-notifications-dialog','channel-editor-dialog'])document.getElementById(id)?.classList.add('native-preferences');
 const manage=document.getElementById('room-management');if(manage){manage.classList.add('native-preferences');manage.setAttribute('aria-label','Room roles and invitations');}
})();
