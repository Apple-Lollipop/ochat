/* Real browser layout and scrolling; isolated IPC fixture, never a real profile. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(process.env.OCHAT_QA_OUTPUT||path.join(__dirname,'../../releases/reports/UI-0.11.4'));
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));
 const browser=await chromium.launch({headless:true,channel:'msedge'}),report=[];
 try{
  for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
   const height=width===480?520:800,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];
   page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,theme);
   await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
   await page.evaluate(()=>{state.peers[0].bio='A long profile with several lines. '.repeat(8);render();});
   if(!await page.locator('#details-dialog').evaluate(n=>n.open))await page.locator('#details-toggle').click();
   const member=page.locator('#members [data-key="'+'b'.repeat(64)+'"] .member-profile');await member.click();
   const dialog=page.locator('#floating-profile');
   await page.locator('#profile-extra-details > summary').click();
   await page.waitForTimeout(100);
   const geometry=await dialog.evaluate(n=>({top:n.getBoundingClientRect().top,bottom:n.getBoundingClientRect().bottom,client:n.clientHeight,scroll:n.scrollHeight,overflow:getComputedStyle(n).overflowY,viewport:innerHeight}));
   console.log(JSON.stringify({theme,width,height,...geometry}));
   assert.ok(geometry.bottom<=height-10,'Expanded profile must stay above the bottom of the window');
   const bounds=await dialog.boundingBox();await page.mouse.move(bounds.x+bounds.width/2,Math.min(height-20,bounds.y+bounds.height/2));await page.mouse.wheel(0,1600);
   await page.waitForFunction(()=>{const n=document.querySelector('#floating-profile');return n.scrollHeight<=n.clientHeight||n.scrollTop>0;});
   const identity=page.locator('#peer-identity');await identity.scrollIntoViewIfNeeded();
   const identityBounds=await identity.boundingBox();assert.ok(identityBounds.y>=0&&identityBounds.y+identityBounds.height<=height,'Full identity is visible');
   const scrollTop=await dialog.evaluate(n=>n.scrollTop);await page.evaluate(()=>render());assert.equal(await dialog.evaluate(n=>n.scrollTop),scrollTop,'Presence updates preserve profile scroll');
   await page.screenshot({path:path.join(output,`${theme}-${width}-profile-identity.png`)});
   await page.keyboard.press('Escape');assert.equal(await dialog.evaluate(n=>n.open),false);assert.equal(await member.evaluate(n=>n===document.activeElement),true);
   await member.click();assert.equal(await dialog.evaluate(n=>n.scrollTop),0);
   await page.locator('#profile-extra-details > summary').focus();await page.keyboard.press('Enter');await page.keyboard.press('Tab');
   assert.equal(await page.locator('#verify-peer-button').evaluate(n=>n===document.activeElement),true);
   await page.setViewportSize({width:480,height:520});await page.waitForTimeout(100);
   const resized=await dialog.boundingBox();assert.ok(resized.x>=0&&resized.y>=0&&resized.x+resized.width<=480&&resized.y+resized.height<=510);
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
   assert.deepEqual(errors,[]);report.push({theme,width,height,expandedProfileVisible:true,wheel:true,keyboard:true,resize:true,focusRestored:true,scrollPreserved:true});await page.close();
  }
  fs.writeFileSync(path.join(output,'profile-scroll.json'),JSON.stringify({engine:'Microsoft Edge headless',fixture:true,checks:report},null,2));console.log('Profile scroll: '+report.length+' layouts passed');
 }finally{await browser.close();await new Promise(r=>server.close(r));}
})().catch(error=>{console.error(error);process.exitCode=1;});
