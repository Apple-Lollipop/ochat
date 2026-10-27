/* Real pointer regression: account fields only, synthetic IPC and no real profile. */
const {server}=require('./visual.cjs');
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path');
const output=path.resolve(__dirname,'../../releases/reports/password-reveal');
async function checkField(page,id){
  const input=page.locator('#'+id),button=input.locator('..').locator('.password-visibility');
  await input.fill('Synthetic password');
  await input.evaluate(n=>n.setSelectionRange(2,8));
  await button.click();assert.equal(await input.getAttribute('type'),'text',id);
  assert.deepEqual(await input.evaluate(n=>[n.selectionStart,n.selectionEnd]),[2,8]);
  await button.click();assert.equal(await input.getAttribute('type'),'password',id);
  const before=await button.boundingBox();
  await page.mouse.move(before.x+before.width/2,before.y+before.height/2);
  await page.mouse.down();const pressed=await button.boundingBox();
  assert.ok(Math.abs(before.y-pressed.y)<.1,'Pressed reveal target must not move');
  await page.mouse.up();assert.equal(await input.getAttribute('type'),'text',id);
  await input.press('Tab');
  await button.press('Space');assert.equal(await input.getAttribute('type'),'password',id);
  await button.focus();await button.press('Enter');assert.equal(await input.getAttribute('type'),'text',id);
}
(async()=>{
  fs.mkdirSync(output,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));
  let browser;const report=[];
  try{
    browser=await chromium.launch({headless:true,channel:process.env.OCHAT_QA_BROWSER||'msedge'});
    for(const theme of ['light','dark'])for(const width of [480,1180])for(const reducedMotion of ['no-preference','reduce']){
      const page=await browser.newPage({viewport:{width,height:width===480?520:800},reducedMotion,colorScheme:theme});
      try{
        await page.addInitScript(()=>{window.__qaCommands=[];window.__qaSubmits=0;document.addEventListener('submit',()=>window.__qaSubmits++,true);window.__TAURI__={event:{listen:async()=>()=>{}},core:{invoke:async c=>{window.__qaCommands.push(c);return c==='account_status'?false:null;}}};});
        await page.goto('http://127.0.0.1:'+server.address().port);await page.waitForSelector('#auth-dialog[open]');
        await checkField(page,'create-password');await checkField(page,'create-password-confirm');
        assert.equal(await page.locator('#create-password').getAttribute('type'),'password','Moving to confirm masks the first field');
        await page.screenshot({path:path.join(output,`${theme}-${width}-${reducedMotion}.png`)});
        for(const [mode,ids] of [['unlock',['unlock-password']],['import',['import-password','import-password-confirm']]]){
          await page.evaluate(mode=>setAuthMode(mode),mode);
          for(const id of ids)await checkField(page,id);
        }
        for(const [dialog,ids] of [['reauth-dialog',['reauth-password']],['password-dialog',['old-password','new-password','new-password-confirm']]]){
          await page.evaluate(id=>{document.querySelectorAll('dialog[open]').forEach(n=>n.close());document.getElementById(id).showModal();},dialog);
          for(const id of ids)await checkField(page,id);
        }
        await page.evaluate(()=>window.OchatPasswords.reset());
        assert.equal(await page.locator('[data-private-password]').evaluateAll(nodes=>nodes.every(n=>n.type==='password'&&!n.value)),true);
        assert.equal(await page.evaluate(()=>window.__qaSubmits),0);
        assert.equal(await page.evaluate(()=>window.__qaCommands.includes('create_account')),false);
        report.push({theme,width,height:width===480?520:800,reducedMotion,fields:9,result:'passed'});
      }finally{await page.close();}
    }
    fs.writeFileSync(path.join(output,'validation.json'),JSON.stringify(report,null,2)+'\n');
    console.log(JSON.stringify(report,null,2));
  }finally{await browser?.close();await new Promise(r=>server.close(r));}
})().catch(error=>{console.error(error);process.exitCode=1;});
