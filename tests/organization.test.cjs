const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');

test('local organization groups keyed rows, saves only local labels and clears private dialog content',async t=>{
  const {Window}=await import('happy-dom');
  const window=new Window({url:'http://tauri.localhost',settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});
  t.after(()=>window.happyDOM.close());
  window.document.write('<dialog id="room-actions-dialog"></dialog><div id="direct-room-list"></div><div id="group-room-list"><button class="room-item" data-key="room"><strong>Original</strong></button><button class="room-item" data-key="other">Other</button></div>');
  const setup=`let unlocked=true,selectedRoom='room',sessionGeneration=1,conversationLayout={order:['room','other'],pinned:[],preferences:{room:{nickname:'Local team',folder:'Work',color:'#AABBCC'}}};function allRooms(){return [{id:'room',name:'Actual team'}]}function conversationContext(){return {title:'Actual team'}}function show(id){document.getElementById(id).setAttribute('open','')}async function saveLayout(next){conversationLayout=next}function render(){window.OchatOrganization.decorate()}window.testState={lock(){unlocked=false;conversationLayout={order:[],pinned:[]};window.OchatOrganization.reset()},layout(){return conversationLayout}};`;
  window.eval(setup+'\n'+fs.readFileSync('ui/organization.js','utf8'));
  const doc=window.document,row=doc.querySelector('[data-key="room"]');
  window.OchatOrganization.decorate();
  assert.equal(window.OchatOrganization.title('room','Actual'),'Local team');
  assert.equal(doc.querySelector('.conversation-folder-heading').textContent,'Work');
  assert.equal(row.style.getPropertyValue('--conversation-color'),'#AABBCC');
  window.OchatOrganization.decorate();assert.equal(doc.querySelector('[data-key="room"]'),row);assert.equal(doc.querySelectorAll('.conversation-folder-heading').length,1);
  doc.getElementById('organize-conversation').click();assert.match(doc.getElementById('organization-original').textContent,/Actual team/);
  doc.getElementById('organization-nickname').value='<b>Private alias</b>';doc.getElementById('organization-folder').value='Friends';
  doc.querySelector('#conversation-organization form').dispatchEvent(new window.Event('submit',{cancelable:true}));await new Promise(r=>setImmediate(r));
  assert.equal(window.testState.layout().preferences.room.nickname,'<b>Private alias</b>');assert.equal(window.testState.layout().preferences.room.folder,'Friends');
  doc.getElementById('organize-conversation').click();window.testState.lock();
  assert.equal(doc.getElementById('organization-nickname').value,'');assert.equal(doc.getElementById('organization-original').textContent,'');assert.equal(doc.getElementById('organization-folders').children.length,0);
  assert.equal(window.OchatOrganization.title('room','Actual'),'Actual');assert.equal(row.style.getPropertyValue('--conversation-color'),'');
});
