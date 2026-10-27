/* Keep reveal controls independent from WebView2's one-shot native eye. */
(() => {
  const fields=[];
  function mask(item){item.input.type='password';item.button.textContent='Show';item.button.setAttribute('aria-pressed','false');item.button.setAttribute('aria-label','Show '+item.label);}
  for(const input of document.querySelectorAll('input[type="password"]')){
    const label=input.labels?.[0]?.textContent.trim()||'password',wrapper=document.createElement('div'),button=document.createElement('button');
    input.dataset.privatePassword='';wrapper.className='password-field';input.before(wrapper);wrapper.append(input,button);
    button.type='button';button.className='password-visibility';button.setAttribute('aria-controls',input.id);
    const item={input,button,label};fields.push(item);mask(item);
    button.addEventListener('pointerdown',event=>{if(event.button===0)event.preventDefault();});
    button.onclick=()=>{const start=input.selectionStart,end=input.selectionEnd;if(input.type==='password'){input.type='text';button.textContent='Hide';button.setAttribute('aria-pressed','true');button.setAttribute('aria-label','Hide '+label);}else mask(item);input.focus({preventScroll:true});if(start!==null&&end!==null)input.setSelectionRange(start,end);};
    wrapper.addEventListener('focusout',event=>{if(!wrapper.contains(event.relatedTarget))mask(item);});
  }
  window.OchatPasswords={reset(){for(const item of fields){item.input.value='';mask(item);}}};
})();
