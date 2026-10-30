/* Synthetic IPC only. WebKit layout coverage does not replace testing WKWebView on a Mac. */
const {fixture,server}=require('./visual.cjs');
const {webkit}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await webkit.launch({headless:true});
 try{
  for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
   const page=await browser.newPage({viewport:{width,height:800},reducedMotion:'reduce'});
   await page.addInitScript(fixture,theme);
   await page.addInitScript(()=>Object.defineProperty(navigator,'platform',{value:'MacIntel'}));
   await page.goto('http://127.0.0.1:'+server.address().port);
   await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
   assert.ok((await page.locator('#quick-switcher').innerText()).includes('⌘K'));
   assert.equal(await page.locator('#rich-text-toolbar button').first().getAttribute('title'),'Bold (⌘B)');
   await page.keyboard.press('Meta+k');await page.locator('#command-palette input').fill('Maya');await page.keyboard.press('Enter');
   await page.waitForFunction(()=>conversationContext(selectedRoom).direct);
   await page.evaluate(()=>{
    for(const [id,text] of [['long','ochat3:'+'aB19'.repeat(2500)],['code','```\n'+'long_code_'.repeat(1000)+'\n```'],['inline','`'+'inline'.repeat(1500)+'`'],['spoiler','||'+'secret'.repeat(1500)+'||']])
     addMessage({type:'message',id,room:selectedRoom,peer:state.id,name:'You',text,timestamp:Date.now(),emojis:[]});
   });
   const sizes=await page.evaluate(()=>({viewport:innerWidth,page:document.documentElement.scrollWidth,timeline:document.getElementById('timeline').clientWidth,content:document.getElementById('timeline').scrollWidth}));
   assert.ok(sizes.page<=sizes.viewport+1,JSON.stringify({theme,width,...sizes}));
   assert.ok(sizes.content<=sizes.timeline+1,JSON.stringify({theme,width,...sizes}));
   // Older WebKit lacks overflow-wrap:anywhere. Its fallback must still contain identifiers.
   await page.addStyleTag({content:'.bubble{overflow-wrap:break-word!important}'});
   assert.ok(await page.evaluate(()=>{const n=document.getElementById('timeline');return n.scrollWidth<=n.clientWidth+1;}));
   await page.screenshot({path:`releases/reports/macos-webkit-${theme}-${width}.png`});
   // Exercise the actual nested media layout with a synthetic native window, no capture.
   const fullscreen=await page.evaluate(async()=>{
    let full=false;window.__TAURI__.window={...window.__TAURI__.window,getCurrentWindow:()=>({isFullscreen:async()=>full,setFullscreen:async value=>{full=value;}})};
    const grid=document.getElementById('visual-grid'),stage=document.getElementById('visual-stage');stage.hidden=false;grid.hidden=false;
    const card=document.createElement('figure');card.className='visual-card';card.innerHTML='<div class="visual-surface"></div><button type="button">Video controls</button>';grid.append(card);
    await window.OchatFullscreen.toggle(card);const rect=card.getBoundingClientRect(),close=document.querySelector('.fullscreen-close');
    close.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));const trapped=document.activeElement===card.querySelector('button');
    await window.OchatFullscreen.exit();card.remove();stage.hidden=true;
    return {x:rect.x,y:rect.y,width:rect.width,height:rect.height,restored:!full,trapped};
   });
   assert.deepEqual(fullscreen,{x:0,y:0,width,height:800,restored:true,trapped:true});
   await page.close();
  }
  console.log('WebKit: 8 light/dark layouts passed: long messages, code/spoilers, legacy wrapping, Command+K, Mac labels and native video expansion.');
 }finally{await browser.close();server.close();}
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
