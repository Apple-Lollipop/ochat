/* Synthetic IPC fixture: no real account, peer, microphone or system changes. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/productivity');
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));
 const browser=await chromium.launch({headless:true,channel:'msedge'});const checks=[];
 try{for(const width of [480,780,1180,1600]){
  const page=await browser.newPage({viewport:{width,height:width===480?640:900},reducedMotion:'reduce'}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,'dark');
  await page.addInitScript(()=>{const original=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=async(c,a)=>{
   if(c==='transfer_history')return [];
   if(c==='save_conversation_layout')return a.layout;
   if(c==='save_workspace'){await original('save_settings',{settings:{...settings,workspace:a.workspace}});return a.workspace;}
   if(c==='thread_history')return {entries:[],total:0};
   if(c==='audio_devices')return [{id:'mic',name:'Test microphone',input:true},{id:'out',name:'Test headphones',input:false}];
   return original(c,a);
  };});
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true'||window.__qaError);
  assert.equal(await page.evaluate(()=>window.__qaError),undefined);await page.waitForTimeout(100);
  assert.deepEqual(errors,[]);
  await page.keyboard.press('Control+k');await page.locator('#command-palette').waitFor({state:'visible'});
  await page.locator('#command-palette input').fill('trnsfrs');assert.ok(await page.locator('.command-result').count());
  await page.keyboard.press('Enter');await page.locator('#transfer-center').waitFor({state:'visible'});await page.keyboard.press('Escape');
  await page.evaluate(()=>document.getElementById('settings-button').click());await page.locator('#settings-dialog').waitFor({state:'visible'});
  await page.locator('.settings-search-tools input').fill('clock');await page.locator('.settings-search-results button').first().click();
  await page.locator('#workspace-hour_cycle').selectOption('12');await page.locator('#workspace-density').selectOption('compact');
  await page.screenshot({path:path.join(output,`${width}-settings.png`)});
  await page.locator('#settings-save').click();await page.waitForTimeout(100);
  assert.equal(await page.evaluate(()=>settings.workspace.hour_cycle),'12');
  assert.equal(await page.evaluate(()=>document.body.dataset.density),'compact');
  await page.evaluate(async()=>{const room=selectedRoom;document.getElementById('message').value='Main draft retained';await window.OchatWorkspace.openThread({room,channel:'general',id:'thread-root',peer:'b'.repeat(64),name:'Maya',text:'Root'});});
  assert.equal(await page.evaluate(()=>document.getElementById('thread-dialog').matches(':modal')),false);
  await page.locator('#thread-message').fill('Thread reply');await page.locator('#thread-composer button[type=submit]').click();await page.waitForTimeout(100);
  assert.equal(await page.locator('#message').inputValue(),'Main draft retained');
  const reply=await page.evaluate(()=>window.__qaCalls.filter(c=>c.command==='send_text').at(-1));assert.equal(reply.args.replyTo.id,'thread-root');
  await page.evaluate(()=>document.getElementById('thread-dialog').close());
  await page.evaluate(()=>{state.voice=true;render();document.querySelector('.quick-device-menu').click();});await page.locator('#quick-audio').waitFor({state:'visible'});
  await page.locator('#quick-input').selectOption('mic');await page.getByRole('button',{name:'Apply devices and levels',exact:true}).click();await page.waitForTimeout(100);assert.equal(await page.evaluate(()=>settings.audio.input_device),'mic');
  assert.equal(await page.evaluate(()=>window.__qaCalls.some(c=>c.command==='test_microphone')),false);
  assert.equal(await page.locator('#navigation-resizer').count(),0);
  await page.evaluate(()=>{document.getElementById('room-actions-toggle').click();});
  await page.locator('#archive-conversation').click();await page.waitForTimeout(100);
  assert.equal(await page.evaluate(()=>conversationLayout.preferences[selectedRoom].archived),true);
  await page.evaluate(()=>window.OchatProductivity.openPalette());await page.locator('#command-palette input').fill('Archived conversations');await page.keyboard.press('Enter');
  await page.locator('#archived-conversations').waitFor({state:'visible'});await page.getByRole('button',{name:'Restore',exact:true}).click();await page.waitForTimeout(100);await page.keyboard.press('Escape');
  assert.equal(await page.evaluate(()=>!!conversationLayout.preferences?.[selectedRoom]?.archived),false);
  await page.evaluate(()=>window.OchatProductivity.dock('search-dialog'));await page.locator('#search-dialog').waitFor({state:'visible'});
  assert.equal(await page.evaluate(()=>document.querySelector('#search-dialog').matches(':modal')),false);
  await page.screenshot({path:path.join(output,`${width}-search.png`)});await page.evaluate(()=>document.getElementById('search-dialog').close());
  await page.evaluate(()=>window.OchatProductivity.onEvent({type:'transfer',room:selectedRoom,id:'file',peer:'peer',name:'Peer',bytes:100,total:1000,status:'receiving'}));
  await page.evaluate(()=>window.OchatProductivity.openTransfers());assert.ok((await page.locator('#transfer-center').textContent()).includes('receiving'));await page.keyboard.press('Escape');
  await page.evaluate(()=>document.getElementById('workspace-focus').click());await page.waitForTimeout(100);assert.equal(await page.evaluate(()=>document.body.dataset.workspaceFocus),'true');
  await page.locator('#workspace-restore').click();await page.waitForTimeout(100);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
  await page.evaluate(()=>lockAccount());await page.waitForTimeout(100);assert.equal(await page.locator('.transfer-center-item').count(),0);assert.equal(await page.locator('.command-result').count(),0);
  assert.deepEqual(errors,[]);checks.push({width,passed:true});await page.close();
 }}finally{await browser.close();await new Promise(r=>server.close(r));}
 fs.writeFileSync(path.join(output,'validation.json'),JSON.stringify(checks,null,2));console.log(JSON.stringify(checks));
})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
