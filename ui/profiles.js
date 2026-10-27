/* Profile presentation. Reuses the existing actions and the encrypted bio command. */
(() => {
  const $=id=>document.getElementById(id);
  const node=(tag,cls,text)=>{const value=document.createElement(tag);value.className=cls||'';if(text!==undefined)value.textContent=text;return value;};
  let api,dialog,home,anchor,returnFocus,closing=false,bioCursor=[0,0];
  function position(){
    if(!dialog?.open)return;
    const gap=12,topEdge=Math.max(12,(document.querySelector('.app-header')?.getBoundingClientRect().bottom||30)+8),width=Math.min(344,innerWidth-24);dialog.style.width=width+'px';dialog.style.maxHeight=(innerHeight-topEdge-gap)+'px';
    const height=dialog.getBoundingClientRect().height,box=anchor?.isConnected?anchor.getBoundingClientRect():null;
    let left=(innerWidth-width)/2,top=(innerHeight-height)/2;
    if(box&&box.width){left=box.right+gap+width<=innerWidth-gap?box.right+gap:box.left-width-gap;top=box.top-64;}
    dialog.style.left=Math.max(gap,Math.min(left,innerWidth-width-gap))+'px';
    dialog.style.top=Math.max(topEdge,Math.min(top,innerHeight-height-gap))+'px';
  }
  function close(restore=true){
    if(!dialog?.open&&!dialog?.contains($('peer-card')))return;
    closing=true;if(dialog.open)dialog.close();home.after($('peer-card'));anchor?.classList.remove('profile-selected');
    const opener=returnFocus;anchor=null;returnFocus=null;api.closed();closing=false;
    if(restore&&opener?.isConnected&&!opener.closest('[hidden]'))opener.focus({preventScroll:true});
  }
  function open(origin){
    anchor?.classList.remove('profile-selected');anchor=origin?.closest?.('.member,.channel-person,.message')||origin;returnFocus=origin;
    anchor?.classList.add('profile-selected');dialog.append($('peer-card'));
    const facts=$('peer-card').querySelector('.peer-facts');
    if(facts)facts.classList.add('profile-connection-facts');
    $('profile-extra-details').open=false;if(!dialog.open)dialog.showModal();dialog.scrollTop=0;position();$('floating-profile-close').focus({preventScroll:true});
  }
  function groups(){
    const list=$('members');for(const heading of list.querySelectorAll('.member-group-title'))heading.remove();
    for(const [offline,label]of [[false,'Online'],[true,'Offline']]){const rows=[...list.querySelectorAll('.member')].filter(row=>row.classList.contains('offline')===offline);if(rows.length){const heading=node('h4','member-group-title',label+' — '+rows.length);rows[0].before(heading);}}
  }
  function bioChanged(){
    const input=$('profile-bio-input'),preview=$('bio-preview-text');preview.textContent=input.value||'Your bio will appear here.';preview.classList.toggle('empty',!input.value);
    $('bio-save-status').textContent='';window.OchatEmoji?.render(preview);
  }
  function setupBio(){
    const field=$('profile-bio-input').closest('fieldset'),input=$('profile-bio-input'),save=field.querySelector('button');save.id='save-bio';
    field.classList.add('bio-editor');field.querySelector('legend').textContent='Your profile';
    const intro=node('p','field-hint','Introduce yourself with words and emojis. Choose Save bio to share your changes with connected peers.');
    const preview=node('div','bio-preview'),banner=node('div','bio-preview-banner'),identity=node('div','bio-preview-identity'),avatar=node('span','avatar'),name=node('strong');avatar.id='bio-preview-avatar';name.id='bio-preview-name';identity.append(avatar,name);const text=node('p');text.id='bio-preview-text';preview.append(banner,identity,text);
    const controls=document.querySelector('.profile-photo-controls'),photoHint=controls.nextElementSibling;field.querySelector('legend').after(intro,preview,controls,photoHint);
    const tools=node('div','bio-tools'),choose=node('button','secondary','Add emoji');choose.id='bio-emoji-button';choose.type='button';choose.setAttribute('aria-expanded','false');choose.setAttribute('aria-controls','bio-emoji-picker');tools.append(choose,$('profile-bio-count'));input.closest('label').after(tools);
    const picker=node('section','bio-emoji-picker');picker.id='bio-emoji-picker';picker.hidden=true;picker.setAttribute('aria-label','Choose an emoji for your bio');
    const head=node('div','bio-picker-heading'),title=node('strong','','Choose emoji'),done=node('button','text-button','Close');done.type='button';done.onclick=()=>hideBioPicker(true);head.append(title,done);const grid=node('div','emoji-grid');grid.id='bio-emoji-grid';picker.append(head,grid);tools.after(picker);
    choose.onclick=()=>{if(!picker.hidden){hideBioPicker(true);return;}bioCursor=[input.selectionStart,input.selectionEnd];grid.replaceChildren();
      for(const value of api.emojis()){const button=node('button','unicode-emoji',value);button.type='button';button.setAttribute('aria-label',value);button.onclick=()=>{const next=input.value.slice(0,bioCursor[0])+value+input.value.slice(bioCursor[1]);if([...next].length>300){api.toast('Your bio can contain up to 300 characters.');return;}input.setRangeText(value,...bioCursor,'end');input.dispatchEvent(new Event('input',{bubbles:true}));api.remember(value);hideBioPicker(false);input.focus({preventScroll:true});};grid.append(button);}
      picker.hidden=false;choose.setAttribute('aria-expanded','true');window.OchatEmoji?.render(picker);grid.querySelector('button')?.focus({preventScroll:true});picker.scrollIntoView?.({block:'nearest'});
    };
    picker.addEventListener('keydown',event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();hideBioPicker(true);}});
    const status=node('span','bio-save-status');status.id='bio-save-status';status.setAttribute('role','status');const actions=node('div','bio-actions');actions.append(status,save);field.append(actions);
    input.addEventListener('input',bioChanged);
    $('settings-dialog').addEventListener('close',()=>hideBioPicker(false));
    const account=document.querySelector('[data-settings-panel="account"]');account.prepend(field);account.querySelector('h3').textContent='Account & backup';
  }
  function hideBioPicker(focus){const picker=$('bio-emoji-picker');if(!picker)return;picker.hidden=true;$('bio-emoji-button').setAttribute('aria-expanded','false');if(focus)$('bio-emoji-button').focus({preventScroll:true});}
  function settings(state){hideBioPicker(false);$('bio-preview-name').textContent=state?.name||'Your profile';api.avatar($('bio-preview-avatar'),state||{});bioChanged();}
  function reset(){close(false);hideBioPicker(false);for(const id of ['bio-preview-name','bio-preview-text','bio-preview-avatar','bio-save-status'])$(id).replaceChildren();for(const key of Object.keys($('bio-preview-avatar').dataset))delete $('bio-preview-avatar').dataset[key];$('bio-emoji-grid').replaceChildren();}
  function init(callbacks){
    document.fonts?.load?.('16px "Ochat Twemoji"','😀')?.catch(()=>{});
    api=callbacks;home=document.createComment('Profile card home');$('peer-card').before(home);dialog=node('dialog','floating-profile');dialog.id='floating-profile';dialog.setAttribute('aria-labelledby','peer-name');
    const banner=node('div','profile-banner'),closeButton=node('button','icon-button','×');closeButton.id='floating-profile-close';closeButton.type='button';closeButton.setAttribute('aria-label','Close profile');closeButton.onclick=()=>close();banner.append(closeButton);dialog.append(banner);document.body.append(dialog);
    dialog.addEventListener('cancel',event=>{event.preventDefault();close();});dialog.addEventListener('click',event=>{if(event.target===dialog){const rect=dialog.getBoundingClientRect();if(event.clientX<rect.left||event.clientX>rect.right||event.clientY<rect.top||event.clientY>rect.bottom){close();const next=document.elementFromPoint?.(event.clientX,event.clientY)?.closest('.member-profile');if(next){next.focus({preventScroll:true});next.click();}}}});
    dialog.addEventListener('close',()=>{if(!closing&&!dialog.open&&dialog.contains($('peer-card')))close();});
    $('peer-card').querySelector('.peer-actions').before($('peer-shared-rooms'));
    const extra=node('details','profile-extra-details'),summary=node('summary','','Identity & security');extra.id='profile-extra-details';extra.append(summary,$('verify-peer-button'),$('peer-verification-status'),$('peer-card').querySelector('.identity-details'));extra.querySelector('.identity-details').open=true;$('peer-card').append(extra);
    // Expanding identity details or loading a profile can grow the anchored card.
    // Keep its scroll container inside the viewport, including its bottom edge.
    extra.addEventListener('toggle',position);
    if(window.ResizeObserver)new ResizeObserver(position).observe(dialog);
    window.addEventListener('resize',position);setupBio();
  }
  window.OchatProfiles={init,open,close,groups,settings,bioChanged,reset,isOpen:()=>!!dialog?.open};
})();
