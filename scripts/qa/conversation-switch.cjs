/* Browser regression: real DOM and keyboard navigation, synthetic native IPC. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'});try{for(const width of [480,780,1180,1600]){
 const page=await browser.newPage({viewport:{width,height:850},reducedMotion:'reduce'});await page.addInitScript(fixture,'dark');await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
 const group=await page.evaluate(()=>selectedRoom);const general=await page.evaluate(()=>selectedChannel());
 await page.locator('#message').fill('Group draft retained');
 async function go(query,click=false){await page.keyboard.press('Control+k');await page.locator('#command-palette input').fill(query);if(click)await page.locator('.command-result').first().click();else await page.keyboard.press('Enter');await page.waitForTimeout(100);}
 await go('Maya');assert.notEqual(await page.evaluate(()=>selectedRoom),group);await page.locator('#message').fill('Private draft retained');
 await go('Ochat Studio',true);assert.equal(await page.evaluate(()=>selectedRoom),group);assert.equal(await page.locator('#message').inputValue(),'Group draft retained');
 await go('Maya');assert.equal(await page.locator('#message').inputValue(),'Private draft retained');
 await go('#projects');assert.equal(await page.evaluate(()=>selectedRoom),group);assert.notEqual(await page.evaluate(()=>selectedChannel()),general);assert.equal(await page.evaluate(({group,general})=>drafts.get(group+'/'+general),{group,general}),'Group draft retained');
 await page.evaluate(()=>{const offline='90000000-0000-4000-8000-000000000001';rooms.push({id:offline,name:'Offline notes',capacity:0});const original=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=async(c,a)=>c==='select_room'&&a.id===offline?null:c==='channel_view'&&a.room===offline?{channels:[],selected:null,unread:{}}:c==='history'&&a.room===offline?[{timestamp:1,event:{type:'message',id:'offline-history',room:offline,peer:state.id,name:'You',text:'Saved offline history',timestamp:1,emojis:[]}}]:original(c,a);renderRooms();});
 await go('Offline notes');assert.equal(await page.evaluate(()=>selectedRoom),'90000000-0000-4000-8000-000000000001');assert.ok((await page.locator('#timeline').innerText()).includes('Saved offline history'));assert.equal(await page.locator('#reconnect-panel').isVisible(),true);assert.equal(await page.evaluate(()=>isActiveRoom()),false);
 assert.equal(await page.locator('#details-resizer').count(),0);assert.equal(await page.locator('[id^="inspector-resizer-"]').count(),0);
 await page.evaluate(()=>{settings.workspace={...settings.workspace,details_width:480};window.OchatProductivity.sync();});await page.waitForTimeout(30);const memberWidth=await page.evaluate(()=>parseInt(document.documentElement.style.getPropertyValue('--workspace-details')));assert.ok(memberWidth>=220&&memberWidth<=260);
 await page.screenshot({path:`releases/reports/conversation-switch-${width}.png`});await page.close();
 }console.log('4 layouts passed: keyboard/click selection, private/group/channel history, offline selection, draft retention and fixed member panel');}finally{await browser.close();server.close();}})().catch(e=>{console.error(e);server.close();process.exitCode=1;});

