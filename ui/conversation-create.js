/* Keep the original action buttons and handlers; only their presentation moves. */
(() => {
  const toggle=document.getElementById('conversation-create-toggle');
  const menu=document.getElementById('conversation-create-menu');
  if(!toggle||!menu)return;
  const actions=()=>[...menu.querySelectorAll('button:not(:disabled)')];
  function close(restore=false){menu.hidden=true;toggle.setAttribute('aria-expanded','false');if(restore)toggle.focus({preventScroll:true});}
  toggle.addEventListener('click',()=>{if(!menu.hidden){close(true);return;}if(!actions().length)return;menu.hidden=false;toggle.setAttribute('aria-expanded','true');actions()[0]?.focus();});
  menu.addEventListener('click',e=>{if(e.target.closest('button'))close();});
  document.addEventListener('pointerdown',e=>{if(!menu.hidden&&!e.target.closest('.conversation-create'))close();});
  document.addEventListener('keydown',e=>{if(menu.hidden)return;if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();close(true);}else if(['ArrowDown','ArrowUp','Home','End'].includes(e.key)){const items=actions(),index=items.indexOf(document.activeElement);e.preventDefault();items[e.key==='Home'?0:e.key==='End'?items.length-1:(index+(e.key==='ArrowDown'?1:-1)+items.length)%items.length]?.focus();}},true);
  document.addEventListener('focusin',e=>{if(!menu.hidden&&!e.target.closest('.conversation-create'))close();});
  new MutationObserver(()=>{toggle.disabled=!actions().length;if(toggle.disabled)close();}).observe(menu,{subtree:true,attributes:true,attributeFilter:['disabled']});
  toggle.disabled=!actions().length;
  document.getElementById('navigation-dialog')?.addEventListener('close',()=>close());
  window.addEventListener('resize',()=>close());
})();
