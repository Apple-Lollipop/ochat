/* Real layout and decoding, synthetic IPC. Never opens an account or microphone. */
const {fixture,server}=require('./visual.cjs');
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const output=path.resolve(__dirname,'../../releases/reports/UI-0.12.3');
function features(){
 const original=window.__TAURI__.core.invoke;
 window.__qaSaved=[];window.__qaCrops=[];window.__qaImageCalls=0;
 const delegated=s=>({...s,room:s.room?.capacity===0?{...s.room,owner:'f'.repeat(64)}:s.room,permissions:{channels:true,calls:true,invitations:false,pins:true}});
 const picture=(w,h)=>{const c=document.createElement('canvas');c.width=w;c.height=h;const x=c.getContext('2d');x.fillStyle='#df5656';x.fillRect(0,0,w/2,h);x.fillStyle='#23a55a';x.fillRect(w/2,0,w/2,h);return c.toDataURL('image/png');};
 window.__TAURI__.core.invoke=async(c,a)=>{
  if(c==='choose_photo_source'){const data=picture(400,200);return {source:data.split(',')[1],preview:data,width:400,height:200,animated:false};}
  if(c==='save_cropped_photo'){window.__qaCrops.push(a);return;}
  if(c==='message_image'){window.__qaImageCalls++;return new Promise(resolve=>{window.__qaResolveImage=()=>resolve(picture(320,900));});}
  if(c==='favorite_chat_image'){window.__qaSaved=a.active?[{hash:a.hash,name:a.name,bytes:100}]:[];return {recent:[],favorites:[],saved_images:window.__qaSaved};}
  if(c==='media_library')return {recent:[],favorites:[],saved_images:window.__qaSaved};
  if(c==='saved_image')return picture(100,200);
  const value=await original(c,a);if(c==='select_room'||c==='snapshot')return delegated(value);if(c==='room_snapshots')return value.map(delegated);return value;
 };
}
(async()=>{
 fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));const browser=await chromium.launch({headless:true,channel:'msedge'}),checks=[];
 try{for(const theme of ['light','dark'])for(const width of [480,780,1180,1600]){
  const height=width===480?520:800,page=await browser.newPage({viewport:{width,height},colorScheme:theme,reducedMotion:'reduce'}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(fixture,theme);await page.addInitScript(features);
  await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForFunction(()=>document.body.dataset.qaReady==='true');assert.equal(await page.evaluate(()=>window.__qaHomeAtUnlock),'true');
  if(!await page.locator('#settings-button').isVisible())await page.locator('#navigation-toggle').click();await page.locator('#settings-button').click();await page.locator('[data-settings-tab="account"]').click();await page.locator('#choose-profile-photo').click();await page.locator('#photo-crop-dialog').waitFor();
  await page.locator('#photo-crop-zoom').evaluate(n=>{n.value='2';n.dispatchEvent(new Event('input'));});const b=await page.locator('#photo-crop-stage').boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+b.width/2-40,b.y+b.height/2);await page.mouse.up();assert.equal(await page.locator('#photo-crop-apply').isVisible(),true);await page.screenshot({path:path.join(output,`${theme}-${width}-crop.png`)});await page.locator('#photo-crop-apply').click();await page.locator('#photo-crop-dialog').waitFor({state:'hidden'});assert.equal(await page.evaluate(()=>window.__qaCrops.length),1);await page.locator('#settings-cancel').click();
  const manage=page.locator('#room-channels .channel-manage').first();if(!await manage.isVisible())await page.locator('#navigation-toggle').click();await manage.click();assert.equal(await page.locator('#channel-editor-dialog').isVisible(),true);await page.screenshot({path:path.join(output,`${theme}-${width}-delegated-channel.png`)});await page.locator('#channel-editor-dialog .dialog-actions button').first().click();if(width<900)await page.evaluate(()=>document.getElementById('navigation-dialog').close());
  await page.locator('#message').fill('Hello ');await page.locator('#emoji-button').click();await page.waitForFunction(()=>[...document.querySelectorAll('#unicode-grid img.twemoji')].slice(0,12).every(i=>i.complete&&i.naturalWidth>0));const popup=await page.locator('#emoji-dialog').boundingBox();assert.ok(popup.width<=362&&popup.height<=392&&popup.x>=0&&popup.y>=0&&popup.x+popup.width<=width+1&&popup.y+popup.height<=height+1,JSON.stringify(popup));const emoji=page.locator('#unicode-grid button[aria-label="😀"]');await emoji.click();await emoji.click();await emoji.click();assert.equal(await page.locator('#message').inputValue(),'Hello 😀😀😀');assert.equal(await page.locator('#emoji-dialog').isVisible(),true);await page.screenshot({path:path.join(output,`${theme}-${width}-emojis.png`)});await page.keyboard.press('Escape');
  await page.evaluate(async()=>{const voice=state.channels.find(c=>c.kind==='text');await handleEvent({type:'message',room:state.room.id,channel:voice.id,id:'41000000-0000-4000-8000-000000000001',peer:'b'.repeat(64),name:'Maya',text:'ochat-image:'+'d'.repeat(64),emojis:[],timestamp:Date.now()});});await page.waitForFunction(()=>typeof window.__qaResolveImage==='function');await page.locator('.message-image-placeholder').last().waitFor();await page.screenshot({path:path.join(output,`${theme}-${width}-image-loading.png`)});await page.evaluate(()=>window.__qaResolveImage());await page.locator('.message-image-media.ready').last().waitFor();await page.waitForTimeout(60);
  const media=await page.locator('.message-image-media.ready img').last().boundingBox(),timeline=await page.locator('#timeline').boundingBox();assert.ok(media.y>=timeline.y-2&&media.y+media.height<=timeline.y+timeline.height+2,JSON.stringify({media,timeline}));await page.locator('[data-favorite-image]').last().click();assert.equal(await page.locator('[data-favorite-image]').last().getAttribute('aria-pressed'),'true');await page.screenshot({path:path.join(output,`${theme}-${width}-image-ready.png`)});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);assert.deepEqual(errors,[]);checks.push({theme,width,height,homeAtUnlock:true,crop:true,delegatedChannels:true,repeatEmoji:true,loadingPlaceholder:true,imageFullyVisible:true,favorite:true,horizontalOverflow:false});await page.close();
 }}finally{await browser.close();await new Promise(r=>server.close(r));}
 fs.writeFileSync(path.join(output,'chat.json'),JSON.stringify({engine:'Microsoft Edge headless',nativeIPC:'fixture only',checks},null,2));console.log(checks.length+' chat/crop/emoji layouts passed');
})().catch(e=>{console.error(e);process.exitCode=1;});
