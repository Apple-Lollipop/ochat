/* Synthetic IPC only: no real accounts, devices or network peers. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/member-status');
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({headless:true,channel:'msedge'}),checks=[];
 try{for(const theme of ['light','dark'])for(const width of [480,1180]){
  const height=width===480?520:800,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];
  page.on('pageerror',error=>errors.push(error.message));await page.addInitScript(fixture,theme);
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
  await page.evaluate(()=>{state.role='owner';state.peers=state.peers.map((peer,index)=>({...peer,role:index?'administrator':'moderator',muted:true,deafened:!index}));state.members=state.peers;render();});
  if(width<900)await page.locator('#navigation-toggle').click();
  const row=page.locator('.channel-person[data-peer="'+'b'.repeat(64)+'"]'),badge=row.locator('.voice-state-badges');
  assert.ok(await badge.isVisible());assert.ok(await badge.locator('.voice-state-muted').isVisible());assert.ok(await badge.locator('.voice-state-deafened').isVisible());
  assert.equal(await row.locator('.speaking-dot').evaluate(node=>getComputedStyle(node).visibility),'hidden');
  const box=await row.boundingBox(),icons=await badge.boundingBox();assert.ok(icons.x>=box.x&&icons.x+icons.width<=box.x+box.width+1);
  await page.screenshot({path:path.join(output,`${theme}-${width}-voice.png`)});
  await row.focus();await page.evaluate(()=>{window.__statusRow=document.activeElement;window.__statusBadge=window.__statusRow.querySelector('.voice-state-badges');state.peers[0].muted=false;state.peers[0].deafened=false;render();});
  assert.ok(await page.evaluate(()=>window.__statusRow===document.activeElement&&window.__statusRow.querySelector('.voice-state-badges')===window.__statusBadge&&window.__statusBadge.hidden));
  if(width<900)await page.keyboard.press('Escape');
  if(!await page.locator('#members').isVisible())await page.locator('#details-toggle').click();
  const member=page.locator('#members .member').filter({has:page.locator('[data-avatar-peer="'+'b'.repeat(64)+'"]')});
  assert.equal(await member.locator('.member-role').textContent(),'Moderator');
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await member.locator('.member-profile').click();await page.locator('#floating-profile').waitFor({state:'visible'});
  assert.equal(await page.locator('#peer-room-role').textContent(),'Moderator');await page.screenshot({path:path.join(output,`${theme}-${width}-profile.png`)});
  assert.deepEqual(errors,[]);checks.push({theme,width,height,badgesWithinRow:true,stableFocusedRow:true,verifiedRoleVisible:true,pageErrors:errors});await page.close();
 }
 fs.writeFileSync(path.join(output,'validation.json'),JSON.stringify({engine:'Edge headless',bridge:'synthetic IPC',checks},null,2));console.log(JSON.stringify({passed:checks.length,output}));
 }finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
