/* Isolated browser fixture: never opens a real account or capture device. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/UI-0.12.1');
const gif='data:image/gif;base64,'+fs.readFileSync(path.resolve(__dirname,'fixtures/avatar.gif')).toString('base64');
function avatars(data){
 const original=window.__TAURI__.core.invoke;
 const patch=s=>({...s,peers:s.peers.map((p,i)=>({...p,availability:i?'busy':'away',avatar_hash:'d'.repeat(64)}))});
 window.__TAURI__.core.invoke=async(c,a)=>{
  if(c==='emoji_asset')return data;
  const result=await original(c,a);
  if(c==='snapshot'||c==='select_room')return patch(result);
  if(c==='room_snapshots')return result.map(patch);
  return result;
 };
 window.addEventListener('load',async()=>{await new Promise(r=>setTimeout(r,80));state=patch(state);roomSnapshots.set(state.room.id,state);render();});
}
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'}),report=[];
 try{for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
  const height=width===480?520:800,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(fixture,theme);await page.addInitScript(avatars,gif);
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');await page.waitForTimeout(180);
  await page.evaluate(()=>{document.getElementById('details-toggle').click();});
  await page.waitForFunction(()=>[...document.querySelectorAll('#members img.profile-photo')].some(i=>i.complete&&i.naturalWidth===48));
  const statuses=await page.locator('#members .presence-dot').evaluateAll(nodes=>nodes.map(n=>n.dataset.status));assert.ok(statuses.includes('away')&&statuses.includes('busy'));
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  await page.screenshot({path:path.join(output,`${theme}-${width}-avatars.png`)});
  await page.evaluate(()=>{document.querySelectorAll('dialog[open]').forEach(d=>d.close());document.getElementById('create-account-form').hidden=false;document.getElementById('unlock-form').hidden=true;document.getElementById('import-form').hidden=true;document.getElementById('auth-dialog').showModal();});
  const field=page.locator('#create-password'),eye=page.locator('[aria-controls="create-password"]');await field.fill('sample testing password');await eye.click();assert.equal(await field.getAttribute('type'),'text');await page.locator('#create-password-confirm').focus();assert.equal(await field.getAttribute('type'),'password');await eye.click();assert.equal(await field.getAttribute('type'),'text');await eye.click();assert.equal(await field.getAttribute('type'),'password');
  const bounds=await eye.boundingBox();assert.ok(bounds.x>=0&&bounds.x+bounds.width<=width);assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  await page.screenshot({path:path.join(output,`${theme}-${width}-passwords.png`)});assert.deepEqual(errors,[]);report.push({theme,width,height,gifDecoded:true,availabilityDots:true,repeatedPasswordReveal:true,horizontalOverflow:false});await page.close();
 }}finally{await browser.close();await new Promise(r=>server.close(r));}
 fs.writeFileSync(path.join(output,'patch.json'),JSON.stringify({engine:'Microsoft Edge headless',nativeIPC:'fixture only',checks:report},null,2));console.log(report.length+' patch layouts passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
