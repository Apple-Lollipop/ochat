const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');

test('all account password controls preserve selection, mask on leaving and clear on reset without submitting',async t=>{
  const {Window}=await import('happy-dom');
  const window=new Window({url:'http://tauri.localhost',settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});
  t.after(()=>window.happyDOM.close());
  window.document.write(fs.readFileSync('ui/index.html','utf8').replace(/<script\b[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link\b[^>]*>/g,''));
  window.eval(fs.readFileSync('ui/passwords.js','utf8'));
  let submits=0;
  window.document.addEventListener('submit',e=>{e.preventDefault();submits++;});
  const fields=[...window.document.querySelectorAll('[data-private-password]')];
  assert.equal(fields.length,9);
  for(const input of fields){
    const button=input.parentElement.querySelector('button');
    assert.equal(button.type,'button');
    assert.equal(button.getAttribute('aria-controls'),input.id);
    input.value='Synthetic password';input.focus();input.setSelectionRange(2,8);
    button.click();assert.equal(input.type,'text');assert.equal(button.getAttribute('aria-pressed'),'true');
    assert.equal(input.selectionStart,2);assert.equal(input.selectionEnd,8);
    button.click();assert.equal(input.type,'password');
    button.click();assert.equal(input.type,'text');
    window.document.getElementById('account-name').focus();
    assert.equal(input.type,'password');assert.equal(input.value,'Synthetic password');
    input.focus();button.click();assert.equal(input.type,'text');
  }
  window.OchatPasswords.reset();
  for(const input of fields){
    assert.equal(input.value,'');assert.equal(input.type,'password');
    assert.equal(input.parentElement.querySelector('button').getAttribute('aria-pressed'),'false');
  }
  assert.equal(submits,0);
});
