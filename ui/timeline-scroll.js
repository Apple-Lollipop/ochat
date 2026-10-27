/* Follow late image/layout expansion only while the reader follows the latest entry. */
(() => {
  const timeline=document.getElementById('timeline');let following=true,frame=0;
  function mediaSize(){timeline.style.setProperty('--inline-media-height',Math.max(80,Math.min(320,(timeline.clientHeight||360)-100))+'px');}if(typeof ResizeObserver==='function')new ResizeObserver(()=>{mediaSize();settle();}).observe(timeline);window.addEventListener('resize',mediaSize);mediaSize();
  const nearBottom=()=>timeline.scrollHeight-timeline.scrollTop-timeline.clientHeight<120;
  function settle(){if(!following)return;cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{if(following)timeline.scrollTop=timeline.scrollHeight;});}
  const observer=typeof ResizeObserver==='function'?new ResizeObserver(settle):null;
  timeline.addEventListener('load',settle,true);
  timeline.addEventListener('scroll',()=>{if(nearBottom())following=true;});
  timeline.addEventListener('wheel',event=>{if(event.deltaY<0){following=false;cancelAnimationFrame(frame);}},{passive:true});
  timeline.addEventListener('touchmove',()=>{following=false;cancelAnimationFrame(frame);},{passive:true});
  timeline.addEventListener('pointerdown',event=>{if(event.target===timeline){following=false;cancelAnimationFrame(frame);}});
  document.addEventListener('keydown',event=>{if(event.target.closest?.('dialog[open]')&&!timeline.contains(event.target))return;if(['PageUp','Home','ArrowUp'].includes(event.key)&&!event.target.closest?.('input,textarea,select,[contenteditable="true"]')){following=false;cancelAnimationFrame(frame);}});
  window.OchatTimeline={appended(entry,bottom){if(bottom)following=true;observer?.observe(entry);settle();},pause(){following=false;cancelAnimationFrame(frame);},resume(){following=true;settle();},reset(){observer?.disconnect();cancelAnimationFrame(frame);following=true;},settle};
})();
