/* Synthetic IPC only: no accounts, external sites or capture devices. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/todo-integration');
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({headless:true,channel:'msedge'}),checks=[];
 try{for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
  const height=width===480?520:850,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,theme);
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');
  await page.evaluate(async()=>{
   document.getElementById('toast').hidden=true;document.getElementById('navigation-dialog').close();
   const canvas=document.createElement('canvas');canvas.width=canvas.height=128;const ctx=canvas.getContext('2d');ctx.fillStyle='#5965df';ctx.fillRect(0,0,128,128);ctx.fillStyle='white';ctx.font='18px sans-serif';ctx.fillText('Image',38,70);const data=canvas.toDataURL();
   const image={type:'message',id:'qa-image',room:selectedRoom,channel:state.text_channel,peer:'b'.repeat(64),name:'Maya',text:'ochat-image:'+'e'.repeat(64),timestamp:Date.now(),emojis:[]};
   const invoke=window.__TAURI__.core.invoke;window.__TAURI__.core.invoke=(command,args)=>command==='message_image'?Promise.resolve(data):command==='thread_history'?Promise.resolve({entries:[{event:image,timestamp:image.timestamp}],total:1}):invoke(command,args);
   window.__imageEvent=image;
   await handleEvent({...image,id:'qa-formatted',text:'**Ready for review**\n- Clear labels\n- Keyboard access\n```js\nconst safe = "<literal>";\n```\n||A private spoiler||'});
   await handleEvent(image);document.querySelector('[data-message-id="qa-image"] .reply-action').click();
  });
  await page.locator('#reply-excerpt img').waitFor();await page.waitForFunction(()=>document.querySelector('#reply-excerpt img')?.naturalWidth===128);
  assert.equal(await page.locator('[data-message-id="qa-formatted"] strong').filter({hasText:'Ready for review'}).count(),1);
  const toolbar=page.locator('#rich-text-toolbar');assert.ok(await toolbar.isVisible());
  const bounds=await toolbar.boundingBox();assert.ok(bounds.x>=0&&bounds.x+bounds.width<=width+1&&bounds.y+bounds.height<=height);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await page.screenshot({path:path.join(output,`${theme}-${width}-messages.png`)});
  await page.evaluate(()=>window.OchatWorkspace.openThread(window.__imageEvent));await page.waitForFunction(()=>document.querySelector('#thread-messages .message-image-full')?.naturalWidth===128);
  assert.equal(await page.locator('#thread-messages .inline-image').count(),1);await page.screenshot({path:path.join(output,`${theme}-${width}-thread.png`)});await page.keyboard.press('Escape');
  await page.evaluate(()=>document.getElementById('organize-conversation').click());await page.locator('#organization-nickname').fill('My design team');await page.locator('#organization-folder').fill('Work');
  assert.ok(await page.locator('#conversation-organization').evaluate(n=>n.scrollWidth<=n.clientWidth+1));await page.screenshot({path:path.join(output,`${theme}-${width}-organization.png`)});await page.locator('#organization-cancel').click();
  await page.evaluate(()=>lockAccount());assert.equal(await page.locator('#rich-text-toolbar').isVisible(),false);assert.equal(await page.locator('#reply-excerpt img').count(),0);
  assert.deepEqual(errors,[]);checks.push({theme,width,height,richText:true,imageReply:true,imageThread:true,organization:true,lockCleanup:true,noHorizontalOverflow:true});await page.close();
 }
 fs.writeFileSync(path.join(output,'validation.json'),JSON.stringify({engine:'Edge headless',network:'synthetic IPC',checks},null,2));console.log(JSON.stringify({passed:checks.length,output}));
 }finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
