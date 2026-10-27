/* Isolated browser fixture. Never opens a real account or native capture device. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/UI-0.12.0');
function communityFixture(theme){
 const original=window.__TAURI__.core.invoke;
 const owner='a'.repeat(64),peer='b'.repeat(64),room='10000000-0000-4000-8000-000000000001';
 const root={type:'message',room,channel:'20000000-0000-4000-8000-000000000001',id:'40000000-0000-4000-8000-000000000001',peer,name:'Maya',text:'The latest changes are ready for review.',timestamp:Date.now()};
 window.__TAURI__.core.invoke=async(command,args)=>{
  if(command==='room_management')return {owner,policy:null,permissions:{invitations:true,channels:true,calls:true,pins:true},members:[{id:peer,name:'A very long member name that must wrap without hiding the complete identity'}],invitations:[]};
  if(command==='create_controlled_invitation')return 'ochat-invite:synthetic-preview-only';
  if(command==='privacy_dashboard')return {tor_enabled:false,tor_ready:false,relay_enabled:true,external_previews:false,blocked:[peer],sessions:[{room:{id:room,name:'Ochat Studio'},peers:[{id:peer,name:'Maya',route:'direct',latency_ms:24}]}]};
  if(command==='thread_history')return {entries:[{room,timestamp:Date.now(),event:root},{room,timestamp:Date.now(),event:{...root,id:'40000000-0000-4000-8000-000000000002',peer:owner,name:'Avery',text:'Looks good. Let’s discuss this here.',reply_to:{peer,id:root.id}}}],total:2,offset:0};
  if(command==='set_availability'||command==='save_room_roles')return;
  return original(command,args);
 };
}
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'}),report=[];
 try{for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
  const height=width===480?520:800,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript({content:`(${fixture.toString()})(${JSON.stringify(theme)});(${communityFixture.toString()})(${JSON.stringify(theme)});`});
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
  const check=async id=>{const b=await page.locator('#'+id).boundingBox();assert.ok(b&&b.x>=0&&b.y>=0&&b.x+b.width<=width+1&&b.y+b.height<=height+1,JSON.stringify({id,width,b}));assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);};
  await page.locator('#message').fill('Main draft stays here');await page.locator('#room-actions-toggle').click();await page.locator('#room-access-button').click();await page.locator('#role-members select').last().waitFor();await check('room-management');await page.screenshot({path:path.join(output,`${theme}-${width}-roles.png`)});
  await page.locator('#invite-uses').fill('5');await page.locator('#create-controlled-invite').click();await page.locator('#controlled-invitation-output').filter({visible:true}).waitFor();assert.equal(await page.locator('#controlled-invitation-output').inputValue(),'ochat-invite:synthetic-preview-only');await page.screenshot({path:path.join(output,`${theme}-${width}-invitations.png`)});await page.keyboard.press('Escape');
  await page.evaluate(()=>window.OchatWorkspace.openDashboard());await page.locator('#blocked-contacts button').waitFor();await check('privacy-dashboard');await page.screenshot({path:path.join(output,`${theme}-${width}-privacy.png`)});await page.keyboard.press('Escape');
  await page.locator('#timeline .message').first().hover();await page.locator('#timeline .message-toolbar button').filter({hasText:'Thread'}).first().click();await page.locator('#thread-messages article').first().waitFor();await check('thread-dialog');await page.locator('#thread-message').fill('A reply in the thread');const sendBounds=await page.locator('#thread-composer button').boundingBox();assert.ok(sendBounds.y>=0&&sendBounds.y+sendBounds.height<=height-10,'Thread Send stays visible');await page.screenshot({path:path.join(output,`${theme}-${width}-thread.png`)});await page.keyboard.press('Escape');assert.equal(await page.locator('#message').inputValue(),'Main draft stays here');
  assert.deepEqual(errors,[]);report.push({theme,width,height,roles:true,invitations:true,privacy:true,thread:true,draftPreserved:true,horizontalOverflow:false});await page.close();
 }}finally{await browser.close();await new Promise(r=>server.close(r));}
 fs.writeFileSync(path.join(output,'community.json'),JSON.stringify({engine:'Microsoft Edge headless',nativeIPC:'fixture only',checks:report},null,2));console.log(report.length+' community layouts passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
