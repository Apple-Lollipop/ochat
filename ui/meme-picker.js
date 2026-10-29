/* Public media search; images are sent over encrypted P2P, never as draft links. */
(() => {
 let api,dialog,button,query,grid,status,origin,page=1,sequence=0,timer,mode='latest',favorites=[],savedImages=[];
 const $=id=>document.getElementById(id),cache=new Map();let cacheBytes=0;
 const node=(tag,cls,text)=>{const n=document.createElement(tag);n.className=cls||'';if(text!==undefined)n.textContent=text;return n;};
 const control=(text,fn,cls='secondary')=>{const n=node('button',cls,text);n.type='button';n.onclick=fn;return n;};
 const valid=g=>api.context().unlocked&&api.context().generation===g;
 async function load(){
  if(!dialog.open||!api.context().unlocked)return;
  const request=++sequence,generation=api.context().generation,text=query.value.trim();grid.replaceChildren();grid.setAttribute('aria-busy','true');status.textContent='Loading…';$('risibank-next').disabled=$('risibank-previous').disabled=true;
  try{
   const library=await api.invoke('media_library');window.OchatPersonalization?.setLibrary(library);favorites=library?.favorites||[];savedImages=library?.saved_images||[];
   const result=mode==='favorites'?{medias:[...savedImages.map(image=>({...image,id:image.hash,slug:image.name,saved:true})),...favorites].filter(m=>m.slug.toLowerCase().includes(text.toLowerCase())),has_more:false}:await api.invoke('risibank_browse',{query:text,page});
   if(!valid(generation)||request!==sequence||!dialog.open)return;
   const medias=result.medias||[];grid.setAttribute('aria-busy','false');status.textContent=medias.length?(mode==='favorites'?'Favorites':text?'Search results':'Latest memes')+' · '+medias.length+' images':mode==='favorites'?'No favorites yet. Save a meme with its star.':'No matching memes.';
   $('risibank-next').disabled=!result.has_more;$('risibank-previous').disabled=mode==='favorites'||page<=1;
   const queue=[];
   for(const media of medias){
    const tile=node('div','meme-tile'),kind=node('span','meme-kind',/\.gif(?:[?#]|$)/i.test(media.cache_url||'')?'GIF':'Photo');tile.append(kind);let sending=false;
    const item=control('',async()=>{
     const c=api.context();if(sending)return;if(!valid(generation)||!origin||c.room!==origin.room||c.channel!==origin.channel){api.toast('Return to the conversation where you opened the picker.');return;}
     sending=true;item.disabled=true;status.textContent='Sending image…';
     try{await api.invoke(media.saved?'send_saved_image':'send_risibank_image',media.saved?{room:origin.room,channel:origin.channel,replyTo:origin.replyTo,hash:media.hash}:{room:origin.room,channel:origin.channel,replyTo:origin.replyTo,url:media.cache_url});if(valid(generation)&&request===sequence){dialog.close();$('message').focus({preventScroll:true});api.toast('Image sent.');}}
     catch(error){if(valid(generation)&&request===sequence){status.textContent=String(error);item.disabled=false;}}finally{sending=false;}
    },'risibank-item');item.setAttribute('aria-label','Send '+media.slug.replaceAll('-',' '));
    const image=node('img');image.alt=media.slug.replaceAll('-',' ');image.loading='lazy';image.decoding='async';const loader=node('span','meme-loading','Loading preview');loader.setAttribute('role','status');item.classList.add('is-loading');item.setAttribute('aria-busy','true');
    function settlePreview(failed=false){loader.remove();item.classList.remove('is-loading');item.setAttribute('aria-busy','false');if(failed){image.hidden=true;const unavailable=node('span','meme-unavailable','Preview unavailable');item.prepend(unavailable);item.title='Preview unavailable. Select to retry sending.';}}
    let animated=false;
    const animate=async()=>{if(animated||!dialog.open||!valid(generation))return;if(kind.textContent!=='GIF'&&!media.saved)return;animated=true;try{const data=await api.invoke(media.saved?'saved_image':'risibank_image',media.saved?{hash:media.hash,thumbnail:false}:{url:media.cache_url,animated:true});if(valid(generation)&&request===sequence&&dialog.open&&/^data:image\/gif;/.test(data)){kind.textContent='GIF';image.src=data;}}catch{animated=false;}};
    item.addEventListener('pointerenter',animate);item.addEventListener('focus',animate);
    image.onload=()=>settlePreview();image.onerror=()=>settlePreview(true);item.append(image,loader,node('span','meme-caption',image.alt));
    const star=control('☆',async()=>{const active=media.saved?false:!favorites.some(m=>m.id===media.id);star.disabled=true;try{const library=await api.invoke(media.saved?'favorite_chat_image':'favorite_risibank',media.saved?{room:origin.room,channel:origin.channel,hash:media.hash,active:false}:{media,active});if(!valid(generation)||request!==sequence)return;favorites=library.favorites||[];savedImages=library.saved_images||[];window.OchatPersonalization?.setLibrary(library);paint();if(mode==='favorites'&&!active)load();}catch(error){if(valid(generation))api.toast(error);}finally{if(valid(generation))star.disabled=false;}},'meme-favorite');
    function paint(){const active=media.saved?savedImages.some(m=>m.hash===media.hash):favorites.some(m=>m.id===media.id);star.textContent=active?'★':'☆';star.setAttribute('aria-pressed',String(active));star.setAttribute('aria-label',active?'Remove from favorites':'Save to favorites');}paint();tile.append(item,star);grid.append(tile);
    queue.push(async()=>{if(!valid(generation)||request!==sequence)return;try{const key=media.saved?'saved:'+media.hash:media.cache_url;let data=cache.get(key);if(!data){data=await api.invoke(media.saved?'saved_image':'risibank_image',media.saved?{hash:media.hash,thumbnail:true}:{url:media.cache_url});if(!valid(generation)||request!==sequence)return;if(typeof data!=='string'||!data.startsWith('data:image/'))throw Error('Invalid preview');while(cacheBytes+data.length>16*1024*1024&&cache.size){const key=cache.keys().next().value;cacheBytes-=cache.get(key).length;cache.delete(key);}cache.set(key,data);cacheBytes+=data.length;}if(valid(generation)&&request===sequence){image.src=data;if(/^data:image\/gif;/.test(data))kind.textContent='GIF';}}catch{if(valid(generation)&&request===sequence)settlePreview(true);}});
   }
   let index=0;await Promise.all(Array.from({length:Math.min(3,queue.length)},async()=>{while(index<queue.length)await queue[index++]();}));
  }catch(error){if(valid(generation)&&request===sequence){grid.setAttribute('aria-busy','false');status.textContent=String(error);}}
 }
 function init(callbacks){
  api=callbacks;button=control('GIF',()=>{if(dialog.open){dialog.close();return;}window.OchatEmojiPopover?.close(false);const c=api.context();if(!c.unlocked||!c.room)return;origin={room:c.room,channel:c.channel,replyTo:c.replyTo || null};mode='latest';query.value='';page=1;for(const t of dialog.querySelectorAll('[data-meme-mode]'))t.setAttribute('aria-selected',String(t.dataset.memeMode===mode));dialog.show();button.setAttribute('aria-expanded','true');place();query.focus();load();},'icon-button gif-button');button.id='gif-button';button.setAttribute('aria-expanded','false');button.setAttribute('aria-controls','risibank-dialog');button.title='GIFs and memes';button.setAttribute('aria-label',button.title);button.disabled=true;$('emoji-button').after(button);
  dialog=node('dialog','risibank-dialog');dialog.id='risibank-dialog';dialog.setAttribute('aria-labelledby','risibank-title');const header=node('div','panel-heading'),title=node('h2','','GIFs & memes');title.id='risibank-title';header.append(title,control('×',()=>dialog.close(),'icon-button'));header.lastChild.setAttribute('aria-label','Close GIF picker');
  const tabs=node('div','meme-tabs');tabs.setAttribute('role','tablist');for(const [key,text]of [['latest','Risibank'],['favorites','Favorites']]){const tab=control(text,()=>{mode=key;page=1;for(const t of tabs.children)t.setAttribute('aria-selected',String(t===tab));load();});tab.dataset.memeMode=key;tab.setAttribute('role','tab');tabs.append(tab);}
  const label=node('label','','Search memes');label.htmlFor='risibank-search';query=node('input');query.id='risibank-search';query.type='search';query.maxLength=120;query.placeholder='Search Risibank';query.oninput=()=>{clearTimeout(timer);++sequence;timer=setTimeout(()=>{page=1;load();},350);};query.onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();clearTimeout(timer);page=1;load();}};
  status=node('p','field-hint');status.id='risibank-results-status';status.setAttribute('role','status');grid=node('div','risibank-grid');grid.id='risibank-grid';const actions=node('div','dialog-actions'),previous=control('Previous',()=>{if(page>1){page--;load();}}),next=control('Next',()=>{page++;load();});previous.id='risibank-previous';next.id='risibank-next';actions.append(previous,next);
  query.title='Select a meme to send immediately. Your draft stays unchanged. Previews contact Risibank; favorites stay encrypted on this device.';dialog.append(header,tabs,label,query,status,grid,actions);document.body.append(dialog);
  dialog.addEventListener('close',()=>{button.setAttribute('aria-expanded','false');++sequence;clearTimeout(timer);grid.replaceChildren();query.value='';origin=null;status.textContent='';if(api.context().unlocked)button.focus({preventScroll:true});});
 }
 function place(){if(!dialog?.open)return;const anchor=button.getBoundingClientRect(),rect=dialog.getBoundingClientRect();dialog.style.left=Math.max(12,Math.min(innerWidth-rect.width-12,anchor.right-rect.width))+'px';dialog.style.top=Math.max(12,Math.min(innerHeight-rect.height-12,anchor.top-rect.height-8))+'px';}
 document.addEventListener('pointerdown',event=>{if(dialog?.open&&!dialog.contains(event.target)&&!button.contains(event.target))dialog.close();},true);
 document.addEventListener('keydown',event=>{if(event.key==='Escape'&&dialog?.open){event.preventDefault();dialog.close();}},true);
 window.addEventListener('resize',place);
 function reset(){++sequence;clearTimeout(timer);cache.clear();cacheBytes=0;favorites=[];savedImages=[];dialog?.close();if(button)button.disabled=true;}
 window.OchatGifPicker={init,reset,sync(){if(button)button.disabled=!api.context().unlocked||$('emoji-button').disabled;}};
})();
