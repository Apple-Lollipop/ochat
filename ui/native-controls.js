/* Presentation-only grouping; original controls, handlers and IDs stay intact. */
(() => {
 const settings=document.getElementById('settings-dialog');
 settings.classList.add('native-preferences');
 const title=settings.querySelector('h2');
 title.id='settings-title';settings.setAttribute('aria-labelledby',title.id);
 const description=document.createElement('p');description.className='preferences-subtitle';description.textContent='Your preferences stay with your encrypted account.';title.after(description);
 function group(title,children){const card=document.createElement('div');card.className='preference-group';const heading=document.createElement('h3');heading.textContent=title;card.append(heading,...children.filter(Boolean));return card;}
 function reorderAudio(){
  const panel=settings.querySelector('[data-settings-panel="audio"]'),original=[...panel.children],shortcuts=original.find(n=>n.tagName==='H3'&&n.textContent==='Keyboard shortcuts'),divider=original.find(n=>n.classList.contains('settings-divider'));
  const between=(first,last)=>original.slice(original.indexOf(first),original.indexOf(last));
  const node=id=>document.getElementById(id),inputLabel=panel.querySelector('label[for="input-device"]'),noise=node('noise-suppression').closest('label'),processing=panel.querySelector('.audio-processing-options');
  const devices=group('Devices and microphone test',[...between(inputLabel,noise),panel.querySelector('label[for="microphone-level"]'),node('microphone-level'),node('microphone-detection'),node('test-microphone'),node('stop-test'),node('test-description')]);devices.id='audio-device-preferences';
  const voice=group('Voice processing',[processing,noise,node('echo-cancellation').closest('label'),node('automatic-gain').closest('label')]);
  const transmission=group('Transmission',between(original[1],shortcuts));
  const advanced=document.createElement('details');advanced.className='audio-shortcuts preference-group';advanced.id='audio-shortcuts';const summary=document.createElement('summary');summary.textContent='Keyboard shortcuts';advanced.append(summary,...between(shortcuts,divider).filter(n=>n!==shortcuts));
  const groups=[devices,voice,transmission,advanced];
  // Keep future controls accessible even if the audio panel gains another section.
  for(const child of original)if(child!==original[0]&&child!==shortcuts&&child!==divider&&!groups.some(g=>g.contains(child)))advanced.append(child);
  panel.replaceChildren(...groups);
 }
 reorderAudio();
 for(const panel of settings.querySelectorAll('[data-settings-panel]')){
  if(panel.dataset.settingsPanel==='audio')continue;
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
