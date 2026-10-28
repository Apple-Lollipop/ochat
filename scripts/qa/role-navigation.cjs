/* Synthetic IPC only; checks real layout without opening a personal profile. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/role-navigation-'+require('../../package.json').version);
(async()=>{fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'}),checks=[];
try{for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
 const page=await browser.newPage({viewport:{width,height:width===480?520:850},colorScheme:theme,reducedMotion:'reduce'}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,theme);
 await page.addInitScript(()=>{const original=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=async(c,a)=>c==='room_management'?{owner:'a'.repeat(64),policy:{role_names:{moderator:'Guides'},role_colors:{moderator:'#D03299'},roles:{['b'.repeat(64)]:'moderator'},permissions:{}},members:[{id:'b'.repeat(64),name:'Maya'}],permissions:{invitations:true},invitations:[]}:original(c,a);});
 await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
 await page.evaluate(async()=>{state.role='owner';state.role_colors={moderator:'#D03299'};state.role_names={moderator:'Guides'};state.peers[0].role='moderator';render();await handleEvent({type:'message',room:state.room.id,channel:state.text_channel,id:'role-message',peer:state.peers[0].id,name:'Maya',text:'A message with my room role.',timestamp:Date.now(),emojis:[]});});
 if(width<900)await page.locator('#navigation-toggle').click();
 assert.ok(await page.locator('.channel-person .role-avatar').count()>0);
 const appearance=await page.locator('.channel-person-name').first().evaluate(n=>({decoration:getComputedStyle(n).textDecorationLine,color:getComputedStyle(n).color}));assert.equal(appearance.decoration,'none');
 const ring=await page.locator('.channel-person .role-avatar').first().evaluate(n=>getComputedStyle(n,'::after').borderTopWidth);assert.equal(ring,'1px');
 const toggle=page.locator('#conversation-create-toggle');await toggle.click();await page.locator('#new-room').waitFor({state:'visible'});
 assert.equal(await page.locator('#navigation-rooms #new-room').count(),0);
 for(const id of ['new-room','new-direct-chat','join-button']){const box=await page.locator('#'+id).boundingBox();assert.ok(box&&box.x>=0&&box.y>=0&&box.x+box.width<=width+1&&box.y+box.height<=850);}
 await page.screenshot({path:path.join(output,`${theme}-${width}-navigation.png`)});await page.keyboard.press('Escape');assert.equal(await toggle.getAttribute('aria-expanded'),'false');assert.equal(await toggle.evaluate(n=>n===document.activeElement),true);
 await toggle.click();await page.keyboard.press('End');assert.equal(await page.evaluate(()=>document.activeElement.id),'join-button');await page.keyboard.press('Escape');
 await toggle.click();await page.locator('#navigation-title').click();assert.equal(await toggle.getAttribute('aria-expanded'),'false');
 if(width<900)await page.keyboard.press('Escape');
 assert.equal(await page.locator('[data-message-id=role-message] .role-avatar').count(),1);
 await page.evaluate(()=>document.getElementById('room-access-button').click());await page.locator('#room-management').waitFor();
 const names=page.locator('#room-management [data-role-name]');assert.equal(await names.count(),4);assert.equal(await page.locator('[data-role-name=moderator]').inputValue(),'Guides');
 await page.locator('[data-role-name=owner]').fill('Captain');await page.locator('#save-room-access').click();const save=await page.evaluate(()=>window.__qaCalls.filter(c=>c.command==='save_room_roles').at(-1));assert.deepEqual(save.args.roleNames,{owner:'Captain',moderator:'Guides'});
 assert.ok(await page.locator('#room-management').evaluate(n=>n.scrollWidth<=n.clientWidth+1));await page.screenshot({path:path.join(output,`${theme}-${width}-roles.png`)});await page.keyboard.press('Escape');
 assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));assert.deepEqual(errors,[]);checks.push({theme,width,customNames:true,avatarRoleRing:true,neutralNames:true,menuKeyboard:true,overflow:false});await page.close();
}}finally{await browser.close();await new Promise(r=>server.close(r));}fs.writeFileSync(path.join(output,'validation.json'),JSON.stringify(checks,null,2));console.log(JSON.stringify({passed:checks.length,output}));})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
