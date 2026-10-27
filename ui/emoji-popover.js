/* A non-modal composer picker; account/room library management can stay modal. */
(() => {
 const dialog=document.getElementById('emoji-dialog'),opener=document.getElementById('emoji-button');
 function place(){if(!dialog.open||!dialog.classList.contains('emoji-popover'))return;const a=opener.getBoundingClientRect(),r=dialog.getBoundingClientRect(),width=r.width||360,height=r.height||360;dialog.style.left=Math.max(12,Math.min(innerWidth-width-12,a.right-width))+'px';dialog.style.top=Math.max(12,Math.min(innerHeight-height-12,a.top-height-10))+'px';}
 function open(){if(!dialog.open){dialog.classList.add('emoji-popover');dialog.show();}opener.setAttribute('aria-expanded','true');place();document.getElementById('message').focus({preventScroll:true});}
 dialog.addEventListener('close',()=>{const restore=dialog.contains(document.activeElement);dialog.classList.remove('emoji-popover');dialog.style.removeProperty('left');dialog.style.removeProperty('top');opener.setAttribute('aria-expanded','false');if(restore)opener.focus({preventScroll:true});});
 document.addEventListener('pointerdown',event=>{if(dialog.open&&dialog.classList.contains('emoji-popover')&&!dialog.contains(event.target)&&!opener.contains(event.target))dialog.close();},true);
 dialog.addEventListener('pointerdown',event=>{if(event.button===0&&event.target.closest('.unicode-emoji,.emoji-library-choose'))event.preventDefault();});
 document.addEventListener('keydown',event=>{if(event.key==='Escape'&&dialog.open&&dialog.classList.contains('emoji-popover')){event.preventDefault();event.stopPropagation();dialog.close();}},true);
 if(typeof ResizeObserver==='function')new ResizeObserver(place).observe(dialog);window.addEventListener('resize',place);window.OchatEmojiPopover={open,place};
})();
