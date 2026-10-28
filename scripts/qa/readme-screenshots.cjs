/* Capture the actual UI with synthetic IPC and public demo content only. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../docs/screenshots');
(async()=>{fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'});
try{for(const theme of ['dark','light']){
 const page=await browser.newPage({viewport:{width:1440,height:900},deviceScaleFactor:1,colorScheme:theme,reducedMotion:'reduce'}),errors=[];
 page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,theme);
 await page.addInitScript(()=>{const previous=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=async(c,a)=>{
  if(c==='list_friends')return ['Maya','Noah','Jamie','Alex','Sam','Robin','Casey','Morgan'].map((name,i)=>({peer:(i+11).toString(16).repeat(64),name,status:'accepted',direct_room:i===0?'10000000-0000-4000-8000-000000000002':null}));
  if(c==='audio_devices')return [{id:'system-mic',name:'System default microphone',input:true,output:false},{id:'system-output',name:'System default speakers',input:false,output:true}];
  return previous(c,a);
 };});
 await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
 await page.evaluate(()=>{document.getElementById('toast').hidden=true;state.role='owner';state.role_names={owner:'Host',moderator:'Guide',member:'Member'};state.role_colors={owner:'#72A7E8',moderator:'#CA8FAF'};state.peers[0].role='moderator';state.peers[1].role='member';render();});
 if(theme==='dark'){
  await page.evaluate(async()=>{const lines=[['b','Maya','**Today’s plan**\n- Review the new ideas\n- Share our notes\n- Catch up in voice'],['c','Noah','I’m in Lounge if anyone wants to join.'],['a','Avery','Perfect. Let’s keep project updates here and sketches in #projects.'],['b','Maya','Sounds good! ✨']];for(let i=0;i<lines.length;i++){const [id,name,text]=lines[i];await handleEvent({type:'message',room:state.room.id,channel:state.text_channel,id:'readme-demo-'+i,peer:id.repeat(64),name,text,timestamp:Date.now()-45000+i*10000,emojis:[],mentions:[]});}document.getElementById('message').focus();});
  await page.screenshot({path:path.join(output,'conversations.png')});
  await page.locator('#settings-button').click();await page.locator('[data-settings-tab="audio"]').click();await page.locator('#settings-dialog').waitFor();
  await page.screenshot({path:path.join(output,'audio.png')});
 }else{
  await page.locator('#rail-home').click();await page.waitForFunction(()=>document.body.dataset.homeOpen==='true');assert.equal(await page.locator('#details-dialog').evaluate(n=>n.open),false);assert.equal(await page.locator('#floating-profile').evaluate(n=>n.open),false);
  await page.screenshot({path:path.join(output,'friends.png')});
 }
 assert.deepEqual(errors,[]);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.close();
}}finally{await browser.close();await new Promise(r=>server.close(r));}console.log('Captured three real UI screenshots with synthetic profiles: '+output);})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
