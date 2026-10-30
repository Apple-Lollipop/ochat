const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');
const flush=()=>new Promise(resolve=>setImmediate(resolve));
async function setup(t,override){
 const {Window}=await import('happy-dom');const w=new Window({settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});
 t.after(()=>w.happyDOM.close());w.document.body.innerHTML='<dialog id="settings-dialog"><section data-settings-panel="account"></section></dialog>';
 const context={unlocked:true,generation:1},calls=[];let saved={enabled:false,configured:false};
 w.eval(fs.readFileSync('ui/discord-webhook.js','utf8'));
 w.OchatDiscordWebhook.init({context:()=>context,invoke:async(command,args)=>{
  calls.push({command,args});if(override)return override(command,args);
  if(command==='discord_webhook_status')return saved;
  if(command==='save_discord_webhook'){saved={enabled:args.enabled,configured:args.url===null?saved.configured:!!args.url};return saved;}
  if(command==='notify_discord_online')return saved.enabled||args.test;
  throw Error('Unexpected command');
 }});
 return {w,get:id=>w.document.getElementById(id),context,calls};
}
test('webhook settings store only an explicit URL, clear it after saving and permit saved-webhook tests',async t=>{
 const a=await setup(t);await a.w.OchatDiscordWebhook.fill();const url=a.get('discord-webhook-url');
 assert.equal(url.type,'password');assert.equal(a.get('discord-webhook-settings').textContent.includes('webhook channel'),true);
 url.value='https://discord.com/api/webhooks/synthetic/token';url.dispatchEvent(new a.w.Event('input'));a.get('discord-webhook-enabled').checked=true;
 const [save,sendTest]=a.get('discord-webhook-settings').querySelectorAll('button');assert.equal(sendTest.disabled,true);
 save.click();await flush();assert.equal(url.value,'');assert.equal(sendTest.disabled,false);
 assert.deepEqual(a.calls.filter(c=>c.command==='save_discord_webhook').map(c=>({enabled:c.args.enabled,url:c.args.url})),[{enabled:true,url:'https://discord.com/api/webhooks/synthetic/token'}]);
 assert.equal(a.calls.filter(c=>c.command==='notify_discord_online'&&c.args.test===false).length,1);
 sendTest.click();await flush();assert.match(a.get('discord-webhook-status').textContent,/Test announcement sent/);
});
test('toggling keeps the stored token and removing the integration explicitly deletes it',async t=>{
 const a=await setup(t);await a.w.OchatDiscordWebhook.fill();const buttons=a.get('discord-webhook-settings').querySelectorAll('button');
 a.get('discord-webhook-url').value='saved URL';buttons[0].click();await flush();
 a.get('discord-webhook-enabled').checked=false;buttons[0].click();await flush();
 const calls=a.calls.filter(c=>c.command==='save_discord_webhook');assert.equal(calls[1].args.url,null);
 buttons[2].click();await flush();const removed=a.calls.filter(c=>c.command==='save_discord_webhook').at(-1);assert.equal(removed.args.url,'');assert.equal(removed.args.enabled,false);assert.equal(buttons[1].disabled,true);
});
test('Lock and dialog closure discard unsaved secrets and stale status responses',async t=>{
 let resolve;const a=await setup(t,()=>new Promise(r=>resolve=r));const fill=a.w.OchatDiscordWebhook.fill();
 a.get('discord-webhook-url').value='unsaved secret';a.context.unlocked=false;a.context.generation++;a.w.OchatDiscordWebhook.reset();resolve({enabled:true,configured:true});await fill;
 assert.equal(a.get('discord-webhook-url').value,'');assert.equal(a.get('discord-webhook-enabled').checked,false);assert.equal(a.get('discord-webhook-status').textContent,'');
 const before=a.calls.length;await a.w.OchatDiscordWebhook.accountOpened();assert.equal(a.calls.length,before);
 a.get('discord-webhook-url').value='another secret';a.get('settings-dialog').dispatchEvent(new a.w.Event('close'));assert.equal(a.get('discord-webhook-url').value,'');
});
test('announcement failures never block account opening and errors remain visible only for manual actions',async t=>{
 const a=await setup(t,(command)=>command==='discord_webhook_status'?{enabled:true,configured:true}:Promise.reject(Error('Discord rate limit reached. Wait before testing again')));
 await a.w.OchatDiscordWebhook.accountOpened();await a.w.OchatDiscordWebhook.fill();a.get('discord-webhook-settings').querySelectorAll('button')[1].click();await flush();
 assert.match(a.get('discord-webhook-status').textContent,/rate limit/);assert.equal(a.get('discord-webhook-settings').querySelectorAll('button')[1].disabled,false);
});

test('general Save enables a previously saved webhook without reentering its URL and preserves it after reopening',async t=>{
 const a=await setup(t);await a.w.OchatDiscordWebhook.fill();
 a.get('discord-webhook-url').value='saved URL';a.get('discord-webhook-settings').querySelector('button').click();await flush();
 a.get('settings-dialog').dispatchEvent(new a.w.Event('close'));await a.w.OchatDiscordWebhook.fill();
 assert.equal(a.get('discord-webhook-enabled').checked,false);
 a.get('discord-webhook-enabled').checked=true;assert.equal(a.w.OchatDiscordWebhook.isDirty(),true);
 await a.w.OchatDiscordWebhook.savePending();assert.equal(a.w.OchatDiscordWebhook.isDirty(),false);
 const saved=a.calls.filter(c=>c.command==='save_discord_webhook').at(-1);assert.equal(saved.args.url,null);assert.equal(saved.args.enabled,true);
 // General Save announces only after its remaining settings have saved successfully.
 assert.equal(a.calls.filter(c=>c.command==='notify_discord_online').length,0);
 await a.w.OchatDiscordWebhook.accountOpened();
 a.get('settings-dialog').dispatchEvent(new a.w.Event('close'));await a.w.OchatDiscordWebhook.fill();
 assert.equal(a.get('discord-webhook-enabled').checked,true);assert.equal(a.get('discord-webhook-url').value,'');
 a.get('discord-webhook-enabled').checked=false;await a.w.OchatDiscordWebhook.savePending();
 a.get('settings-dialog').dispatchEvent(new a.w.Event('close'));await a.w.OchatDiscordWebhook.fill();assert.equal(a.get('discord-webhook-enabled').checked,false);
});

test('general Save propagates webhook write failures and Cancel never persists a pending toggle',async t=>{
 const a=await setup(t,command=>command==='discord_webhook_status'?{enabled:false,configured:true}:Promise.reject(Error('Could not save webhook')));
 await a.w.OchatDiscordWebhook.fill();a.get('discord-webhook-enabled').checked=true;
 await assert.rejects(a.w.OchatDiscordWebhook.savePending(),/Could not save webhook/);assert.equal(a.w.OchatDiscordWebhook.isDirty(),true);
 const count=a.calls.length;a.get('settings-dialog').dispatchEvent(new a.w.Event('close'));assert.equal(a.calls.length,count);
 await a.w.OchatDiscordWebhook.fill();assert.equal(a.get('discord-webhook-enabled').checked,false);
});

test('saving unrelated settings waits for a pending webhook load and does not rewrite an unchanged integration',async t=>{
 let resolve;const a=await setup(t,()=>new Promise(r=>resolve=r));
 const loading=a.w.OchatDiscordWebhook.fill(),saving=a.w.OchatDiscordWebhook.savePending();let completed=false;saving.then(()=>completed=true);
 await flush();assert.equal(completed,false);resolve({enabled:true,configured:true});await loading;await saving;
 assert.equal(a.calls.length,1);assert.equal(a.w.OchatDiscordWebhook.isDirty(),false);
});
