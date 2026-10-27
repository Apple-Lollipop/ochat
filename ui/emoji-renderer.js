/* Twemoji 17.0.3. Artwork by Twitter and contributors, CC-BY 4.0.
 * Vendored assets keep message rendering offline. Drafts remain Unicode text. */
(() => {
  const excluded='script,style,textarea,input,select,option,pre,code,svg,[contenteditable],.twemoji-wrap,[hidden],dialog:not([open])';
  let observer,scheduled=false;
  const pending=new Set();
  function render(root){
    if(!window.twemoji || !root || root.nodeType!==1 || root.closest(excluded))return;
    const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT),texts=[];
    while(walker.nextNode())if(!walker.currentNode.parentElement?.closest(excluded))texts.push(walker.currentNode);
    for(const text of texts){
      if(!/[\p{Extended_Pictographic}\p{Regional_Indicator}\u20e3]/u.test(text.data))continue;
      const fragment=document.createElement('span');fragment.textContent=text.data;
      window.twemoji.parse(fragment,{base:'vendor/twemoji/',folder:'svg',ext:'.svg',className:'twemoji'});
      if(!fragment.querySelector('img.twemoji'))continue;
      for(const image of fragment.querySelectorAll('img.twemoji')){
        image.loading=text.parentElement.closest('.unicode-emoji')?'eager':'lazy';image.decoding='async';
        const wrap=document.createElement('span'),original=document.createElement('span');
        wrap.className='twemoji-wrap twemoji-loading';original.className='twemoji-original';original.textContent=image.alt;original.setAttribute('aria-hidden','true');
        image.replaceWith(wrap);wrap.append(image,original);
        image.onload=()=>wrap.classList.remove('twemoji-loading');image.onerror=()=>{image.hidden=true;wrap.classList.remove('twemoji-loading');wrap.classList.add('twemoji-failed');original.removeAttribute('aria-hidden');};if(image.complete&&image.naturalWidth>0)wrap.classList.remove('twemoji-loading');
      }
      text.replaceWith(...fragment.childNodes);
    }
  }
  function init(){
    if(observer)return;render(document.body);
    observer=new MutationObserver(records=>{
      for(const record of records){
        if(record.type==='attributes')pending.add(record.target);
        else if(record.type==='characterData')pending.add(record.target.parentElement);
        else for(const node of record.addedNodes)pending.add(node.nodeType===1?node:node.parentElement);
      }
      if(scheduled)return;scheduled=true;queueMicrotask(()=>{scheduled=false;const roots=new Set(pending);pending.clear();for(const root of roots){if(!root?.isConnected)continue;let ancestor=root.parentElement;while(ancestor&&!roots.has(ancestor))ancestor=ancestor.parentElement;if(!ancestor)render(root);}});
    });
    observer.observe(document.body,{childList:true,subtree:true,characterData:true,attributes:true,attributeFilter:['open','hidden']});
  }
  function destroy(){observer?.disconnect();observer=null;pending.clear();scheduled=false;}
  window.addEventListener('unload',destroy);
  window.OchatEmoji={init,render,destroy};
})();
