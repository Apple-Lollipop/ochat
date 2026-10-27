/* Layout checks with synthetic IPC; no accounts, microphones or WAN peers. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/UI-0.12.2');
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'}),checks=[];
 try{for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
  const height=width===480?520:800,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,theme);
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');if(!await page.locator('#settings-button').isVisible())await page.locator('#navigation-toggle').click();await page.locator('#settings-button').click();await page.locator('[data-settings-tab="privacy"]').click();await page.locator('#relay-only').check();assert.equal(await page.locator('#tor-enabled').isChecked(),false);await page.locator('#relay-only').scrollIntoViewIfNeeded();
  const bounds=await page.locator('#relay-only').boundingBox();assert.ok(bounds.x>=0&&bounds.x+bounds.width<=width&&bounds.y>=0&&bounds.y+bounds.height<=height);assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:path.join(output,`${theme}-${width}-relay-only.png`)});
  await page.locator('#settings-cancel').click();await page.evaluate(()=>{
   document.getElementById('voice-diagnostics-dialog').showModal();window.OchatWorkspace.diagnostics({network:{peers:[{name:'Maya',missing_percent:0}],transport:{produced:100,queued:100,received:100,rejected:0,errors:0},playback:{decoded_frames:95,decode_errors:0,rendered_samples:91200,level:.1,jitter:{['b'.repeat(64)]:{late_packets:5,concealed_frames:3,target_ms:100,queued_ms:60,rebuffer_events:1,discarded_packets:0}}}}});
  });assert.match(await page.locator('#voice-diagnostics-peers').innerText(),/5 late packets/);assert.match(await page.locator('#voice-diagnostics-peers').innerText(),/0.0% recent missing packets/);assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:path.join(output,`${theme}-${width}-diagnostics.png`)});assert.deepEqual(errors,[]);checks.push({theme,width,height,relayToggleVisible:true,torRemainsOff:true,lateVsArrivalLoss:true,horizontalOverflow:false});await page.close();
 }}finally{await browser.close();await new Promise(r=>server.close(r));}
 fs.writeFileSync(path.join(output,'voice.json'),JSON.stringify({engine:'Microsoft Edge headless',nativeIPC:'isolated fixture',checks},null,2));console.log(checks.length+' voice/route layouts passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
