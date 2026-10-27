const assert = require('node:assert/strict');
const fs = require('node:fs');
const { test } = require('node:test');
const source = fs.readFileSync('ui/vendor/twemoji/twemoji.min.js','utf8')+'\n'+fs.readFileSync('ui/emoji-renderer.js','utf8')+'\n'+fs.readFileSync('ui/profiles.js','utf8')+'\n'+fs.readFileSync('ui/community.js','utf8')+'\n'+fs.readFileSync('ui/discord.js','utf8')+'\n'+fs.readFileSync('ui/presentation.js','utf8')+'\n'+fs.readFileSync('ui/video-renderer.js','utf8')+'\n'+fs.readFileSync('ui/media.js','utf8')+'\n'+fs.readFileSync('ui/features.js','utf8')+'\n'+fs.readFileSync('ui/meme-picker.js','utf8')+'\n'+fs.readFileSync('ui/personalization.js','utf8')+'\n'+fs.readFileSync('ui/passwords.js','utf8')+'\n'+fs.readFileSync('ui/timeline-scroll.js','utf8')+'\n'+fs.readFileSync('ui/photo-crop.js','utf8')+'\n'+fs.readFileSync('ui/emoji-popover.js','utf8')+'\n'+fs.readFileSync('ui/app.js','utf8')+'\n'+fs.readFileSync('ui/presence.js','utf8')+'\n'+fs.readFileSync('ui/workspace.js','utf8')+'\n'+fs.readFileSync('ui/native-controls.js','utf8');
const html = fs.readFileSync('ui/index.html','utf8').replace(/<script\b[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link\b[^>]*>/g,'');
const settings = {theme:'system',audio:{input_device:null,output_device:null,input_volume:1,output_volume:1,noise_suppression:true,echo_cancellation:true,automatic_gain:true},message_sound:true,ringtone:true,sound_volume:0.35,relay_enabled:false};
const base = {id:'self',name:'Test',room:null,peers:[],voice:false,muted:false,relay_enabled:false,call:null};
const connected = {...base,room:{id:'room',name:'Room',capacity:2},peers:[{id:'peer',name:'Peer',voice:false,route:'direct',typing:false}]};
const flush = () => new Promise(resolve => setImmediate(resolve));
const channelRoom = { ...connected,room:{id:'room',name:'Studio',capacity:0,owner:'self'},text_channel:'general',voice_channel:null,channels:[{id:'general',room:'room',kind:'text',name:'general',order:0},{id:'projects',room:'room',kind:'text',name:'projects',order:1},{id:'voice-general',room:'room',kind:'voice',name:'General',order:0},{id:'voice-lounge',room:'room',kind:'voice',name:'Lounge',order:1}] };

test('compact emoji picker stays open and preserves button positions for repeated insertion',async t=>{
 const a=await setup(t);a.session(connected);a.input('Hello ');a.get('message').setSelectionRange(6,6);a.get('emoji-button').click();await flush();
 assert.equal(a.get('emoji-dialog').open,true);assert.equal(a.get('emoji-dialog').classList.contains('emoji-popover'),true);
 const emoji=[...a.get('unicode-grid').children].find(n=>n.getAttribute('aria-label')==='😀'),index=[...a.get('unicode-grid').children].indexOf(emoji);emoji.click();await flush();emoji.click();await flush();
 assert.equal(a.get('message').value,'Hello 😀😀');assert.equal(a.get('emoji-dialog').open,true);assert.equal(a.get('unicode-grid').children[index],emoji);
 a.window.document.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true}));assert.equal(a.get('emoji-dialog').open,false);
});

test('channel management opens for delegated permissions and closes when revoked',async t=>{
 const a=await setup(t),room={...channelRoom,room:{...channelRoom.room,owner:'another-owner'},permissions:{channels:true}};a.session(room);const manage=a.get('room-channels').querySelector('.channel-manage');assert.equal(manage.hidden,false);manage.click();assert.equal(a.get('channel-editor-dialog').open,true);a.get('channel-name').value='Renamed';a.get('channel-editor-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();assert.ok(a.calls.some(([c,args])=>c==='edit_channel'&&args.name==='Renamed'));
 a.session(room);a.get('room-channels').querySelector('.channel-manage').click();assert.equal(a.get('channel-editor-dialog').open,true);a.session({...room,permissions:{channels:false}});assert.equal(a.get('channel-editor-dialog').open,false);
});

test('photo cropping requires Apply and retains the original room target',async t=>{
 const a=await setup(t);a.session(channelRoom);const original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='choose_photo_source'?{source:'AA==',preview:'data:image/png;base64,AAAA',width:400,height:200,animated:false}:original(c,args);
 a.get('choose-room-illustration').click();await flush();assert.equal(a.get('photo-crop-dialog').open,true);assert.equal(a.calls.some(([c])=>c==='save_cropped_photo'),false);a.get('photo-crop-dialog').close();await flush();assert.equal(a.calls.some(([c])=>c==='save_cropped_photo'),false);
 a.get('choose-room-illustration').click();await flush();a.get('photo-crop-zoom').value='2';a.get('photo-crop-zoom').dispatchEvent(new a.window.Event('input'));a.get('photo-crop-apply').click();await flush();const args=a.calls.find(([c])=>c==='save_cropped_photo')[1];assert.equal(args.room,'room');assert.equal(args.profile,false);assert.ok(args.crop.width<.5);assert.ok(Math.abs(args.crop.width*400-args.crop.height*200)<.001);
});

test('late image loading follows the latest message but respects scrolling up',async t=>{
 const a=await setup(t);a.session(connected);let height=200;const timeline=a.get('timeline');Object.defineProperty(timeline,'scrollHeight',{configurable:true,get:()=>height});Object.defineProperty(timeline,'clientHeight',{configurable:true,get:()=>100});timeline.scrollTop=100;
 await a.app.handleEvent({type:'message',room:'room',id:'late-image-layout',peer:'peer',name:'Peer',text:'A message',emojis:[],timestamp:1});const image=a.window.document.createElement('img');timeline.lastElementChild.append(image);height=500;image.dispatchEvent(new a.window.Event('load'));await new Promise(r=>setTimeout(r,30));assert.equal(timeline.scrollTop,500);
 timeline.dispatchEvent(new a.window.WheelEvent('wheel',{deltaY:-100}));timeline.scrollTop=50;height=800;image.dispatchEvent(new a.window.Event('load'));await new Promise(r=>setTimeout(r,30));assert.equal(timeline.scrollTop,50);
});

test('unlock opens Home without acknowledging a restored private conversation',async t=>{
 const a=await setup(t),snapshot={...connected,channels:[]};const original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='list_friends'?[]:c==='room_snapshots'?[snapshot]:c==='room_participants'?snapshot.peers:c==='history'?[{room:'room',timestamp:1,event:{type:'message',room:'room',id:'private-history',peer:'peer',name:'Peer',text:'Private history',timestamp:1,emojis:[]}}]:original(c,args);
 await a.app.completeAccount({snapshot,settings:structuredClone(settings),rooms:[snapshot.room]});await flush();assert.equal(a.window.document.body.dataset.homeOpen,'true');assert.equal(a.get('home-content').hidden,false);assert.equal(a.calls.some(([c])=>c==='mark_read'),false);
});

test('manual sensitivity supports quiet thresholds without changing other audio preferences',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();a.get('automatic-sensitivity').checked=false;a.get('automatic-sensitivity').dispatchEvent(new a.window.Event('change'));a.get('voice-sensitivity').value='100';a.get('voice-sensitivity').dispatchEvent(new a.window.Event('input'));assert.equal(Number(a.get('voice-threshold').value),.0001);assert.match(a.get('voice-sensitivity-label').textContent,/-80 dBFS/);a.get('settings-save').click();await flush();const next=a.calls.find(([c])=>c==='save_settings')[1].settings;assert.equal(next.audio.voice_threshold,.0001);assert.equal(next.audio.echo_cancellation,true);
});

test('a received image can be saved and resent from Favorites without touching the draft',async t=>{
 const a=await setup(t);a.session(connected);a.input('My draft');const hash='e'.repeat(64),library={recent:[],favorites:[],saved_images:[]},original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>{if(c==='media_library')return structuredClone(library);if(c==='message_image'||c==='saved_image')return 'data:image/png;base64,AAAA';if(c==='favorite_chat_image'){a.calls.push([c,args]);library.saved_images=args.active?[{hash,name:'Image from Peer',bytes:32}]:[];return structuredClone(library);}return original(c,args);};
 await a.app.handleEvent({type:'message',room:'room',id:'save-received',peer:'peer',name:'Peer',text:'ochat-image:'+hash,emojis:[],timestamp:1});await flush();await a.get('timeline').querySelector('.inline-image')._load();await flush();const star=a.get('timeline').querySelector('[data-favorite-image]');assert.ok(star);star.click();await flush();assert.equal(star.getAttribute('aria-pressed'),'true');assert.equal(a.get('message').value,'My draft');a.get('gif-button').click();await flush();a.window.document.querySelector('[data-meme-mode="favorites"]').click();await flush();const item=a.get('risibank-grid').querySelector('.risibank-item');assert.ok(item);item.click();await flush();assert.ok(a.calls.some(([c,args])=>c==='send_saved_image'&&args.hash===hash));assert.equal(a.get('message').value,'My draft');
});

test('password reveal remains available after focus changes and Lock clears revealed fields',async t=>{
 const a=await setup(t),password=a.get('create-password'),confirm=a.get('create-password-confirm');
 const eye=password.parentElement.querySelector('button');assert.ok(eye);password.value='private password';password.focus();eye.click();assert.equal(password.type,'text');
 confirm.focus();assert.equal(password.type,'password');password.focus();eye.click();assert.equal(password.type,'text');eye.click();assert.equal(password.type,'password');eye.click();assert.equal(password.value,'private password');
 a.window.OchatPasswords.reset();assert.equal(password.value,'');assert.equal(password.type,'password');assert.equal(eye.getAttribute('aria-pressed'),'false');
});

test('availability dots change color state without replacing the member or speaking indicator',async t=>{
 const a=await setup(t);a.session(channelRoom);const avatar=a.get('members').querySelector('[data-avatar-peer="peer"]'),dot=avatar.querySelector('.presence-dot');assert.equal(dot.dataset.status,'available');
 for(const status of ['away','busy','available']){a.session({...channelRoom,peers:[{...connected.peers[0],availability:status}]});assert.equal(avatar.querySelector('.presence-dot'),dot);assert.equal(dot.dataset.status,status);assert.equal(dot.getAttribute('aria-label'),({away:'Away',busy:'Do not disturb',available:'Available'})[status]);}
 a.session({...channelRoom,peers:[],members:[{...connected.peers[0],availability:'busy',online:false}]});assert.equal(a.get('members').querySelector('[data-avatar-peer="peer"] .presence-dot').dataset.status,'offline');
});

test('partial group delivery names outstanding recipients instead of implying nobody received it',async t=>{
 const a=await setup(t);a.session(channelRoom);const original=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(c,args)=>c==='message_deliveries'?[{id:'partial',status:'pending',expected:['peer','offline'],received:['peer'],read:[]}]:original(c,args);
 await a.app.handleEvent({type:'message',room:'room',channel:'general',id:'partial',peer:'self',name:'Test',text:'Group message',timestamp:1});await a.app.refreshMessageTools('room');await flush();
 const status=a.get('timeline').querySelector('[data-message-id="partial"] .message-delivery');assert.match(status.textContent,/Delivered to 1\/2 · 1 pending/);assert.match(status.title,/Offline recipients/);assert.ok(status.querySelector('button'));
});

test('relay-only can be cancelled and saving it relocks without enabling Tor',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();assert.equal(a.get('relay-only').checked,false);a.get('relay-only').checked=true;a.get('settings-cancel').click();a.get('settings-button').click();await flush();assert.equal(a.get('relay-only').checked,false);
 const original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='save_settings'?(a.calls.push([c,args]),true):original(c,args);
 a.get('relay-only').checked=true;a.get('settings-save').click();await flush();const saved=a.calls.find(([c])=>c==='save_settings')[1].settings;assert.equal(saved.relay_only,true);assert.equal(saved.tor_enabled,false);assert.equal(a.get('auth-dialog').open,true);assert.equal(a.get('message').disabled,true);
});

test('voice diagnostics separates late playout from arrival sequence loss',async t=>{
 const a=await setup(t);a.session(connected);a.window.OchatWorkspace.diagnostics({snapshot:connected,network:{peers:[{name:'Peer',missing_percent:0}],playback:{decoded_frames:10,decode_errors:0,rendered_samples:4800,level:.1,jitter:{peer:{late_packets:4,concealed_frames:3,target_ms:100,queued_ms:60,rebuffer_events:1,discarded_packets:0}}}}});
 const text=a.get('voice-diagnostics-peers').textContent;assert.match(text,/4 late packets/);assert.match(text,/buffer target 100 ms/);assert.match(text,/0.0% recent missing packets/);
});

test('appearance controls cannot shadow emoji panels and both libraries can import images',async t=>{
 const a=await setup(t);a.session(channelRoom);const ids=[...a.window.document.querySelectorAll('[id]')].map(n=>n.id);assert.equal(new Set(ids).size,ids.length,'Every UI id must be unique');assert.equal(a.get('custom-panel').tagName,'SECTION');assert.equal(a.get('appearance-panel-color').type,'color');
 const original=a.window.__TAURI__.core.invoke;const libraries={personal:[],room:[]};
 a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='personal_emojis')return libraries.personal;if(command==='room_emojis')return libraries.room;if(['import_personal_emoji','import_emoji'].includes(command)){a.calls.push([command,args]);libraries[command==='import_emoji'?'room':'personal'].push({id:command,name:args.name,hash:'a'.repeat(64)});return true;}return original(command,args);};
 for(const scope of ['personal','room']){await a.app.openEmojis(true,scope);assert.equal(a.get('custom-panel').hidden,false);assert.equal(a.get('add-emoji').disabled,false);a.get('add-emoji').click();assert.equal(a.get('emoji-edit-dialog').open,true);a.get('emoji-name').value=scope+'_cat';a.get('emoji-edit-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();assert.equal(a.get('emoji-edit-dialog').open,false);assert.match(a.get('custom-grid').textContent,new RegExp(scope+'_cat'));}
 assert.ok(a.calls.some(([c,args])=>c==='import_personal_emoji'&&args.name==='personal_cat'));assert.ok(a.calls.some(([c,args])=>c==='import_emoji'&&args.name==='room_cat'));
});

test('late emoji library responses cannot replace the currently chosen library',async t=>{
 const a=await setup(t);a.session(channelRoom);const original=a.window.__TAURI__.core.invoke;let finish;
 a.window.__TAURI__.core.invoke=(c,args)=>c==='personal_emojis'?new Promise(resolve=>{finish=resolve;}):c==='room_emojis'?Promise.resolve([{id:'room',name:'room_cat',hash:'a'.repeat(64)}]):original(c,args);
 const first=a.app.openEmojis(true,'personal');await a.app.openEmojis(true,'room');finish([{id:'personal',name:'personal_cat',hash:'b'.repeat(64)}]);await first;
 assert.match(a.get('custom-grid').textContent,/room_cat/);assert.equal(a.get('custom-grid').textContent.includes('personal_cat'),false);assert.equal(a.get('custom-tab').classList.contains('selected'),true);
});

test('Twemoji uses local artwork while drafts and message payloads remain Unicode',async t=>{
 const a=await setup(t);a.session(connected);const text='Hello 😀 👨🏽‍💻 🇫🇷 ❤️';a.input(text);await a.app.handleEvent({type:'message',room:'room',id:'twemoji',peer:'peer',name:'Peer',text,emojis:[],timestamp:1});await flush();
 const bubble=a.get('timeline').querySelector('.bubble'),images=[...bubble.querySelectorAll('img.twemoji')];assert.equal(images.length,4);assert.equal(bubble.textContent,text);assert.equal(a.get('message').value,text);for(const img of images){assert.match(img.getAttribute('src'),/^vendor\/twemoji\/svg\/[a-f0-9-]+\.svg$/);assert.ok(fs.existsSync('ui/'+img.getAttribute('src')));}
 a.submit();await flush();assert.equal(a.calls.find(([c])=>c==='send_text')[1].text,text);
});

test('Onion badges require a current peer report and never infer Tor from an ordinary relay',async t=>{
 const a=await setup(t);const remote={...connected.peers[0],route:'relay'};a.session({...connected,peers:[remote]});assert.equal(a.get('members').querySelector('.peer-tor'),null);
 a.session({...connected,peers:[{...remote,tor_enabled:true}]});const badge=a.get('members').querySelector('.peer-tor');assert.ok(badge);assert.match(badge.getAttribute('aria-label'),/reported by this peer/);assert.ok(a.get('conversation-identity').querySelector('.peer-tor'));
 a.session({...connected,peers:[],members:[{...remote,tor_enabled:true,online:false}]});assert.equal(a.get('members').querySelector('.peer-tor'),null);assert.equal(a.get('conversation-identity').querySelector('.peer-tor'),null);
});

test('Tor settings distinguish a pending checkbox change from active protection',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();assert.equal(a.get('tor-status-card').dataset.state,'off');a.get('tor-enabled').checked=true;a.get('tor-enabled').dispatchEvent(new a.window.Event('change'));assert.equal(a.get('tor-pending-change').hidden,false);assert.equal(a.get('tor-status-card').dataset.state,'off');assert.equal(a.calls.some(([c])=>c==='update_settings'),false);a.get('settings-cancel').click();
 a.get('settings-button').click();await flush();assert.equal(a.get('tor-enabled').checked,false);assert.equal(a.get('tor-pending-change').hidden,true);
});

test('Friends separates contacts from requests without losing actions or search',async t=>{
 const a=await setup(t);a.session(connected);const original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='list_friends'?[{peer:'peer',name:'Peer',status:'accepted',direct_room:'room'},{peer:'incoming',name:'Incoming person',status:'incoming'},{peer:'outgoing',name:'Outgoing person',status:'outgoing',request_delivered:false}]:original(c,args);
 a.get('friends-button').click();await flush();assert.equal(a.get('friends-all-count').textContent,'1');assert.equal(a.get('friends-requests-count').textContent,'2');assert.equal(a.get('accepted-friend-list').parentElement.hidden,false);assert.equal(a.get('received-friend-list').parentElement.hidden,true);assert.ok(a.get('accepted-friend-list').querySelector('.friend-more .danger'));
 a.window.document.querySelector('[data-friends-filter="requests"]').click();assert.equal(a.get('received-friend-list').parentElement.hidden,false);assert.equal(a.get('sent-friend-list').parentElement.hidden,false);assert.equal(a.get('accepted-friend-list').parentElement.hidden,true);a.get('friends-search').value='incoming';a.get('friends-search').dispatchEvent(new a.window.Event('input'));assert.equal(a.get('sent-friend-list').parentElement.hidden,true);const accept=[...a.get('received-friend-list').querySelectorAll('button')].find(b=>b.textContent==='Accept');accept.click();await flush();assert.ok(a.calls.some(([c,args])=>c==='respond_friend'&&args.peer==='incoming'&&args.accept));
});
async function setup(t,nativeWindow,captureCapabilities={native:false}) {
  const {Window} = await import('happy-dom');
  let window = new Window({url:'http://tauri.localhost',settings:{enableJavaScriptEvaluation:true,suppressInsecureJavaScriptEnvironmentWarning:true}});
  let app,fixture;
  const calls=[];
  // Close the associated browser and release references retained by test hooks.
  t.after(async()=>{
    try { window.OchatEmoji?.destroy();window.document.body.replaceChildren(); }
    finally {
      try { await window.happyDOM.close(); }
      finally {
        if(fixture)for(const key of Object.keys(fixture))fixture[key]=null;
        calls.length=0;app=null;window=null;
      }
    }
  });
  window.document.write(html);
  window.__TAURI__ = {event:{listen:async()=>()=>{}},core:{invoke:async(command,args)=>{calls.push([command,args]);if(command==='account_status')return false;if(command==='media_library')return {recent:[],favorites:[]};if(command==='record_recent_emoji')return {recent:[args.value],favorites:[]};if(command==='screen_capture_capabilities')return captureCapabilities;if(command==='read_clipboard_image')return null;if(['message_updates','message_deliveries','activity_items','history','room_emojis','list_rooms','audio_devices','room_participants','room_snapshots','personal_emojis'].includes(command))return [];if(command==='select_room')return connected;if(command==='get_conversation_layout')return {order:[],pinned:[]};if(command==='save_conversation_layout')return args.layout;if(command==='get_settings')return settings;if(command==='emoji_asset')return null;return 1;}}};
  if(nativeWindow)window.__TAURI__.window={getCurrentWindow:()=>nativeWindow};
  app = window.eval(source + '\n;({completeAccount,openEmojis,render,applyTheme,accentTokens,parseColor,contrast,mixColor,handleEvent,loadRoom,refreshMessageTools,openMessageTools,openReactions,openCollection,refreshActivity,refreshDiagnostics,showRecovery,lockAccount,renderEmojiLibrary,normalizeLayout,saveLayout,allRooms,acknowledgeConversation,messageLinks,hydrateRoomMembers,archive(value){rooms=value;roomMembers.clear();roomSnapshots.clear();render();},session(value,config){unlocked=true;state=value;settings=config;selectedRoom=value.room?.id||null;rooms=value.room?[value.room]:[];if(value.room)roomSnapshots.set(value.room.id,value);document.getElementById("auth-dialog").close();render();}})');
  await flush();
  const get=id=>window.document.getElementById(id);
  const session=value=>app.session(value,structuredClone(settings));
  const input=text=>{get('message').value=text;get('message').dispatchEvent(new window.Event('input'));};
  const submit=()=>get('composer').dispatchEvent(new window.Event('submit',{cancelable:true}));
  fixture={window,app,calls,get,session,input,submit};
  return fixture;
}

test('room profiles float independently, keep members visible and restore focus',async t=>{
 const a=await setup(t);a.session({...channelRoom,peers:[{...connected.peers[0],bio:'Hello 👨🏽‍💻',latency_ms:26}]});a.get('details-toggle').click();const member=a.get('members').querySelector('[data-key="peer"] .member-profile');member.focus();member.click();
 assert.equal(a.get('floating-profile').open,true);assert.equal(a.get('details-dialog').open,true);assert.equal(a.get('participants-section').hidden,false);assert.equal(a.get('details-overview').hidden,false);assert.equal(a.get('details-title').textContent,'Group details');assert.equal(a.get('floating-profile').contains(a.get('peer-card')),true);assert.equal(a.get('peer-connection').textContent,'Direct · 26 ms');
 a.get('floating-profile').dispatchEvent(new a.window.Event('cancel',{cancelable:true}));assert.equal(a.get('floating-profile').open,false);assert.equal(a.window.document.activeElement,member);assert.equal(a.get('details-dialog').contains(a.get('peer-card')),true);
 member.click();await a.app.lockAccount();assert.equal(a.get('floating-profile').open,false);assert.equal(a.get('peer-bio').textContent,'');assert.equal(a.get('members').textContent,'');
});

test('profile route follows observed paths, independently of the peer Tor report',async t=>{
 const a=await setup(t);a.session({...connected,peers:[{...connected.peers[0],tor_enabled:true,route:'direct'}]});a.get('conversation-identity').click();assert.equal(a.get('peer-connection').textContent,'Direct');assert.equal(a.get('peer-name').querySelectorAll('.peer-tor').length,1);
 a.session({...connected,peers:[{...connected.peers[0],tor_enabled:false,route:'relay',latency_ms:93}]});assert.equal(a.get('peer-connection').textContent,'Relayed · 93 ms');assert.equal(a.get('peer-name').querySelectorAll('.peer-tor').length,0);
 a.session({...connected,peers:[{...connected.peers[0],route:'connected'}]});assert.equal(a.get('peer-connection').textContent,'Connected · route unavailable');
 a.session({...connected,peers:[],members:[{...connected.peers[0],online:false,route:'relay'}]});assert.equal(a.get('peer-connection').textContent,'Disconnected');
});

test('bio emoji selection replaces only its selection and saves Unicode separately from chat',async t=>{
 const a=await setup(t);a.session({...connected,bio:'Hello'});a.input('Chat draft stays');const original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='snapshot'?{...connected,bio:'Hello 😀'}:original(c,args);
 a.get('settings-button').click();await flush();a.window.document.querySelector('[data-settings-tab="account"]').click();const input=a.get('profile-bio-input');input.value='Hello world';input.setSelectionRange(6,11);a.get('bio-emoji-button').click();await flush();const emoji=[...a.get('bio-emoji-grid').children].find(b=>b.getAttribute('aria-label')==='😀');assert.ok(emoji.querySelector('img.twemoji'));emoji.click();await flush();assert.equal(input.value,'Hello 😀');assert.equal(a.get('bio-preview-text').textContent,'Hello 😀');assert.equal(a.get('message').value,'Chat draft stays');assert.equal(a.get('bio-emoji-picker').hidden,true);
 a.get('save-bio').click();await flush();assert.ok(a.calls.some(([c,args])=>c==='save_profile_bio'&&args.bio==='Hello 😀'));assert.equal(a.calls.some(([c])=>c==='send_text'),false);
 input.value='x'.repeat(300);input.setSelectionRange(300,300);a.get('bio-emoji-button').click();[...a.get('bio-emoji-grid').children].find(b=>b.getAttribute('aria-label')==='😀').click();assert.equal([...input.value].length,300);assert.match(a.get('toast').textContent,/300/);
 await a.app.lockAccount();assert.equal(a.get('bio-preview-text').textContent,'');assert.equal(a.get('bio-preview-name').textContent,'');assert.equal(a.get('bio-emoji-grid').children.length,0);
});

test('room roster groups presence without replacing member buttons during speaking updates',async t=>{
 const a=await setup(t);const offline={id:'offline',name:'Offline peer',online:false,voice:false};a.session({...channelRoom,members:[...connected.peers,offline]});const member=a.get('members').querySelector('[data-key="peer"] .member-profile');assert.deepEqual([...a.get('members').querySelectorAll('.member-group-title')].map(n=>n.textContent),['Online — 2','Offline — 1']);a.session({...channelRoom,peers:[{...connected.peers[0],speaking:true,voice:true}],members:[...connected.peers,offline]});assert.equal(a.get('members').querySelector('[data-key="peer"] .member-profile'),member);
});
test('offline conversations queue messages while a missing conversation retains the draft',async t=>{
  const a=await setup(t);a.session(base);a.input('My draft');a.submit();assert.match(a.get('toast').textContent,/Choose a conversation/);assert.equal(a.calls.filter(([c])=>c==='send_text').length,0);
  a.session({...connected,peers:[]});assert.equal(a.get('message').disabled,false);assert.equal(a.get('send-button').disabled,false);assert.match(a.get('composer-status').textContent,/outbox/);a.submit();await flush();assert.equal(a.calls.filter(([c])=>c==='send_text').length,1);assert.equal(a.get('message').value,'');
  a.input('Another draft');a.session(connected);assert.equal(a.get('message').value,'Another draft');a.session({...connected,peers:[]});assert.equal(a.get('send-button').disabled,false);
});
test('empty drafts are blocked and successful sends clear the composer',async t=>{
  const a=await setup(t);a.session(connected);assert.equal(a.get('send-button').disabled,true);a.submit();assert.equal(a.calls.filter(([c])=>c==='send_text').length,0);
  a.input('Hello peer');assert.equal(a.get('send-button').disabled,false);a.submit();await flush();assert.equal(a.calls.find(([c])=>c==='send_text')[1].text,'Hello peer');assert.equal(a.get('message').value,'');assert.equal(a.get('send-button').disabled,true);
});
test('typing events carry only a boolean and stop on blur',async t=>{
  const a=await setup(t);a.session(connected);a.input('Secret draft, not transmitted');await flush();a.get('message').dispatchEvent(new a.window.Event('blur'));await flush();const signals=a.calls.filter(([c])=>c==='set_typing').map(([,args])=>args);assert.ok(signals.some(s=>s.active));assert.equal(signals.at(-1).active,false);for(const signal of signals)assert.deepEqual(Object.keys(signal),['active','room','channel']);
});
test('appearance choices apply without changing account content',async t=>{
  const a=await setup(t);a.session(connected);a.app.applyTheme('dark');assert.equal(a.window.document.documentElement.dataset.theme,'dark');a.app.applyTheme('light');assert.equal(a.window.document.documentElement.style.colorScheme,'light');a.app.applyTheme('system');assert.equal(a.window.document.documentElement.dataset.theme,'system');
});
test('incoming calls require an explicit action and can be declined',async t=>{
  const a=await setup(t);a.session({...connected,call:{id:'call',peer:'peer',name:'Peer',status:'incoming'}});assert.equal(a.get('call-dialog').open,true);assert.equal(a.get('accept-call').hidden,false);assert.equal(a.calls.some(([c])=>['accept_call','start_voice'].includes(c)),false);a.get('decline-call').click();await flush();assert.ok(a.calls.some(([c])=>c==='end_call'));a.session(connected);assert.equal(a.get('call-dialog').open,false);
});
test('messages render markup literally and typing labels aggregate peers',async t=>{
  const a=await setup(t);a.session({...connected,peers:[{...connected.peers[0],typing:true}]});assert.equal(a.get('typing-status').textContent,'Peer is typing…');await a.app.handleEvent({type:'message',room:'room',id:'message',peer:'peer',name:'Peer',text:'<img src=x onerror=alert(1)>',emojis:[],timestamp:1});assert.equal(a.get('timeline').querySelector('.bubble').textContent,'<img src=x onerror=alert(1)>');assert.equal(a.get('timeline').querySelector('img'),null);
});
test('all room participants have shared emoji editing controls',async t=>{
  const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'emoji_bank',room:'room',entries:[{id:'emoji',name:'room_cat',hash:'a'.repeat(64),removed:false,revision:1,actor:'b'.repeat(64)}]});const buttons=[...a.get('custom-grid').querySelectorAll('button')];assert.ok(buttons.some(b=>b.textContent==='Rename'&&!b.disabled));assert.ok(buttons.some(b=>b.textContent==='Remove'&&!b.disabled));
});
test('locking removes recovery keys, messages and identities from the UI',async t=>{
  const a=await setup(t);a.session(connected);a.app.showRecovery('ochat-recovery1:SECRET',false);await a.app.handleEvent({type:'message',room:'room',id:'message',peer:'peer',name:'Peer',text:'PRIVATE MESSAGE',emojis:[],timestamp:1});await a.app.lockAccount();assert.equal(a.get('recovery-value').value,'');assert.equal(a.get('identity-value').value,'');assert.equal(a.get('timeline').textContent.includes('PRIVATE MESSAGE'),false);assert.equal(a.get('message').disabled,true);assert.equal(a.get('auth-dialog').open,true);
});

test('late emoji and settings responses cannot reopen private dialogs after locking',async t=>{
  const a=await setup(t);a.session(connected);
  let resolveSettings,resolveEmojis;
  const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>{
    if(command==='get_settings')return new Promise(resolve=>{resolveSettings=resolve;});
    if(command==='room_emojis')return new Promise(resolve=>{resolveEmojis=resolve;});
    return previous(command,args);
  };
  a.get('settings-button').click();a.get('shared-emojis-button').click();
  await a.app.lockAccount();
  resolveSettings(structuredClone(settings));resolveEmojis([{id:'late',name:'private_cat',hash:'a'.repeat(64)}]);await flush();
  assert.equal(a.get('settings-dialog').open,false);assert.equal(a.get('emoji-dialog').open,false);
  assert.equal(a.get('custom-grid').textContent.includes('private_cat'),false);
});

test('offline participants remain visible and background rooms keep their own messages',async t=>{
  const a=await setup(t);a.session({...connected,peers:[],members:[{...connected.peers[0],online:false}]});
  assert.match(a.get('members').textContent,/PeerOffline/);
  assert.equal(a.get('send-button').disabled,true);
  const other={...connected,room:{id:'other',name:'Other room',capacity:4}};
  await a.app.handleEvent({type:'room_state',snapshot:other});
  await a.app.handleEvent({type:'message',room:'other',id:'background',peer:'peer',name:'Peer',text:'BACKGROUND ONLY',emojis:[]});
  assert.equal(a.get('timeline').textContent.includes('BACKGROUND ONLY'),false);
  assert.match(a.get('room-list').textContent,/Other room.*2 online/);assert.equal(a.get('group-room-list').querySelector('.unread-badge').getAttribute('aria-label'),'1 unread messages');
  a.get('new-room').click();assert.equal(a.get('create-dialog').open,true);
  assert.equal(a.calls.some(([c])=>c==='leave_room'),false);
});

test('switching connected rooms keeps separate drafts without leaving either room',async t=>{
  const a=await setup(t);a.session(connected);a.input('Draft for first');
  const other={...connected,room:{id:'other',name:'Other room',capacity:4}};
  const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='select_room'?Promise.resolve(args.id==='other'?other:connected):previous(command,args);
  await a.app.handleEvent({type:'room_state',snapshot:other});
  [...a.get('room-list').querySelectorAll('button')].find(b=>b.querySelector('strong').textContent==='Other room').click();await flush();
  assert.equal(a.get('room-title').textContent,'Other room');assert.equal(a.get('message').value,'');a.input('Draft for second');
  [...a.get('room-list').querySelectorAll('button')].find(b=>b.querySelector('strong').textContent==='Peer').click();await flush();
  assert.equal(a.get('room-title').textContent,'Peer');assert.equal(a.get('message').value,'Draft for first');
  assert.equal(a.calls.some(([c])=>c==='leave_room'),false);
  assert.equal([...a.get('room-list').querySelectorAll('small')].filter(s=>s.textContent.includes('online')).length,2);
});

test('presence dots, shared voice duration and speaking names remain visible before joining',async t=>{
  const a=await setup(t);const started=Date.now()-65000;
  a.session({...connected,voice_started_at:started,peers:[{...connected.peers[0],voice:true,speaking:true}],members:[{...connected.peers[0],online:true,voice:true,speaking:true},{id:'offline',name:'Away',online:false,voice:false}]});
  assert.equal(a.get('voice-bar').hidden,true);assert.match(a.get('room-voice-roster').textContent,/Peer/);
  assert.ok(a.get('room-voice-roster').querySelector('[aria-label="Speaking"]'));
  assert.match(a.get('room-voice-elapsed').textContent,/01:0[5-9]/);
  assert.ok(a.get('members').querySelector('.presence-dot.online'));
  assert.ok(a.get('members').querySelector('.presence-dot.offline'));
});

test('attachment decisions identify each recipient separately',async t=>{
  const a=await setup(t);a.session(connected);
  await a.app.handleEvent({type:'file_offered',room:'room',peer:'self',name:'Test',outgoing:true,offer:{id:'attachment',name:'report.pdf',size:100,hash:'a'.repeat(64)}});
  await a.app.handleEvent({type:'file_decision',room:'room',id:'attachment',peer:'peer',name:'Alice',accepted:true});
  await a.app.handleEvent({type:'file_decision',room:'room',id:'attachment',peer:'other',name:'Bob',accepted:false});
  assert.match(a.get('timeline').textContent,/Alice · Accepted/);assert.match(a.get('timeline').textContent,/Bob · Declined/);
});

test('only the owner sees room deletion and deleted rooms cannot return through stale events',async t=>{
  const a=await setup(t);a.session(connected);assert.equal(a.get('delete-room').hidden,true);
  a.session({...connected,room:{...connected.room,owner:'self'}});assert.equal(a.get('delete-room').hidden,false);
  await a.app.handleEvent({type:'message',room:'room',id:'private',peer:'peer',name:'Peer',text:'DELETED HISTORY',emojis:[]});
  await a.app.handleEvent({type:'room_deleted',room:'room'});
  await a.app.handleEvent({type:'room_state',snapshot:connected});
  await a.app.handleEvent({type:'message',room:'room',id:'late',peer:'peer',name:'Peer',text:'LATE PRIVATE MESSAGE',emojis:[]});
  assert.equal(a.get('timeline').textContent.includes('DELETED HISTORY'),false);assert.equal(a.get('timeline').textContent.includes('LATE PRIVATE MESSAGE'),false);
  assert.equal(a.get('room-list').textContent.includes('Room'),false);assert.equal(a.get('delete-room').hidden,true);
});

test('profile photos appear beside peers and are cleared from the interface on lock',async t=>{
  const a=await setup(t);const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='emoji_asset'?Promise.resolve('data:image/png;base64,cGhvdG8='):previous(command,args);
  a.session({...connected,avatar_hash:'a'.repeat(64),peers:[{...connected.peers[0],avatar_hash:'b'.repeat(64)}]});await flush();
  assert.ok(a.get('profile-avatar').querySelector('img'));assert.ok(a.get('members').querySelector('img'));
  await a.app.lockAccount();assert.equal(a.get('profile-avatar').querySelector('img'),null);assert.equal(a.get('profile-photo-preview').querySelector('img'),null);
});

test('voice disconnect is next to the voice room and stays out of the message composer',async t=>{
  const a=await setup(t);a.session({...connected,room:{...connected.room,capacity:0},voice:true});
  assert.ok(a.get('voice-label').parentElement.contains(a.get('stop-voice')));assert.equal(a.get('composer').contains(a.get('stop-voice')),false);assert.equal(a.get('stop-voice').hidden,false);assert.equal(a.get('voice-button').hidden,true);
  a.get('stop-voice').click();await flush();assert.ok(a.calls.some(([c])=>c==='stop_voice'));
});

test('room creation separates open groups from direct chats without a capacity selector',async t=>{
  const a=await setup(t);a.session(connected);assert.equal(a.get('room-capacity'),null);
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>{const value=await previous(command,args);if(command==='create_room')return 'ochat2:test';if(command==='snapshot')return connected;return value;};
  a.get('new-room').click();assert.equal(a.get('create-room-title').textContent,'New group');a.get('create-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();assert.equal(a.calls.find(([c])=>c==='create_room')[1].direct,false);
  a.get('invite-dialog').close();a.get('new-direct-chat').click();assert.equal(a.get('create-room-title').textContent,'Invite one person');a.get('create-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();assert.equal(a.calls.filter(([c])=>c==='create_room').at(-1)[1].direct,true);
});

test('join notices render names literally and are deduplicated',async t=>{
  const a=await setup(t);a.session(connected);const event={type:'member_joined',room:'room',id:'join',peer:'peer',name:'<b>Peer</b>',timestamp:1};await a.app.handleEvent(event);await a.app.handleEvent(event);assert.equal(a.get('timeline').querySelectorAll('.member-join').length,1);assert.equal(a.get('timeline').querySelector('.member-join').textContent,'<b>Peer</b> joined the room.');assert.equal(a.get('timeline').querySelector('.member-join b'),null);
});

test('mention completion selects a participant and keeps encrypted mention metadata separate from the draft',async t=>{
  const a=await setup(t);a.session(connected);a.get('message').value='Hello @Pe';a.get('message').setSelectionRange(9,9);a.get('message').dispatchEvent(new a.window.Event('input'));assert.equal(a.get('mention-picker').hidden,false);
  a.get('message').dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'Enter',cancelable:true}));assert.equal(a.get('message').value,'Hello @Peer ');assert.equal(a.calls.some(([c])=>c==='send_text'),false);
  a.input('Hello @Peer there');a.submit();await flush();assert.equal(a.calls.find(([c])=>c==='send_text')[1].mentions[0].peer,'peer');
  await a.app.handleEvent({type:'message',room:'room',id:'mention',peer:'peer',name:'Peer',text:'Hello @Test',timestamp:1,emojis:[],mentions:[{peer:'self',name:'Test'}]});assert.equal(a.get('timeline').querySelector('.mention-token').textContent,'@Test');assert.ok(a.get('timeline').querySelector('.mentions-you'));
});

test('unread messages are highlighted and marked read only after focused viewing',async t=>{
  const a=await setup(t);a.session(connected);const id='00000000-0000-4000-8000-000000000001';await a.app.handleEvent({type:'message',room:'room',id,peer:'peer',name:'Peer',text:'Unread',timestamp:1,emojis:[]});assert.ok(a.get('timeline').querySelector('.unread'));assert.ok(a.get('timeline').querySelector('.unread-divider'));assert.equal(a.calls.some(([c])=>c==='mark_read'),false);
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>{const value=await previous(command,args);return command==='mark_read'?{room:args.room,through:args.through,ids:args.ids}:value;};a.window.document.hasFocus=()=>true;a.window.dispatchEvent(new a.window.Event('focus'));await new Promise(resolve=>setTimeout(resolve,2100));assert.equal(a.get('timeline').querySelector('.unread'),null);assert.ok(a.calls.some(([c])=>c==='mark_read'));
});

test('desktop notifications default to direct messages and mentions with private previews',async t=>{
  const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();assert.equal(a.get('desktop-notifications').checked,true);assert.equal(a.get('notification-previews').checked,false);assert.equal(a.get('notification-scope').value,'mentions_and_direct');a.get('notification-previews').checked=true;a.get('settings-save').click();await flush();assert.equal(a.calls.find(([c])=>c==='save_settings')[1].settings.notification_previews,true);
});

test('reply drafts send only a message reference and quotes jump to the original',async t=>{
  const a=await setup(t);a.session(connected);
  const original={type:'message',room:'room',id:'original',peer:'peer',name:'Peer',text:'<b>Original</b>',timestamp:1,emojis:[]};
  await a.app.handleEvent(original);a.get('timeline').querySelector('.reply-action').click();
  assert.equal(a.get('reply-draft').hidden,false);assert.equal(a.get('reply-excerpt').textContent,'<b>Original</b>');assert.equal(a.get('reply-draft').querySelector('b'),null);
  a.input('My answer');a.submit();await flush();const sent=a.calls.find(([c])=>c==='send_text')[1];assert.deepEqual({...sent.replyTo},{id:'original',peer:'peer'});assert.equal(a.get('reply-draft').hidden,true);
  await a.app.handleEvent({...original,id:'answer',peer:'self',reply_to:{id:'original',peer:'peer'},text:'My answer'});
  const quote=a.get('timeline').querySelector('.reply-quote');assert.match(quote.textContent,/Peer<b>Original<\/b>/);quote.click();await flush();assert.equal(a.window.document.activeElement.dataset.messageId,'original');
});

test('history search uses saved results and clears pending private data on lock',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;let resolveSearch;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='search_history'?new Promise(resolve=>{resolveSearch=resolve;}):previous(command,args);
  a.get('search-button').click();a.get('search-query').value='private';a.get('search-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));
  await a.app.lockAccount();resolveSearch({offset:0,total:1,entries:[{room:'room',timestamp:1,event:{peer:'peer',id:'found',name:'Peer',text:'PRIVATE SEARCH RESULT'}}]});await flush();
  assert.equal(a.get('search-results').textContent,'');assert.equal(a.get('search-query').value,'');assert.equal(a.get('search-dialog').open,false);
});

test('search pages navigate to older messages and can return to the latest history',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke,queries=[];
  const history=Array.from({length:600},(_,index)=>({room:'room',timestamp:index+1,event:{type:'message',room:'room',id:'m'+index,peer:'peer',name:'Peer',text:index===10?'<b>Found</b>':'History '+index,timestamp:index+1,emojis:[]}}));
  a.window.__TAURI__.core.invoke=(command,args)=>{
    if(command==='search_history'){queries.push(args);return Promise.resolve({offset:args.offset,total:51,entries:[history[10]]});}
    if(command==='history')return Promise.resolve(history);
    return previous(command,args);
  };
  a.get('search-button').click();a.get('search-query').value='Found';a.get('search-scope').value='all';a.get('search-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();
  assert.equal(queries[0].room,null);assert.equal(a.get('search-next').hidden,false);assert.equal(a.get('search-results').querySelector('b'),null);
  a.get('search-next').click();await flush();assert.equal(queries.at(-1).offset,50);
  a.get('search-results').querySelector('button').click();await flush();assert.equal(a.window.document.activeElement.dataset.messageId,'m10');assert.equal(a.get('history-context').hidden,false);
  a.get('latest-messages').click();await flush();assert.equal(a.get('history-context').hidden,true);assert.equal(a.get('timeline').querySelector('[data-message-id="m10"]'),null);assert.ok(a.get('timeline').querySelector('[data-message-id="m599"]'));
});

test('room notification preferences preserve other rooms and support mute and private previews',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='get_settings'?Promise.resolve({...structuredClone(settings),room_notifications:{other:{scope:'all'}}}):previous(command,args);
  a.get('room-notifications-button').click();await flush();assert.equal(a.get('room-notification-scope').value,'inherit');
  a.get('room-notification-scope').value='muted';a.get('room-notification-preview').value='off';a.get('room-notifications-save').click();await flush();
  const saved=a.calls.find(([c])=>c==='save_settings')[1].settings;assert.equal(saved.room_notifications.room.scope,'muted');assert.equal(saved.room_notifications.room.previews,false);assert.equal(saved.room_notifications.other.scope,'all');
});

test('push-to-talk releases on key-up, focus loss and mute and ignores text entry',async t=>{
  const a=await setup(t);a.app.session({...connected,voice:true},{...structuredClone(settings),audio:{...settings.audio,push_to_talk:true,talk_key:'Space'}});
  assert.equal(a.get('talk-button').hidden,false);a.get('message').focus();a.get('message').dispatchEvent(new a.window.KeyboardEvent('keydown',{code:'Space',key:' ',bubbles:true}));assert.equal(a.calls.some(([c])=>c==='hold_to_talk'),false);
  a.get('room-title').dispatchEvent(new a.window.KeyboardEvent('keydown',{code:'Space',key:' ',bubbles:true,cancelable:true}));assert.equal(a.calls.filter(([c])=>c==='hold_to_talk').at(-1)[1].pressed,true);
  a.window.document.dispatchEvent(new a.window.KeyboardEvent('keyup',{code:'Space',key:' ',bubbles:true}));assert.equal(a.calls.filter(([c])=>c==='hold_to_talk').at(-1)[1].pressed,false);
  a.get('talk-button').dispatchEvent(new a.window.PointerEvent('pointerdown',{button:0,pointerId:1}));a.window.dispatchEvent(new a.window.Event('blur'));assert.equal(a.get('talk-button').getAttribute('aria-pressed'),'false');
  a.get('talk-button').dispatchEvent(new a.window.PointerEvent('pointerdown',{button:0,pointerId:1}));a.get('mute-button').click();await flush();assert.equal(a.calls.filter(([c])=>c==='hold_to_talk').at(-1)[1].pressed,false);
});

test('speaking shows a green dot and saved rooms have a prominent reconnect action',async t=>{
  const a=await setup(t);a.session({...connected,peers:[{...connected.peers[0],voice:true,speaking:true}]});assert.ok(a.get('room-voice-roster').querySelector('.speaking-dot[aria-label="Speaking"]'));
  const other={...connected,room:{id:'saved',name:'Saved',capacity:0}};a.session(other);await a.app.handleEvent({type:'room_disconnected',room:'saved'});await a.app.handleEvent({type:'state',snapshot:base});
  assert.equal(a.get('reconnect-panel').hidden,false);assert.equal(a.get('reconnect-button').textContent,'Reconnect to room');assert.ok(a.get('reconnect-panel').contains(a.get('reconnect-button')));assert.equal(a.get('reconnect-button').closest('.header-actions'),null);
});

test('friends can be requested from a participant and messaged without a room creation form',async t=>{
  const a=await setup(t);a.session(connected);let friendList=[];const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='list_friends')return friendList;if(command==='add_friend'){friendList=[{peer:'peer',name:'Peer',status:'outgoing'}];}if(command==='open_direct')return 'room';return previous(command,args);};
  a.get('members').querySelector('[data-key=peer] .friend-action').click();await flush();assert.match(a.get('friend-list').textContent,/Waiting to reach this contact/);
  friendList=[{peer:'peer',name:'Peer',status:'accepted',direct_room:null}];await a.app.handleEvent({type:'friends_changed'});
  [...a.get('friend-list').querySelectorAll('button')].find(button=>button.textContent==='Message').click();await flush();
  assert.equal(a.get('create-dialog').open,false);assert.equal(a.calls.some(([c])=>c==='create_room'),false);
  await a.app.lockAccount();assert.equal(a.get('friend-list').textContent,'');
});

test('the persistent dock targets background voice and push-to-talk after switching chats',async t=>{
  const a=await setup(t),voice={...connected,voice:true,room:{id:'voice-room',name:'Workshop',capacity:0},voice_started_at:Date.now()-65000,peers:[{...connected.peers[0],voice:true,speaking:true}]};
  a.app.session(connected,{...structuredClone(settings),audio:{...settings.audio,push_to_talk:true,talk_key:'Space'}});
  await a.app.handleEvent({type:'room_state',snapshot:voice});assert.equal(a.get('voice-bar').hidden,false);assert.equal(a.get('voice-label').textContent,'Workshop');assert.match(a.get('voice-elapsed').textContent,/01:0[5-9]/);
  a.get('room-title').dispatchEvent(new a.window.KeyboardEvent('keydown',{code:'Space',key:' ',bubbles:true,cancelable:true}));assert.equal(a.calls.filter(([c])=>c==='hold_to_talk').at(-1)[1].room,'voice-room');
  a.get('mute-button').click();await flush();assert.deepEqual({...a.calls.find(([c])=>c==='set_muted')[1]},{room:'voice-room',muted:true});
  a.get('stop-voice').click();await flush();assert.equal(a.calls.find(([c])=>c==='stop_voice')[1].room,'voice-room');
  await a.app.handleEvent({type:'room_state',snapshot:{...voice,voice:false,peers:[]}});assert.equal(a.get('voice-bar').hidden,true);
});

test('voice and participant elements survive speaking updates without losing focus',async t=>{
  const a=await setup(t),snapshot={...connected,voice:true,peers:[{...connected.peers[0],voice:true,speaking:true}],members:[{...connected.peers[0],online:true,voice:true,speaking:true}]};a.session(snapshot);
  const member=a.get('members').querySelector('[data-key="peer"]'),button=member.querySelector('.member-profile'),dot=member.querySelector('.speaking-dot'),voice=a.get('voice-roster').querySelector('[data-key="peer"]'),voiceDot=voice.querySelector('.speaking-dot');button.focus();
  await a.app.handleEvent({type:'state',snapshot:{...snapshot,peers:[{...snapshot.peers[0],speaking:false}],members:[{...snapshot.members[0],speaking:false}]}});
  assert.equal(a.get('members').querySelector('[data-key="peer"]'),member);assert.equal(a.window.document.activeElement,button);assert.equal(member.querySelector('.speaking-dot'),dot);assert.equal(dot.hidden,true);assert.equal(voice.querySelector('.speaking-dot'),voiceDot);assert.equal(voiceDot.hidden,true);
  await a.app.handleEvent({type:'state',snapshot});assert.equal(dot.hidden,false);assert.equal(voiceDot.hidden,false);
});

test('conversation tabs remain stable and support keyboard navigation',async t=>{
  const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'another',name:'Another',capacity:0}}});
  const tabs=[...a.get('conversation-tabs').querySelectorAll('[role="tab"]')],selected=tabs.find(tab=>tab.getAttribute('aria-selected')==='true');selected.focus();
  await a.app.handleEvent({type:'state',snapshot:{...connected,speaking:true}});assert.equal(a.window.document.activeElement,selected);
  a.get('conversation-tabs').dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'Home',bubbles:true,cancelable:true}));assert.equal(a.window.document.activeElement,tabs[0]);
});

test('narrow panels are exclusive and resizing preserves drafts and reply state',async t=>{
  const a=await setup(t);a.session(connected);Object.defineProperty(a.window,'innerWidth',{value:480,configurable:true});
  a.get('navigation-toggle').click();assert.equal(a.get('navigation-dialog').open,true);a.get('details-toggle').click();assert.equal(a.get('navigation-dialog').open,false);assert.equal(a.get('details-dialog').open,true);
  a.get('details-dialog').querySelector('.close-dialog').click();await flush();assert.equal(a.window.document.activeElement,a.get('details-toggle'));
  await a.app.handleEvent({type:'message',room:'room',peer:'peer',name:'Peer',id:'source',text:'Original',emojis:[],timestamp:1});a.get('timeline').querySelector('.reply-action').click();a.input('Kept draft');a.get('details-toggle').click();
  Object.defineProperty(a.window,'innerWidth',{value:1180,configurable:true});a.window.dispatchEvent(new a.window.Event('resize'));await flush();assert.equal(a.get('details-dialog').open,true);assert.equal(a.get('details-toggle').getAttribute('aria-expanded'),'true');assert.equal(a.get('message').value,'Kept draft');assert.equal(a.get('reply-draft').hidden,false);
  a.get('settings-button').click();await flush();Object.defineProperty(a.window,'innerWidth',{value:480,configurable:true});a.window.dispatchEvent(new a.window.Event('resize'));assert.equal(a.get('settings-dialog').open,true);assert.equal(a.get('details-dialog').open,false);
  await a.app.lockAccount();assert.equal(a.get('details-dialog').open,false);assert.equal(a.get('conversation-tabs').querySelectorAll('.conversation-tab-entry').length,0);assert.equal(a.get('room-voice-roster').textContent,'');
});

test('custom accent previews cancel cleanly and validation prevents invalid colors',async t=>{
  const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
  const initial=a.window.document.documentElement.style.getPropertyValue('--accent');
  a.get('accent-hex').value='#FF00AA';a.get('accent-hex').dispatchEvent(new a.window.Event('input'));assert.notEqual(a.window.document.documentElement.style.getPropertyValue('--accent'),initial);
  a.get('accent-hex').value='invalid';a.get('accent-hex').dispatchEvent(new a.window.Event('input'));assert.equal(a.get('settings-save').disabled,true);
  a.get('settings-cancel').click();a.get('settings-button').click();await flush();assert.equal(a.get('settings-save').disabled,false);assert.equal(a.get('accent-hex').value,'#007AFF');
  a.get('accent-hex').value='#ABCDEF';a.get('accent-hex').dispatchEvent(new a.window.Event('input'));a.get('settings-save').click();await flush();assert.equal(a.calls.find(([c])=>c==='save_settings')[1].settings.accent_color,'#ABCDEF');
});

test('derived accent and button colors retain contrast for extreme and saturated choices',async t=>{
  const a=await setup(t);
  for(const dark of [false,true])for(const color of ['#FFFFFF','#000000','#00FF00','#FF0000','#0000FF','#FFFF00','#808080','#007AFF','#ABCDEF']){
    const tokens=a.app.accentTokens(color,dark),accent=a.app.parseColor(tokens.accent),foreground=a.app.parseColor(tokens.foreground);
    for(const surface of dark?['#313338','#2b2d31','#383a40','#2b2d31']:['#ffffff','#f2f3f5','#e3e5e8','#f2f3f5']){const background=a.app.parseColor(surface);assert.ok(a.app.contrast(accent,background)>=4.5,`${color} on ${surface}`);assert.ok(a.app.contrast(accent,a.app.mixColor(background,accent,dark?.18:.12))>=4.5);}
    assert.ok(a.app.contrast(accent,foreground)>=4.5);assert.ok(a.app.contrast(a.app.parseColor(tokens.hover),foreground)>=4.5);
  }
});

test('private chats identify the recipient in the header, tabs, composer and profile',async t=>{
  const a=await setup(t);a.session(connected);
  assert.equal(a.get('room-title').textContent,'Peer');
  assert.equal(a.get('conversation-kind').textContent,'Private chat');
  assert.equal(a.get('message').placeholder,'Message Peer…');
  assert.equal(a.get('composer-status').textContent,'To Peer · Private chat');
  assert.equal(a.get('conversation-tabs').querySelector('strong').textContent,'Peer');
  assert.equal(a.get('voice-button').closest('header').className,'topbar');
  a.get('conversation-identity').focus();a.get('conversation-identity').click();
  assert.equal(a.get('peer-name').textContent,'Peer');assert.equal(a.get('peer-presence').textContent,'Online');
  assert.equal(a.get('peer-connection').textContent,'Direct');
  assert.equal(a.get('participants-section').hidden,true);
  a.get('peer-copy').click();await flush();assert.equal(a.calls.find(([c])=>c==='copy_text')[1].text,'peer');
  assert.equal(a.calls.some(([c])=>['call_peer','start_voice','accept_call'].includes(c)),false);
  a.get('details-dialog').close();await flush();assert.equal(a.window.document.activeElement,a.get('conversation-identity'));
});

test('sender profiles distinguish identical names by identity and never interpret markup',async t=>{
  const a=await setup(t),name='<Same name>',group={...connected,room:{...connected.room,capacity:0},peers:[{...connected.peers[0],id:'one',name},{...connected.peers[0],id:'two',name}]};a.session(group);
  for(const peer of group.peers)await a.app.handleEvent({type:'message',room:'room',id:peer.id,peer:peer.id,name,text:'Hello',emojis:[],timestamp:1});
  const authors=a.get('timeline').querySelectorAll('.message-author');authors[1].click();
  assert.equal(a.get('peer-name').textContent,name);assert.equal(a.get('peer-identity').textContent,'two');assert.equal(a.get('peer-card').querySelector('same'),null);
  a.get('peer-add').click();await flush();assert.equal(a.calls.find(([c])=>c==='add_friend')[1].peer,'two');
  assert.equal(a.get('floating-profile').open,true);assert.equal(a.get('details-overview').hidden,false);a.get('floating-profile-close').click();assert.equal(a.get('peer-card').hidden,true);assert.equal(a.get('details-overview').hidden,false);
  authors[0].click();assert.equal(a.get('peer-identity').textContent,'one');
});

test('saved direct chats retain recipient details without claiming online presence',async t=>{
  const a=await setup(t);a.session(connected);
  const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='list_friends'?Promise.resolve([{peer:'peer',name:'Peer',direct_room:'room',status:'accepted',online:true}]):previous(command,args);
  await a.app.handleEvent({type:'friends_changed'});
  await a.app.handleEvent({type:'room_disconnected',room:'room'});await a.app.handleEvent({type:'state',snapshot:base});
  assert.equal(a.get('room-title').textContent,'Peer');assert.match(a.get('room-subtitle').textContent,/Disconnected/);
  a.get('conversation-identity').click();assert.equal(a.get('peer-presence').textContent,'Offline');assert.equal(a.get('peer-relationship').textContent,'Friend');
  assert.equal(a.get('peer-message').hidden,true);assert.equal(a.get('voice-button').disabled,true);
  assert.equal(a.get('reconnect-button').hidden,false);a.input('Kept until connected');assert.equal(a.get('send-button').disabled,false);
});

test('accepted friends show their photo and live presence without any room or sent message',async t=>{
 const a=await setup(t);a.session(base);const previous=a.window.__TAURI__.core.invoke;
 let friend={peer:'new-friend',name:'New friend',status:'accepted',direct_room:null,profile_known:true,presence_online:true,online:true,avatar_hash:'a'.repeat(64),bio:'A profile without a chat',route:'direct',tor_enabled:false};
 a.window.__TAURI__.core.invoke=async(command,args)=>command==='list_friends'?[friend]:command==='emoji_asset'?'data:image/png;base64,cGhvdG8=':previous(command,args);
 await a.app.handleEvent({type:'friends_changed'});await flush();
 const row=a.get('friend-list').querySelector('[data-peer="new-friend"]');assert.ok(row.querySelector('.presence-dot.online'));assert.ok(row.querySelector('.avatar img'));
 row.querySelector('.friend-heading').click();assert.equal(a.get('peer-presence').textContent,'Online');assert.equal(a.get('peer-bio').textContent,friend.bio);assert.equal(a.get('peer-connection').textContent,'Direct');
 assert.equal(a.calls.some(([command])=>['open_direct','send_text','create_room','join_room'].includes(command)),false);
 friend={...friend,presence_online:false,online:false,route:null,tor_enabled:null};await a.app.handleEvent({type:'friends_changed'});await flush();
 assert.ok(a.get('friend-list').querySelector('.presence-dot.offline'));assert.ok(a.get('friend-list').querySelector('.avatar img'));assert.equal(a.get('peer-presence').textContent,'Offline');
 friend={...friend,presence_online:true,online:true,avatar_hash:null,bio:'',route:'relay'};await a.app.handleEvent({type:'friends_changed'});await flush();
 assert.equal(a.get('friend-list').querySelector('.avatar img'),null);assert.equal(a.get('peer-bio').textContent,'No bio yet.');assert.equal(a.get('peer-connection').textContent,'Relayed');
 await a.app.lockAccount();assert.equal(a.get('peer-bio').textContent,'');assert.equal(a.get('friend-list').textContent,'');
});

test('profile voice state follows the active speaker in another conversation',async t=>{
  const a=await setup(t);a.session(connected);
  const voice={...connected,room:{id:'voice',name:'Workshop',capacity:0},voice:true,peers:[{...connected.peers[0],voice:true,speaking:true}]};
  await a.app.handleEvent({type:'room_state',snapshot:voice});
  a.get('voice-roster').querySelector('[data-key="peer"]').click();assert.equal(a.get('peer-name').textContent,'Peer');assert.equal(a.get('peer-voice').textContent,'Speaking now');
  assert.equal(a.get('voice-button').disabled,true);assert.match(a.get('voice-button').title,/Workshop/);
  await a.app.handleEvent({type:'room_state',snapshot:{...voice,peers:[{...voice.peers[0],speaking:false}]}});
  assert.equal(a.get('peer-voice').textContent,'In a voice chat');assert.equal(a.get('peer-voice').dataset.speaking,'false');
  assert.equal(a.calls.some(([c])=>['start_voice','call_peer','select_room'].includes(c)),false);
});

test('desktop profiles open once without stealing composer focus or reopening after dismissal',async t=>{
  const a=await setup(t);a.session(base);a.get('message').focus();a.input('Draft');
  Object.defineProperty(a.window,'innerWidth',{value:1600,configurable:true});a.session(connected);
  assert.equal(a.get('details-dialog').open,true);assert.equal(a.window.document.activeElement,a.get('message'));
  a.get('details-dialog').close();await flush();a.app.render({...connected,peers:[{...connected.peers[0],typing:true}]});
  assert.equal(a.get('details-dialog').open,false);assert.equal(a.get('message').value,'Draft');
});

test('profile and recipient data are removed from text, attributes and actions on lock',async t=>{
  const a=await setup(t),peer={...connected.peers[0],id:'private-peer-id',name:'PrivateRecipient'};a.session({...connected,peers:[peer]});a.get('conversation-identity').click();
  assert.equal(a.get('peer-identity').textContent,peer.id);await a.app.lockAccount();
  assert.equal(a.get('peer-identity').textContent,'');assert.equal(a.get('peer-copy').onclick,null);
  assert.equal(a.window.document.body.innerHTML.includes('PrivateRecipient'),false);assert.equal(a.window.document.body.innerHTML.includes('private-peer-id'),false);
});

test('unknown private recipients do not inherit an unrelated room participant',async t=>{
  const a=await setup(t);a.session({...connected,peers:[],room:{...connected.room,name:'Invitation for a colleague'}});
  assert.equal(a.get('room-title').textContent,'Invitation for a colleague');assert.equal(a.get('room-subtitle').textContent,'Waiting for the other participant');
  a.get('details-toggle').click();assert.equal(a.get('peer-card').hidden,true);assert.equal(a.get('voice-button').disabled,true);
});

test('tab deletion confirms the named background conversation and keeps the current draft',async t=>{
  const a=await setup(t);a.session(connected);a.input('Keep this draft');
  const other={...connected,room:{id:'other',name:'Other group',capacity:0}};
  await a.app.handleEvent({type:'room_state',snapshot:other});
  const remove=a.get('conversation-tabs').querySelector('[data-key="other"] .tab-delete');remove.click();
  assert.match(a.get('confirm-title').textContent,/Other group/);assert.match(a.get('confirm-description').textContent,/Other participants keep their copies/);
  a.get('confirm-dialog').querySelector('.close-dialog').click();await flush();assert.equal(a.calls.some(([c])=>c==='forget_room'),false);
  remove.click();a.get('confirm-action').click();await flush();
  assert.deepEqual({...a.calls.find(([c])=>c==='forget_room')[1]},{room:'other'});
  assert.equal(a.calls.some(([c])=>c==='delete_room'),false);
  assert.equal(a.get('conversation-tabs').querySelector('[data-key="other"]'),null);assert.equal(a.get('message').value,'Keep this draft');assert.equal(a.get('room-title').textContent,'Peer');
  await a.app.handleEvent({type:'room_state',snapshot:other});await a.app.handleEvent({type:'message',room:'other',id:'late',peer:'peer',name:'Peer',text:'Late message',timestamp:1});
  assert.equal(a.get('conversation-tabs').querySelector('[data-key="other"]'),null);
});

test('deleting the selected chat opens the remaining chat without stopping its voice dock',async t=>{
  const a=await setup(t);a.session(connected);
  const voice={...connected,room:{id:'voice',name:'Workshop',capacity:0},voice:true};
  await a.app.handleEvent({type:'room_state',snapshot:voice});const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='select_room'?Promise.resolve(voice):previous(command,args);
  a.get('forget-room').click();a.get('confirm-action').click();await flush();await flush();
  assert.equal(a.get('room-title').textContent,'Workshop');assert.equal(a.get('voice-bar').hidden,false);
  assert.equal(a.get('conversation-tabs').querySelector('[data-key="room"]'),null);
  assert.equal(a.calls.some(([c])=>['stop_voice','end_call'].includes(c)),false);
});

test('failed local deletion preserves the tab and deleting the last chat clears its profile',async t=>{
  const a=await setup(t);a.session(connected);a.get('conversation-identity').click();a.input('Unsent');
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=(command,args)=>command==='forget_room'?Promise.reject(Error('Storage unavailable')):previous(command,args);
  a.get('forget-room').click();a.get('confirm-action').click();await flush();
  assert.ok(a.get('conversation-tabs').querySelector('[data-key="room"]'));assert.equal(a.get('message').value,'Unsent');assert.match(a.get('toast').textContent,/Storage unavailable/);
  a.window.__TAURI__.core.invoke=previous;a.get('forget-room').click();a.get('confirm-action').click();await flush();
  assert.equal(a.get('conversation-tabs').querySelector('[role="tab"]'),null);assert.equal(a.get('details-dialog').open,false);assert.equal(a.get('peer-name').textContent,'');assert.equal(a.get('message').value,'');assert.ok(a.get('welcome'));
});

test('rail and navigation precede the conversation; members follow it in document order',async t=>{
  const a=await setup(t);assert.deepEqual([...a.window.document.querySelector('.workspace').children].map(node=>node.id),['conversation-rail','navigation-dialog','conversation-panel','details-dialog']);
});

test('an explicit successful invitation can reopen a locally deleted tab with empty history',async t=>{
  const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'room_forgotten',room:'room'});
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=(command,args)=>command==='snapshot'?Promise.resolve(connected):command==='list_rooms'?Promise.resolve([connected.room]):previous(command,args);
  a.get('join-button').click();a.get('invitation-input').value='ochat2:valid';a.get('join-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();await flush();
  assert.ok(a.get('conversation-tabs').querySelector('[data-key="room"]'));assert.equal(a.get('room-title').textContent,'Peer');assert.equal(a.get('timeline').querySelector('.message'),null);
});

test('pins stay first and keyboard reordering persists without changing drafts or voice',async t=>{
  const a=await setup(t);a.session(connected);a.input('Keep this draft');
  const other={...connected,room:{id:'other',name:'Another',capacity:2},voice:true};
  await a.app.handleEvent({type:'room_state',snapshot:other});
  const strip=a.get('private-conversation-tabs'),ids=()=>[...strip.children].map(node=>node.dataset.key);
  assert.deepEqual(ids(),['other','room']);
  const original=strip.querySelector('[data-key="room"]');
  original.querySelector('.tab-pin').click();await flush();
  assert.deepEqual(ids(),['room','other']);assert.equal(original.querySelector('.tab-pin').getAttribute('aria-pressed'),'true');
  assert.deepEqual(Array.from(a.calls.filter(([c])=>c==='save_conversation_layout').at(-1)[1].layout.pinned),['room']);
  original.querySelector('.tab-pin').click();await flush();
  original.querySelector('[role="tab"]').focus();
  original.querySelector('[role="tab"]').dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'ArrowRight',altKey:true,bubbles:true,cancelable:true}));await flush();
  assert.deepEqual(ids(),['other','room']);assert.equal(strip.lastElementChild,original);
  assert.equal(a.window.document.activeElement,original.querySelector('[role="tab"]'));
  assert.equal(a.get('message').value,'Keep this draft');assert.match(a.get('voice-label').textContent,/Peer/);
  assert.equal(a.calls.some(([c])=>['stop_voice','select_room','save_settings'].includes(c)),false);
});

test('drag reorders within its section and rejects crossing pinned conversations',async t=>{
  const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'other',name:'Another',capacity:2}}});
  const strip=a.get('conversation-tabs'),source=strip.querySelector('[data-key="room"]'),target=strip.querySelector('[data-key="other"]');
  target.getBoundingClientRect=()=>({left:0,width:60,top:0,height:54});const transfer={setData(){}};
  source._tab.ondragstart({clientX:0,clientY:0,dataTransfer:transfer});target.ondragover({clientX:10,clientY:10,dataTransfer:transfer,preventDefault(){}});
  assert.equal(target.dataset.drop,'before');target.ondrop({preventDefault(){}});await flush();
  assert.equal(a.get('private-conversation-tabs').firstElementChild,source);assert.equal(source.classList.contains('dragging'),false);
  source._pin.click();await flush();const saves=a.calls.filter(([c])=>c==='save_conversation_layout').length;
  source._tab.ondragstart({clientX:0,dataTransfer:transfer});target.ondragover({clientX:10,clientY:50,dataTransfer:transfer,preventDefault(){}});target.ondrop({preventDefault(){}});await flush();
  assert.equal(transfer.dropEffect,'none');assert.equal(a.calls.filter(([c])=>c==='save_conversation_layout').length,saves);
});

test('failed tab persistence rolls back and locking clears layout and pending feedback',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='save_conversation_layout'?Promise.reject(Error('Disk full')):previous(command,args);
  a.get('conversation-tabs').querySelector('.tab-pin').click();await flush();
  assert.equal(a.get('conversation-tabs').querySelector('.tab-pin').getAttribute('aria-pressed'),'false');assert.match(a.get('toast').textContent,/Could not save conversation order/);
  await a.app.lockAccount();assert.equal(a.app.allRooms().length,0);assert.equal(a.get('conversation-tabs').querySelectorAll('.conversation-tab-entry').length,0);
});

test('unopened tabs pulse once for live remote messages and stop on focused opening',async t=>{
  const a=await setup(t);a.session(connected);a.window.document.hasFocus=()=>true;
  await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'other',name:'Another',capacity:0}}});
  const event={type:'message',room:'other',id:'new',peer:'peer',name:'Peer',text:'Hello',emojis:[],timestamp:Date.now()};
  await a.app.handleEvent(event);await a.app.handleEvent(event);
  const other=a.get('conversation-tabs').querySelector('[data-key="other"]');
  assert.ok(other.classList.contains('needs-attention'));assert.equal(other._tab._badge.textContent,'1');assert.equal(other._tab._badge.getAttribute('aria-label'),'1 unread messages');
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=(command,args)=>command==='select_room'?Promise.resolve({...connected,room:{id:'other',name:'Another',capacity:0}}):previous(command,args);
  other._tab.click();await flush();a.app.acknowledgeConversation();
  assert.equal(other.classList.contains('needs-attention'),false);
  await a.app.handleEvent({...event,id:'own',peer:'self'});assert.equal(other.classList.contains('needs-attention'),false);
  await a.app.handleEvent({...event,id:'visible'});assert.equal(other.classList.contains('needs-attention'),false);
});

test('direct tabs use the remote profile photo and groups expose illustration controls only to owners',async t=>{
  const a=await setup(t),hash='a'.repeat(64),photo='data:image/png;base64,AAAA';const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='emoji_asset'?Promise.resolve(photo):previous(command,args);
  a.session({...connected,peers:[{...connected.peers[0],avatar_hash:hash}]});await flush();
  assert.equal(a.get('conversation-tabs').querySelector('.tab-avatar img').src,photo);
  assert.equal(a.get('room-illustration-section').hidden,true);
  a.session({...connected,room:{id:'group',name:'Design',capacity:0,owner:'self',image_hash:hash}});await flush();
  assert.equal(a.get('room-illustration-section').hidden,false);assert.equal(a.get('room-illustration-actions').hidden,false);
  assert.equal(a.get('room-illustration').querySelector('img').src,photo);
  a.get('choose-room-illustration').click();await flush();assert.deepEqual(structuredClone(a.calls.find(([c])=>c==='choose_photo_source')[1]),{profile:false});
  a.session({...connected,room:{id:'group',name:'Design',capacity:0,owner:'peer'}});
  assert.equal(a.get('room-illustration-actions').hidden,true);
});

test('HTTP links are safe literal anchors and preserve emoji-like URL segments and trailing punctuation',async t=>{
  const a=await setup(t);a.session(connected);
  await a.app.handleEvent({type:'message',room:'room',id:'links',peer:'peer',name:'Peer',text:'See (https://example.com/a_(b)). http://example.org/:test:?q=@Peer javascript:alert(1) <script>',emojis:[],timestamp:1});
  const links=[...a.get('timeline').querySelectorAll('.bubble a')];assert.equal(links.length,2);
  assert.equal(links[0].href,'https://example.com/a_(b)');assert.equal(links[1].href,'http://example.org/:test:?q=@Peer');
  assert.equal(a.get('timeline').querySelector('script'),null);assert.equal(links[0].rel,'noopener noreferrer');
  links[0].click();await flush();assert.deepEqual(structuredClone(a.calls.find(([c])=>c==='open_link')[1]),{url:'https://example.com/a_(b)'});
  assert.equal(a.app.messageLinks('file:///test https://user:pass@example.com javascript:x').length,0);
});

test('preview metadata stays literal, duplicate URLs share a request, and late previews cannot survive locking',async t=>{
  const a=await setup(t);a.session(connected);let resolvePreview;const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='link_preview'?new Promise(resolve=>{resolvePreview=resolve;}):previous(command,args);
  for(const id of ['one','two'])await a.app.handleEvent({type:'message',room:'room',id,peer:'peer',name:'Peer',text:'https://example.com',emojis:[],timestamp:1});
  const cards=[...a.get('timeline').querySelectorAll('.link-preview')];cards.forEach(card=>card.querySelector('button').click());await flush();
  resolvePreview({title:'<img src=x onerror=x>',description:'<script>bad</script>',image:'https://tracker.example/image.png'});await flush();
  assert.equal(cards[0].querySelector('.link-title').textContent,'<img src=x onerror=x>');assert.equal(cards[0].querySelector('img'),null);assert.equal(cards[0].querySelector('script'),null);
  await a.app.handleEvent({type:'message',room:'room',id:'late',peer:'peer',name:'Peer',text:'https://example.org',emojis:[],timestamp:1});
  const late=a.get('timeline').querySelectorAll('.link-preview')[2];late.querySelector('button').click();await flush();await a.app.lockAccount();
  resolvePreview({title:'PRIVATE PREVIEW',description:'Private',image:null});await flush();assert.equal(a.window.document.body.textContent.includes('PRIVATE PREVIEW'),false);
});

test('queued layout changes save in order and a later failure retains the last successful pin',async t=>{
  const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'other',name:'Another',capacity:0}}});
  let resolveFirst;let saves=0;const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>{
    if(command!=='save_conversation_layout')return previous(command,args);
    saves++;if(saves===1)return new Promise(resolve=>{resolveFirst=()=>resolve(args.layout);});return Promise.reject(Error('Disk full'));
  };
  a.get('conversation-tabs').querySelector('[data-key="room"] .tab-pin').click();await flush();
  a.get('conversation-tabs').querySelector('[data-key="other"] .tab-pin').click();await flush();assert.equal(saves,1);
  resolveFirst();await flush();await flush();
  const pinned=[...a.get('conversation-tabs').querySelectorAll('.pinned')].map(node=>node.dataset.key);assert.deepEqual(pinned,['room']);assert.equal(saves,2);
});

test('voice cue switches and automatic link previews are independent persisted preferences',async t=>{
  const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
  a.get('voice-join-sound').checked=false;a.get('voice-leave-sound').checked=true;a.get('link-previews').checked=false;a.get('settings-save').click();await flush();
  const saved=a.calls.find(([command])=>command==='save_settings')[1].settings;
  assert.equal(saved.voice_join_sound,false);assert.equal(saved.voice_leave_sound,true);assert.equal(saved.link_previews,false);assert.equal(saved.message_sound,true);
});

test('saved private tabs recover the offline recipient photo before opening the chat',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;
  await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'saved',name:'Old invitation',capacity:2}}});
  await a.app.handleEvent({type:'room_disconnected',room:'saved'});
  // A saved room without a cached roster is the account-startup case.
  const photo='data:image/png;base64,AAAA';a.window.__TAURI__.core.invoke=(command,args)=>command==='room_participants'?Promise.resolve([{id:'offline',name:'Morgan',avatar_hash:'c'.repeat(64)}]):command==='emoji_asset'?Promise.resolve(photo):previous(command,args);
  a.app.session({...base,room:null},structuredClone(settings));
  a.app.archive([{id:'saved',name:'Old invitation',capacity:2}]);
  await a.app.hydrateRoomMembers();await flush();
  const tab=a.get('conversation-tabs').querySelector('[data-key="saved"]');assert.match(tab.textContent,/Morgan/);assert.equal(tab.querySelector('.tab-avatar img').src,photo);
  assert.equal(a.calls.some(([command])=>command==='select_room'),false);
});

test('desktop navigation stays open and separates private chats from joined rooms',async t=>{
  const a=await setup(t);Object.defineProperty(a.window,'innerWidth',{value:1180,configurable:true});a.session(connected);a.input('Keep my draft');
  await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'group',name:'Workshop',capacity:0},voice:true}});
  assert.equal(a.get('navigation-dialog').open,true);assert.equal(a.get('direct-count').textContent,'1');assert.equal(a.get('group-count').textContent,'1');
  assert.match(a.get('direct-room-list').textContent,/Peer/);assert.match(a.get('group-room-list').textContent,/Workshop/);
  a.get('room-actions-toggle').click();assert.equal(a.get('navigation-dialog').open,true);a.get('room-actions-dialog').close();
  a.get('group-room-list').querySelector('button').click();await flush();
  assert.equal(a.get('navigation-dialog').open,true);assert.equal(a.calls.some(([c])=>c==='stop_voice'),false);
});

test('the visible Friends entry opens requests directly and is cleared on lock',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='list_friends'?Promise.resolve([{peer:'incoming',name:'Morgan',status:'incoming'}]):previous(command,args);
  await a.app.handleEvent({type:'friends_changed'});assert.equal(a.get('friends-button').textContent,'Friends (1)');
  a.get('friends-button').click();assert.equal(a.get('navigation-friends').hidden,false);assert.match(a.get('friend-list').textContent,/Morgan/);assert.equal(a.get('navigation-dialog').open,true);
  a.get('navigation-rooms-button').click();assert.equal(a.get('navigation-rooms').hidden,false);
  await a.app.lockAccount();assert.equal(a.get('navigation-dialog').open,false);assert.equal(a.get('friends-button').textContent,'Friends');assert.equal(a.get('friends-button').disabled,true);assert.equal(a.get('direct-count').textContent,'0');assert.equal(a.get('group-count').textContent,'0');assert.equal(a.get('friend-list').textContent,'');
});

test('resizing docks navigation without turning it into an unwanted mobile modal',async t=>{
  const a=await setup(t);Object.defineProperty(a.window,'innerWidth',{value:1180,configurable:true});a.session(connected);a.input('Draft');
  a.get('room-list').querySelector('button').focus();Object.defineProperty(a.window,'innerWidth',{value:480,configurable:true});a.window.dispatchEvent(new a.window.Event('resize'));await flush();
  assert.equal(a.get('navigation-dialog').open,false);assert.equal(a.window.document.activeElement,a.get('navigation-toggle'));
  a.get('friends-button').click();assert.equal(a.get('navigation-dialog').open,true);assert.equal(a.get('navigation-friends').hidden,false);
  a.get('details-toggle').click();assert.equal(a.get('navigation-dialog').open,false);assert.equal(a.get('details-dialog').open,true);
  a.get('details-dialog').close();Object.defineProperty(a.window,'innerWidth',{value:1600,configurable:true});a.window.dispatchEvent(new a.window.Event('resize'));await flush();
  assert.equal(a.get('navigation-dialog').open,true);assert.equal(a.get('message').value,'Draft');
});

test('an account without rooms can add a contact and see the delivered outgoing request',async t=>{
  const a=await setup(t);a.session(base);let list=[];const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=async(command,args)=>{
    if(command==='send_friend_request'){a.calls.push([command,args]);list=[{peer:'bob',name:'Bob',status:'outgoing',request_delivered:true}];return {peer:'bob',status:'outgoing',delivered:true};}
    if(command==='list_friends')return list;return previous(command,args);
  };
  a.get('friends-button').click();a.get('add-friend-button').click();assert.equal(a.get('add-friend-dialog').open,true);assert.equal(a.window.document.activeElement,a.get('friend-contact-id'));
  a.get('friend-contact-id').value='  ochat-contact2:public-route  ';a.get('add-friend-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();await flush();
  assert.equal(a.calls.find(([c])=>c==='send_friend_request')[1].identifier,'ochat-contact2:public-route');assert.match(a.get('sent-friend-list').textContent,/Bob.*awaiting acceptance/);
  assert.equal(a.get('add-friend-dialog').open,false);assert.equal(a.get('friend-contact-id').value,'');assert.equal(a.calls.some(([c])=>['create_room','join_room','open_direct'].includes(c)),false);
});

test('received and sent requests show separate delivery states and incoming actions',async t=>{
  const a=await setup(t);a.session(base);let list=[{peer:'alice',name:'Alice',status:'incoming'},{peer:'bob',name:'Bob',status:'outgoing',request_delivered:true},{peer:'carol',name:'Carol',status:'outgoing',request_delivered:false}];
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>{
    if(command==='list_friends')return list;
    if(command==='respond_friend'){a.calls.push([command,args]);list=list.map(f=>f.peer===args.peer?{...f,status:'accepted'}:f);return true;}return previous(command,args);
  };
  await a.app.handleEvent({type:'friends_changed'});assert.equal(a.get('friends-button').textContent,'Friends (1)');
  assert.match(a.get('received-friend-list').textContent,/Alice.*AcceptDecline/);assert.match(a.get('sent-friend-list').textContent,/Bob.*awaiting acceptance.*Carol.*Waiting to reach/);
  [...a.get('received-friend-list').querySelectorAll('button')].find(button=>button.textContent==='Accept').click();await flush();
  assert.match(a.get('accepted-friend-list').textContent,/Alice.*Message/);assert.equal(a.get('received-friend-list').querySelector('.friend-row'),null);assert.equal(a.get('friends-button').textContent,'Friends');
});

test('invalid contact IDs retain an inline error and unsent requests show their saved state',async t=>{
  const a=await setup(t);a.session(base);const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='send_friend_request'?Promise.reject(Error('Paste a complete contact ID')):previous(command,args);
  a.get('add-friend-button').click();a.get('friend-contact-id').value='bad id';a.get('add-friend-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();
  assert.equal(a.get('add-friend-dialog').open,true);assert.match(a.get('add-friend-form').querySelector('.form-error').textContent,/complete contact ID/);assert.equal(a.get('friend-contact-id').value,'bad id');
  a.window.__TAURI__.core.invoke=(command,args)=>command==='send_friend_request'?Promise.resolve({peer:'bob',status:'outgoing',delivered:false}):previous(command,args);
  a.get('friend-contact-id').value='ochat-contact2:offline';a.get('add-friend-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();await flush();assert.match(a.get('toast').textContent,/saved request will retry/);
});

test('copying a contact ID is explicit and late contact operations cannot survive locking',async t=>{
  const a=await setup(t);a.session(base);let resolveId,resolveRequest;const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='contact_id'?new Promise(resolve=>{resolveId=resolve;}):command==='send_friend_request'?new Promise(resolve=>{resolveRequest=resolve;}):previous(command,args);
  a.get('copy-contact-id').click();await flush();resolveId('ochat-contact2:public-route');await flush();assert.equal(a.calls.find(([c])=>c==='copy_text')[1].text,'ochat-contact2:public-route');
  a.get('copy-contact-id').click();a.get('add-friend-button').click();a.get('friend-contact-id').value='ochat-contact2:private-route';a.get('add-friend-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();
  await a.app.lockAccount();resolveId('ochat-contact2:late');resolveRequest({peer:'bob',status:'outgoing',delivered:true});await flush();await flush();
  assert.equal(a.calls.filter(([c])=>c==='copy_text').length,1);assert.equal(a.get('friend-contact-id').value,'');assert.equal(a.get('add-friend-dialog').open,false);assert.equal(a.get('friend-list').textContent,'');assert.equal(a.get('copy-contact-id').disabled,true);assert.equal(a.get('copy-contact-id').textContent,'Copy my contact ID');assert.equal(a.get('auth-dialog').open,true);
});

test('a pending request does not reopen navigation after the user closes its dialog',async t=>{
  const a=await setup(t);Object.defineProperty(a.window,'innerWidth',{value:480,configurable:true});a.session(base);let resolveRequest;const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='send_friend_request'?new Promise(resolve=>{resolveRequest=resolve;}):previous(command,args);
  a.get('friends-button').click();a.get('add-friend-button').click();a.get('friend-contact-id').value='ochat-contact2:pending';a.get('add-friend-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();
  a.get('add-friend-dialog').close();resolveRequest({peer:'bob',status:'outgoing',delivered:true});await flush();await flush();
  assert.equal(a.get('navigation-dialog').open,false);assert.equal(a.get('add-friend-dialog').open,false);
});

test('Studio groups messages by identity and time while retaining author context',async t=>{
  const a=await setup(t);a.session(connected);const timestamp=Date.UTC(2026,9,24,12);
  const send=(id,peer,offset,extra={})=>a.app.handleEvent({type:'message',room:'room',id,peer,name:'Morgan',text:'A message',emojis:[],timestamp:timestamp+offset,...extra});
  await send('first','peer',0);await send('second','peer',10000);await send('same-name','another-peer',20000);await send('after-pause','another-peer',150001);await send('reply','another-peer',160000,{reply_to:{peer:'peer',id:'first'}});
  const rows=[...a.get('timeline').querySelectorAll('.message')];assert.deepEqual(rows.map(row=>row.classList.contains('continuation')),[false,true,false,false,false]);
  assert.match(rows[1].getAttribute('aria-label'),/Message from Morgan/);assert.equal(rows[1].querySelector('.reply-action').getAttribute('aria-label'),'Reply to Morgan');
  assert.equal(a.get('timeline').querySelectorAll('.day-divider').length,1);await send('next-day','another-peer',86400000);assert.equal(a.get('timeline').querySelectorAll('.day-divider').length,2);assert.equal(a.get('timeline').querySelector('.message:last-child').classList.contains('continuation'),false);
});

test('files and notices interrupt an editorial message group',async t=>{
  const a=await setup(t);a.session(connected);const event={type:'message',room:'room',peer:'peer',name:'Peer',text:'Text',emojis:[],timestamp:Date.now()};
  await a.app.handleEvent({...event,id:'first'});await a.app.handleEvent({type:'member_joined',room:'room',id:'joined',peer:'new',name:'New person',timestamp:event.timestamp});await a.app.handleEvent({...event,id:'after-notice',timestamp:event.timestamp+1000});
  await a.app.handleEvent({type:'file_offered',room:'room',peer:'peer',name:'Peer',outgoing:false,offer:{id:'file',name:'notes.txt',size:10,hash:'a'.repeat(64)}});await a.app.handleEvent({...event,id:'after-file',timestamp:Date.now()});
  const rows=[...a.get('timeline').querySelectorAll('.message.studio-text')];assert.ok(rows.every(row=>!row.classList.contains('continuation')));
});

test('trimming a long conversation retains a visible author at the start',async t=>{
  const a=await setup(t);a.session(connected);const timestamp=Date.now();
  for(let i=0;i<505;i++)await a.app.handleEvent({type:'message',room:'room',id:'m'+i,peer:'peer',name:'Peer',text:'Line '+i,emojis:[],timestamp:timestamp+i});
  const timeline=a.get('timeline'),rows=timeline.querySelectorAll('.message');assert.ok(timeline.children.length<=500);assert.equal(rows[0].classList.contains('continuation'),false);assert.equal(rows[rows.length-1].classList.contains('continuation'),true);
});

test('Studio voice portraits and speaking dots survive updates in a background room',async t=>{
  const a=await setup(t);a.session(connected);const voice={...connected,room:{id:'voice',name:'Workshop',capacity:0},voice:true,peers:[{...connected.peers[0],voice:true,speaking:true}]};
  await a.app.handleEvent({type:'room_state',snapshot:voice});const person=a.get('voice-roster').querySelector('[data-key="peer"]'),portrait=person.querySelector('.voice-avatar'),dot=person.querySelector('.speaking-dot');person.focus();
  await a.app.handleEvent({type:'room_state',snapshot:{...voice,peers:[{...voice.peers[0],speaking:false}]}});
  assert.equal(a.get('voice-roster').querySelector('[data-key="peer"]'),person);assert.equal(person.querySelector('.voice-avatar'),portrait);assert.equal(person.querySelector('.speaking-dot'),dot);assert.equal(a.window.document.activeElement,person);assert.equal(dot.hidden,true);assert.equal(person.classList.contains('is-speaking'),false);
});


test('message reactions, edit tombstones and pins update in place without changing focus',async t=>{
 const a=await setup(t);a.session(connected);const event={type:'message',room:'room',id:'tools',peer:'peer',name:'Peer',text:'original',emojis:[],mentions:[],timestamp:1};await a.app.handleEvent(event);const row=a.get('timeline').querySelector('[data-message-id="tools"]');a.get('message').focus();
 const update=(change,revision=1,actor='peer')=>a.app.handleEvent({type:'message_updated',room:'room',update:{room:'room',message:{id:'tools',peer:'peer'},actor,revision,change}});
 await update({kind:'reaction',emoji:'👍',active:true});assert.equal(row.querySelector('.reaction-chip').textContent,'👍1');row.querySelector('.reaction-chip').click();await flush();assert.ok(a.calls.some(([c,args])=>c==='update_message'&&args.change.kind==='reaction'));
 await update({kind:'pin',active:true});assert.equal(row.querySelector('.message-pin').hidden,false);
 await update({kind:'edit',text:'changed <literal>',emojis:[]});assert.equal(row.querySelector('.bubble').textContent,'changed <literal>');assert.equal(row.querySelector('.edited-label').textContent,'Edited');assert.equal(a.get('timeline').querySelector('[data-message-id="tools"]'),row);assert.equal(a.window.document.activeElement,a.get('message'));
 await update({kind:'edit',text:null,emojis:[]},2);assert.equal(row.querySelector('.bubble').textContent,'Message deleted');assert.equal(row.querySelector('.message-toolbar').hidden,true);assert.equal(row.querySelector('.reaction-chip'),null);
 await update({kind:'edit',text:'stale resurrection',emojis:[]},1);assert.equal(row.querySelector('.bubble').textContent,'Message deleted');
});
test('only your messages expose editing and deletion, with confirmation before deletion',async t=>{
 const a=await setup(t);a.session(connected);a.app.openMessageTools({room:'room',id:'m',peer:'peer',name:'Peer',text:'remote',timestamp:1});assert.equal(a.get('message-tools-actions').textContent.includes('Edit message'),false);
 a.app.openMessageTools({room:'room',id:'own',peer:'self',name:'Test',text:'mine',timestamp:1});const edit=[...a.get('message-tools-actions').querySelectorAll('button')].find(b=>b.textContent==='Edit message');edit.click();await flush();assert.equal(a.get('edit-message-text').value,'mine');assert.equal(a.get('edit-message-dialog').open,true);
 a.get('edit-message-text').value='edited';a.get('edit-message-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();assert.ok(a.calls.some(([c,args])=>c==='update_message'&&args.change.text==='edited'));
 a.app.openMessageTools({room:'room',id:'own',peer:'self',name:'Test',text:'mine',timestamp:1});[...a.get('message-tools-actions').querySelectorAll('button')].find(b=>b.textContent==='Delete message…').click();await flush();assert.equal(a.get('confirm-dialog').open,true);assert.equal(a.calls.some(([c,args])=>c==='update_message'&&args.change.text===null),false);
});
test('delivery confirmation is distinct from read and failed delivery retries the original id',async t=>{
 const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'message',room:'room',id:'d',peer:'self',name:'Test',text:'test',emojis:[],timestamp:1});const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='message_deliveries'?[{id:'d',status:'failed',expected:['peer'],received:[]}]:previous(c,args);
 await a.app.refreshMessageTools('room');const node=a.get('timeline').querySelector('.message-delivery');assert.match(node.textContent,/Not confirmed/);node.querySelector('button').click();await flush();assert.ok(a.calls.some(([c,args])=>c==='retry_message'&&args.id==='d'&&args.room==='room'));
});
test('activity shows requests and references, and locking clears every tool panel',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='activity_items'?[{room:'room',timestamp:1,event:{type:'message',id:'a',peer:'peer',name:'Peer',text:'private mention',mentions:[{peer:'self',name:'Test'}]}}]:previous(c,args);
 await a.app.refreshActivity();assert.match(a.get('activity-button').textContent,/1/);assert.match(a.get('activity-results').textContent,/private mention/);await a.app.lockAccount();assert.equal(a.get('activity-results').textContent,'');assert.equal(a.get('verification-fingerprint').textContent,'');assert.equal(a.get('activity-button').disabled,true);
});
test('shared gallery filters file offers, images and safe links without loading remote images',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='shared_items'?[{room:'room',timestamp:1,event:{type:'file_offered',peer:'peer',name:'Peer',offer:{id:'f',name:'photo.png',size:20}}},{room:'room',timestamp:2,event:{type:'message',id:'l',peer:'peer',name:'Peer',text:'https://example.com/path'}}]:previous(c,args);
 await a.app.openCollection('shared');assert.match(a.get('collection-results').textContent,/photo.png/);assert.equal(a.get('collection-results').querySelector('a').href,'https://example.com/path');a.window.document.querySelector('[data-collection="images"]').click();assert.match(a.get('collection-status').textContent,/1 item/);assert.equal(a.get('collection-results').querySelector('img'),null);
});

test('identity verification requires an explicit comparison and carries the exact fingerprint',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='peer_verification'?{peer:args.peer,fingerprint:'ABCD 1234',verified:false,same_name_verified:true}:previous(c,args);
 a.get('conversation-identity').click();await flush();a.get('verify-peer-button').click();await flush();assert.equal(a.get('verification-dialog').open,true);assert.equal(a.get('verification-fingerprint').textContent,'ABCD 1234');assert.match(a.get('verification-status').textContent,/different verified identity/);assert.equal(a.calls.some(([c])=>c==='verify_identity'),false);
 a.get('confirm-verification').click();await flush();assert.ok(a.calls.some(([c,args])=>c==='verify_identity'&&args.peer==='peer'&&args.fingerprint==='ABCD 1234'&&args.verified===true));await a.app.lockAccount();assert.equal(a.get('verification-fingerprint').textContent,'');
});
test('voice diagnostics reports native devices and latency without opening capture',async t=>{
 const a=await setup(t);a.session({...connected,voice:true});const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='voice_diagnostics'?{snapshot:{...connected,voice:true,muted:true,peers:[{id:'peer',name:'Peer',voice:true,route:'direct',latency_ms:300}]},audio:settings.audio,level:0.25}:c==='audio_devices'?[{id:'mic',name:'Studio microphone',input:true,is_default:true},{id:'out',name:'Headphones',input:false,is_default:true}]:previous(c,args);
 a.get('voice-diagnostics-button').click();await flush();await flush();assert.equal(a.get('voice-diagnostics-dialog').open,true);assert.match(a.get('voice-diagnostics-facts').textContent,/Studio microphone/);assert.match(a.get('voice-diagnostics-advice').textContent,/muted/);assert.match(a.get('voice-diagnostics-peers').textContent,/High latency/);assert.equal(a.get('diagnostic-level').value,0.25);assert.equal(a.calls.some(([c])=>['start_voice','start_call','accept_call','test_microphone'].includes(c)),false);a.get('voice-diagnostics-dialog').close();assert.equal(a.get('diagnostic-level').value,0);
});

test('read receipts are optional and delivered remains distinct when they are disabled',async t=>{
 const a=await setup(t);a.session(connected);const event={type:'message',room:'room',id:'read',peer:'self',name:'Test',text:'test',emojis:[],timestamp:1};await a.app.handleEvent(event);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='message_deliveries'?[{id:'read',status:'delivered',expected:['peer'],received:['peer'],read:['peer']}]:previous(c,args);
 await a.app.refreshMessageTools('room');assert.equal(a.get('timeline').querySelector('.message-delivery').textContent,'Delivered');a.app.session(connected,{...settings,read_receipts:true});await a.app.refreshMessageTools('room');assert.equal(a.get('timeline').querySelector('.message-delivery').textContent,'Read');
});


test('presentation shortcuts reach conversations, search, settings and the composer without bypassing account consent',async t=>{
 const a=await setup(t);const key=(value,extra={})=>a.window.document.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:value,ctrlKey:true,bubbles:true,cancelable:true,...extra}));key(',');await flush();assert.equal(a.get('settings-dialog').open,false);
 a.session(connected);key('k');await flush();assert.equal(a.get('navigation-dialog').open,true);key('Enter');assert.equal(a.window.document.activeElement,a.get('message'));key('f');await flush();assert.equal(a.get('search-dialog').open,true);a.get('search-dialog').close();key(',');await flush();assert.equal(a.get('settings-dialog').open,true);assert.equal(a.calls.some(([c])=>['start_voice','accept_call','test_microphone'].includes(c)),false);
});
test('only live messages animate and reduced motion prevents presentation animations',async t=>{
 const a=await setup(t);a.session(connected);const event={type:'message',room:'room',id:'animation',peer:'peer',name:'Peer',text:'Live',emojis:[],mentions:[],timestamp:1};await a.app.handleEvent(event);assert.equal(a.get('timeline').querySelector('[data-message-id="animation"]').classList.contains('is-new'),true);
 const old=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='history'?[{room:'room',timestamp:1,event}]:c==='read_marker'?{room:'room',through:1,ids:[]}:old(c,args);await a.app.loadRoom('room');assert.equal(a.get('timeline').querySelector('.is-new'),null);
 a.window.matchMedia=query=>({matches:query.includes('prefers-reduced-motion')});await a.app.handleEvent({...event,id:'reduced'});assert.equal(a.get('timeline').querySelector('[data-message-id="reduced"]').classList.contains('is-new'),false);
});
test('outside clicks dismiss utility panels but never dismiss recovery or incoming-call consent',async t=>{
 const a=await setup(t);a.session(connected);a.app.openMessageTools({room:'room',id:'m',peer:'peer',name:'Peer',text:'test',timestamp:1});const menu=a.get('message-tools-dialog');menu.getBoundingClientRect=()=>({left:20,right:400,top:20,bottom:400});menu.dispatchEvent(new a.window.MouseEvent('click',{clientX:1,clientY:1,bubbles:true}));assert.equal(menu.open,false);
 a.app.showRecovery('test recovery key',true);const recovery=a.get('recovery-dialog');recovery.getBoundingClientRect=menu.getBoundingClientRect;recovery.dispatchEvent(new a.window.MouseEvent('click',{clientX:1,clientY:1,bubbles:true}));assert.equal(recovery.open,true);assert.equal(a.calls.some(([c])=>c==='accept_call'),false);
});

test('Discord rail and dock move as existing nodes across responsive layouts',async t=>{
  const a=await setup(t);Object.defineProperty(a.window,'innerWidth',{value:1180,configurable:true});
  a.session({...connected,voice:true});a.input('Keep this draft');a.get('message').focus();
  const dock=a.get('voice-bar'),speaker=a.get('voice-roster').firstElementChild,tab=a.get('conversation-tabs').firstElementChild;
  assert.equal(dock.parentElement.id,'sidebar-footer');assert.equal(a.get('conversation-tabs').parentElement.id,'conversation-rail');
  Object.defineProperty(a.window,'innerWidth',{value:480,configurable:true});a.window.dispatchEvent(new a.window.Event('resize'));await flush();
  assert.equal(dock.parentElement.id,'conversation-panel');assert.equal(a.get('conversation-tabs').parentElement.id,'navigation-rail-slot');assert.equal(a.get('voice-roster').firstElementChild,speaker);assert.equal(a.get('conversation-tabs').firstElementChild,tab);assert.equal(a.get('message').value,'Keep this draft');assert.equal(a.window.document.activeElement,a.get('message'));
  Object.defineProperty(a.window,'innerWidth',{value:1180,configurable:true});a.window.dispatchEvent(new a.window.Event('resize'));await flush();assert.equal(dock.parentElement.id,'sidebar-footer');assert.equal(a.get('voice-roster').firstElementChild,speaker);assert.equal(a.calls.some(([c])=>['start_voice','stop_voice','select_room'].includes(c)),false);
});

test('Discord shortcuts forward existing room and friend actions without duplicate capture',async t=>{
  const a=await setup(t);a.session({...connected,room:{...connected.room,capacity:0}});a.get('rail-home').click();assert.equal(a.get('navigation-friends').hidden,false);a.get('navigation-rooms-button').click();
  a.get('rail-create').click();assert.equal(a.get('create-dialog').open,true);a.get('create-dialog').close();a.get('rail-join').click();assert.equal(a.get('join-dialog').open,true);a.get('join-dialog').close();
  a.get('sidebar-invite').click();await flush();assert.equal(a.calls.filter(([c])=>c==='room_management').length,1);assert.equal(a.calls.some(([c])=>['start_voice','accept_call'].includes(c)),false);
});

test('vertical rail keyboard reordering persists the same conversations',async t=>{
  const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'other',name:'Another',capacity:2}}});
  const strip=a.get('conversation-tabs'),tab=strip.querySelector('[data-key="room"] [role="tab"]');tab.focus();tab.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'ArrowUp',altKey:true,bubbles:true,cancelable:true}));await flush();
  assert.equal(a.get('private-conversation-tabs').firstElementChild.dataset.key,'room');assert.equal(a.window.document.activeElement,tab);assert.ok(a.calls.some(([c])=>c==='save_conversation_layout'));assert.equal(a.calls.some(([c])=>c==='select_room'),false);
});

test('locking clears new sidebar shortcuts and their private labels',async t=>{
  const a=await setup(t);a.session({...connected,room:{...connected.room,name:'Private group',capacity:0}});assert.equal(a.get('sidebar-room-name').textContent,'Private group');await a.app.lockAccount();assert.equal(a.get('sidebar-room-name').textContent,'Chat');assert.equal(a.get('sidebar-channel-tools').hidden,true);assert.equal(a.get('rail-home').disabled,true);assert.equal(a.window.document.body.innerHTML.includes('Private group'),false);
});

test('pending friend requests can retry once while keeping a visible delivery state',async t=>{
  const a=await setup(t);a.session(connected);let entries=[{peer:'remote',name:'Remote',status:'outgoing',request_delivered:false,request_error:'Connection timed out'}],resolveRetry;
  const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=(command,args)=>command==='list_friends'?Promise.resolve(entries):command==='retry_friend_request'?new Promise(resolve=>{a.calls.push([command,args]);resolveRetry=resolve;}):previous(command,args);
  await a.app.handleEvent({type:'friends_changed'});const retry=a.get('sent-friend-list').querySelector('button[aria-label="Retry friend request to Remote"]');assert.ok(retry);assert.match(a.get('sent-friend-list').textContent,/Connection timed out/);
  retry.click();retry.click();await flush();assert.equal(a.calls.filter(([c])=>c==='retry_friend_request').length,1);
  await a.app.handleEvent({type:'friends_changed'});assert.equal(a.get('sent-friend-list').querySelector('button[aria-label="Retry friend request to Remote"]').disabled,true);
  entries=[{...entries[0],request_delivered:true,request_error:null}];resolveRetry({status:'outgoing',delivered:true,request_error:null});await flush();await flush();assert.match(a.get('sent-friend-list').textContent,/Request sent · awaiting acceptance/);assert.equal(a.get('sent-friend-list').querySelector('details'),null);assert.equal(a.get('sent-friend-list').querySelector('button[aria-label="Retry friend request to Remote"]').disabled,false);
});

test('opening Friends refreshes a request even if its change event was missed',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=(command,args)=>command==='list_friends'?Promise.resolve([{peer:'remote',name:'New contact',status:'incoming'}]):previous(command,args);
  a.get('friends-button').click();await flush();assert.match(a.get('received-friend-list').textContent,/New contact/);assert.equal(a.get('navigation-friends').hidden,false);assert.equal(a.calls.some(([c])=>c==='send_friend_request'),false);
});

test('a late friend retry result cannot restore private labels after Lock',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;let resolveRetry;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='list_friends'?Promise.resolve([{peer:'private-contact-id',name:'Private contact',status:'outgoing',request_delivered:false,request_error:'Private network detail'}]):command==='retry_friend_request'?new Promise(resolve=>{resolveRetry=resolve;}):previous(command,args);
  await a.app.handleEvent({type:'friends_changed'});a.get('sent-friend-list').querySelector('[aria-label="Retry friend request to Private contact"]').click();await flush();await a.app.lockAccount();resolveRetry({status:'outgoing',delivered:false,request_error:'Late private detail'});await flush();assert.equal(a.window.document.body.innerHTML.includes('private-contact-id'),false);assert.equal(a.window.document.body.textContent.includes('Private contact'),false);assert.equal(a.window.document.body.textContent.includes('Late private detail'),false);assert.equal(a.get('toast').hidden,true);
});

test('a failed Friends refresh after Lock does not display private connection errors',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;let rejectRefresh;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='list_friends'?new Promise((_,reject)=>{rejectRefresh=reject;}):previous(command,args);
  a.get('friends-button').click();await flush();await a.app.lockAccount();rejectRefresh(Error('PRIVATE CONNECTION DETAIL'));await flush();assert.equal(a.get('toast').hidden,true);assert.equal(a.window.document.body.textContent.includes('PRIVATE CONNECTION DETAIL'),false);
});

test('a failed resend is not presented as successful because of an older receipt',async t=>{
  const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;
  a.window.__TAURI__.core.invoke=(command,args)=>command==='send_friend_request'?Promise.resolve({peer:'remote',status:'outgoing',delivered:true,request_error:'Connection timed out'}):command==='list_friends'?Promise.resolve([{peer:'remote',name:'Remote',status:'outgoing',request_delivered:true,request_error:'Connection timed out'}]):previous(command,args);
  a.get('add-friend-button').click();a.get('friend-contact-id').value='ochat-contact2:public-contact';a.get('add-friend-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();assert.match(a.get('toast').textContent,/Contact not reachable yet/);assert.match(a.get('sent-friend-list').textContent,/Connection timed out/);
});

test('voice badges show remote voice activity without joining and disappear when voice ends',async t=>{
 const a=await setup(t);a.session({...connected,peers:[{...connected.peers[0],voice:true}]});
 assert.equal(a.get('voice-bar').hidden,true);for(const node of a.window.document.querySelectorAll('.tab-voice-icon'))assert.equal(node.hidden,false);
 a.session(connected);for(const node of a.window.document.querySelectorAll('.tab-voice-icon'))assert.equal(node.hidden,true);
});
test('personal emoji management is separate from the room bank and works without a room',async t=>{
 const a=await setup(t);a.session(base);const old=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(c,args)=>c==='personal_emojis'?[{id:'personal',name:'my_cat',hash:'a'.repeat(64)}]:old(c,args);
 a.get('my-emojis-button').click();await flush();assert.equal(a.get('emoji-dialog').open,true);assert.match(a.get('custom-grid').textContent,/my_cat/);assert.equal(a.get('add-emoji').disabled,false);assert.match(a.get('emoji-library-hint').textContent,/private library/);
 await a.app.handleEvent({type:'emoji_bank',room:'room',entries:[{name:'shared_cat'}]});assert.doesNotMatch(a.get('custom-grid').textContent,/shared_cat/);
 const rename=[...a.get('custom-grid').querySelectorAll('button')].find(b=>b.textContent==='Rename');assert.equal(rename.disabled,false);rename.click();assert.equal(a.get('emoji-edit-title').textContent,'Rename personal emoji');
});
test('custom shortcuts record modifier combinations, reject duplicates, and cancel with Settings',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
 a.get('microphone-shortcut').click();await flush();a.window.document.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'m',code:'KeyM',ctrlKey:true,shiftKey:true,bubbles:true,cancelable:true}));
 assert.equal(a.get('microphone-shortcut').dataset.shortcut,'Control+Shift+KeyM');
 a.get('deafen-shortcut').click();await flush();a.window.document.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'m',code:'KeyM',ctrlKey:true,shiftKey:true,bubbles:true,cancelable:true}));assert.match(a.get('shortcut-status').textContent,/already assigned/);
 a.window.document.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'d',code:'KeyD',ctrlKey:true,shiftKey:true,bubbles:true,cancelable:true}));assert.equal(a.get('deafen-shortcut').dataset.shortcut,'Control+Shift+KeyD');
 a.get('settings-cancel').click();assert.equal(a.calls.some(([c])=>c==='save_settings'),false);
 a.get('settings-button').click();await flush();assert.equal(a.get('microphone-shortcut').dataset.shortcut,'');
});
test('deafen targets the active background vocal instead of the selected room',async t=>{
 const a=await setup(t);a.session({...connected,room:{id:'voice-room',name:'Voice',capacity:4},voice:true});
 await a.app.handleEvent({type:'state',snapshot:{...connected,room:{id:'text-room',name:'Text',capacity:4}}});
 a.get('deafen-button').click();await flush();assert.equal(a.calls.find(([c])=>c==='set_deafened')[1].room,'voice-room');assert.equal(a.calls.find(([c])=>c==='set_deafened')[1].deafened,true);
});

test('single letters, digits, space and shift combinations can be recorded',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
 for(const [key,code,shiftKey,expected] of [['m','KeyM',false,'KeyM'],['4','Digit4',false,'Digit4'],[' ','Space',false,'Space'],['M','KeyM',true,'Shift+KeyM'],['Escape','Escape',false,'Escape']]){
  a.get('microphone-shortcut').click();await flush();a.get('microphone-shortcut-input').dispatchEvent(new a.window.KeyboardEvent('keydown',{key,code,shiftKey,bubbles:true,cancelable:true}));assert.equal(a.get('microphone-shortcut-input').value,expected);
 }
 a.get('settings-cancel').click();
});

test('left and right Alt and Ctrl can be recorded alone on release, including AltGr',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
 for(const [key,code,flags] of [['Control','ControlLeft',{ctrlKey:true}],['Control','ControlRight',{ctrlKey:true}],['Alt','AltLeft',{altKey:true}],['AltGraph','AltRight',{ctrlKey:true,altKey:true}]]){
  a.get('microphone-shortcut').click();await flush();const field=a.get('microphone-shortcut-input');
  field.dispatchEvent(new a.window.KeyboardEvent('keydown',{key,code,...flags,bubbles:true,cancelable:true}));assert.equal(a.get('microphone-shortcut').getAttribute('aria-pressed'),'true');
  field.dispatchEvent(new a.window.KeyboardEvent('keyup',{key,code,ctrlKey:key==='AltGraph',bubbles:true,cancelable:true}));assert.equal(field.value,code);assert.equal(a.get('microphone-shortcut').getAttribute('aria-pressed'),'false');
 }
 a.get('settings-cancel').click();
});

test('recording a modified letter preserves the combination and does not overwrite it on modifier release',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();a.get('microphone-shortcut').click();await flush();const field=a.get('microphone-shortcut-input');
 field.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'Control',code:'ControlLeft',ctrlKey:true,bubbles:true,cancelable:true}));
 field.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'m',code:'KeyM',ctrlKey:true,bubbles:true,cancelable:true}));
 field.dispatchEvent(new a.window.KeyboardEvent('keyup',{key:'Control',code:'ControlLeft',bubbles:true,cancelable:true}));assert.equal(field.value,'Control+KeyM');
 a.get('settings-cancel').click();
});

test('Ctrl plus Left Alt records the same chord in either key order and release order',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
 for(const order of [['ControlLeft','AltLeft'],['AltLeft','ControlLeft']])for(const released of ['ControlLeft','AltLeft']){
  a.get('microphone-shortcut').click();await flush();const field=a.get('microphone-shortcut-input');
  const held=new Set();
  for(const code of order){held.add(code);field.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:code.startsWith('Control')?'Control':'Alt',code,ctrlKey:held.has('ControlLeft'),altKey:held.has('AltLeft'),bubbles:true,cancelable:true}));}
  held.delete(released);field.dispatchEvent(new a.window.KeyboardEvent('keyup',{key:released.startsWith('Control')?'Control':'Alt',code:released,ctrlKey:held.has('ControlLeft'),altKey:held.has('AltLeft'),bubbles:true,cancelable:true}));
  assert.equal(field.value,'Control+AltLeft');assert.equal(a.get('microphone-shortcut').getAttribute('aria-pressed'),'false');
 }
 a.get('settings-cancel').click();
});

test('a regular Ctrl Alt letter combination is not saved as modifiers alone',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();a.get('microphone-shortcut').click();await flush();const field=a.get('microphone-shortcut-input');
 for(const [key,code] of [['Control','ControlLeft'],['Alt','AltLeft'],['m','KeyM']])field.dispatchEvent(new a.window.KeyboardEvent('keydown',{key,code,ctrlKey:true,altKey:key!=='Control',bubbles:true,cancelable:true}));
 field.dispatchEvent(new a.window.KeyboardEvent('keyup',{key:'Alt',code:'AltLeft',ctrlKey:true,bubbles:true,cancelable:true}));assert.equal(field.value,'Control+Alt+KeyM');a.get('settings-cancel').click();
});

test('a remote stream is announced without receiving images until Watch, with independent volume and Stop watching',async t=>{
 const a=await setup(t);a.session({...connected,voice:true,call:{id:'call',status:'active',peer:'peer'},peers:[{...connected.peers[0],voice:true,visual_media:true,visual_revision:3}]});
 const control={session:'share',media:'call',kind:'screen',profile:{codec:'jpeg',width:640,height:360,fps:10}};
 const frame={type:'visual_packet',room:'room',peer:'peer',control,sequence:1,key:true,data:'YWJj'};
 await a.app.handleEvent(frame);assert.equal(a.get('visual-grid').children.length,0);
 await a.app.handleEvent({type:'visual_available',room:'room',peer:'peer',control});const card=a.get('visual-grid').firstElementChild;
 assert.equal(a.calls.some(([c])=>c==='watch_visual'),false);assert.equal(card.querySelector('img').hasAttribute('src'),false);
 await a.app.handleEvent(frame);assert.equal(card.querySelector('img').hasAttribute('src'),false);
 const slider=card.querySelector('input[type=range]');slider.value=35;slider.dispatchEvent(new a.window.Event('input'));slider.dispatchEvent(new a.window.Event('change'));assert.equal(a.calls.some(([c])=>c==='set_peer_volume'),false);
 card.querySelector('.visual-watch').click();await flush();assert.equal(a.calls.find(([c])=>c==='watch_visual')[1].volume,0.35);
 await a.app.handleEvent(frame);assert.equal(card.querySelector('img').hasAttribute('src'),true);
 card.querySelector('.visual-watch').click();await flush();assert.equal(a.calls.filter(([c])=>c==='watch_visual').at(-1)[1].watching,false);
 assert.equal(card.querySelector('img').hasAttribute('src'),false);await a.app.handleEvent({...frame,sequence:2});assert.equal(card.querySelector('img').hasAttribute('src'),false);
});

test('late and missing VP8 packets recover without resetting a newer frame backwards',async t=>{
 const a=await setup(t);const decoders=[],decoded=[],requests=[];
 a.window.VideoDecoder=class {constructor(callbacks){this.callbacks=callbacks;this.decodeQueueSize=0;decoders.push(this);}configure(){}decode(chunk){decoded.push(chunk.timestamp);}close(){this.closed=true;}};
 a.window.EncodedVideoChunk=class {constructor(value){Object.assign(this,value);}};
 const renderer=a.window.OchatVideoRenderer.create(a.window.document.createElement('div'),{requestKeyframe:event=>requests.push(event.sequence)});
 const control={session:'s',profile:{codec:'vp8',width:640,height:360,fps:60}};
 const packet=(sequence,key=false)=>renderer.packet({type:'visual_packet',control,sequence,timestamp:sequence,key,data:'AAAA'});
 packet(1,true);packet(3);assert.equal(requests.length,1);assert.equal(!!decoders[0].closed,false);
 packet(2);assert.deepEqual(decoded,[1]);packet(4,true);packet(5);assert.deepEqual(decoded,[1,4,5]);
 packet(4,true);assert.deepEqual(decoded,[1,4,5]);
 decoders[0].decodeQueueSize=9;packet(6);assert.equal(!!decoders[0].closed,false);
 decoders[0].decodeQueueSize=0;packet(7);packet(8,true);assert.deepEqual(decoded,[1,4,5,8]);assert.equal(decoders.length,1);
 renderer.dispose();
});

test('shortcut recording supports AZERTY logical letters, missing codes and standalone function keys',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
 const record=async event=>{a.get('microphone-shortcut').click();await flush();a.get('microphone-shortcut-input').dispatchEvent(new a.window.KeyboardEvent('keydown',{bubbles:true,cancelable:true,...event}));};
 await record({key:'m',code:'Semicolon',ctrlKey:true,shiftKey:true});assert.equal(a.get('microphone-shortcut-input').value,'Control+Shift+KeyM');
 await record({key:'F8',code:''});assert.equal(a.get('microphone-shortcut-input').value,'F8');
 await record({key:'q',code:'Unidentified',altKey:true});assert.equal(a.get('microphone-shortcut-input').value,'Alt+KeyQ');
 a.get('microphone-shortcut-input').value='Alt+Shift+F9';a.get('microphone-shortcut-input').dispatchEvent(new a.window.Event('input'));assert.equal(a.window.OchatFeatures.readShortcuts().microphone_shortcut,'Alt+Shift+F9');
 a.get('settings-cancel').click();
});

test('private and room categories retain distinct entries and cannot be reordered across categories',async t=>{
 const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'group',name:'Group',capacity:0}}});
 assert.equal(a.get('private-conversation-tabs').querySelectorAll('.conversation-tab-entry').length,1);
 assert.equal(a.get('room-conversation-tabs').querySelectorAll('.conversation-tab-entry').length,1);
 const entries=[...a.get('conversation-tabs').querySelectorAll('.conversation-tab-entry')];assert.equal(entries[0].dataset.key,'room');assert.equal(entries[1].dataset.key,'group');
});
test('native file drops require review and remain bound to the original target',async t=>{
 const a=await setup(t);a.session(connected);let sent;
 const old=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>{if(c==='send_dropped_files'){sent=args;return [];}return old(c,args);};
 a.window.OchatFeatures.drop({paths:['C:\\files\\one.txt','C:\\files\\two.png']});assert.equal(a.get('drop-dialog').open,true);assert.equal(a.calls.some(([c])=>c==='send_dropped_files'),false);assert.match(a.get('drop-file-list').textContent,/one.txt/);
 a.get('drop-send').click();await flush();assert.equal(sent.room,'room');assert.equal(sent.paths.length,2);assert.equal(a.get('drop-dialog').open,false);
 await a.app.lockAccount();a.window.OchatFeatures.drop({paths:['C:\\private.txt']});assert.equal(a.get('drop-dialog').open,false);assert.equal(a.get('drop-file-list').children.length,0);
});
test('video capture is never opened for an incoming or unaccepted call',async t=>{
 const a=await setup(t);let captured=0;Object.defineProperty(a.window.navigator,'mediaDevices',{value:{getUserMedia:async()=>{captured++;throw Error('must not capture');}},configurable:true});
 a.session({...connected,call:{id:'call',peer:'peer',name:'Peer',status:'incoming'}});a.get('camera-button').click();await flush();assert.equal(captured,0);assert.equal(a.calls.some(([c])=>c==='start_visual'),false);
});
test('late camera permission cannot activate capture after locking',async t=>{
 const a=await setup(t);let resolve,stopped=0;const track={stop(){stopped++;}},stream={getTracks:()=>[track],getVideoTracks:()=>[track]};
 Object.defineProperty(a.window.navigator,'mediaDevices',{value:{getUserMedia:()=>new Promise(r=>resolve=r)},configurable:true});
 a.session({...connected,voice:true,call:{id:'call',status:'active',peer:'peer'},peers:[{...connected.peers[0],voice:true,visual_media:true,visual_revision:3}]});
 a.get('camera-button').click();await flush();a.get('capture-options-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();await a.app.lockAccount();resolve(stream);await flush();assert.equal(stopped,1);assert.equal(a.calls.some(([c])=>c==='start_visual'),false);assert.equal(a.get('visual-stage').hidden,true);
});
test('remote video is ephemeral, keyed by capture session, and removed on call end',async t=>{
 const a=await setup(t);const voice={...connected,voice:true,call:{id:'call',status:'active',peer:'peer'},peers:[{...connected.peers[0],voice:true,visual_media:true}]};a.session(voice);
 const frame={type:'visual_frame',room:'room',peer:'peer',kind:'camera',session:'capture',media:'call',data_url:'data:image/jpeg;base64,YWJj'};
 await a.app.handleEvent({type:'visual_available',room:'room',peer:'peer',control:{session:'capture',media:'call',kind:'camera',profile:{codec:'jpeg',width:640,height:360,fps:10}}});a.get('visual-grid').querySelector('.visual-watch').click();await flush();await a.app.handleEvent(frame);const card=a.get('visual-grid').firstElementChild;assert.ok(card);await a.app.handleEvent(frame);assert.equal(a.get('visual-grid').firstElementChild,card);
 await a.app.handleEvent({...frame,media:'old-call',session:'old'});assert.equal(a.get('visual-grid').firstElementChild,card);
 await a.app.handleEvent({type:'visual_stopped',room:'room',peer:'peer',kind:'camera',session:'old'});assert.equal(a.get('visual-grid').firstElementChild,card);
 await a.app.handleEvent({type:'room_state',snapshot:connected});a.app.render(connected);assert.equal(a.get('visual-grid').children.length,0);assert.equal(a.get('visual-stage').hidden,true);
});

test('custom titlebar retains drag, minimize, maximize and close actions',async t=>{
 const events=[];const native={minimize:async()=>events.push('minimize'),toggleMaximize:async()=>events.push('maximize'),close:async()=>events.push('close'),startDragging:async()=>events.push('drag'),onDragDropEvent:async()=>()=>{}};
 const a=await setup(t,native);for(const id of ['window-minimize','window-maximize','window-close'])a.get(id).click();await flush();assert.deepEqual(events,['minimize','maximize','close']);
 a.window.document.querySelector('.app-header').dispatchEvent(new a.window.MouseEvent('mousedown',{button:0,bubbles:true,detail:1}));await flush();assert.equal(events.at(-1),'drag');
});
test('participant volume targets the chosen person with independent listening gain',async t=>{
 const a=await setup(t);a.session({...connected,voice:true,call:{id:'call',peer:'peer',status:'active'},peers:[{...connected.peers[0],voice:true}]});
 a.get('voice-roster').querySelector('[aria-label^="View Peer"]').click();assert.equal(a.get('participant-volume-button').hidden,false);a.get('participant-volume-button').click();assert.equal(a.get('peer-volume-dialog').open,true);assert.equal(a.get('peer-volume').value,'100');
 a.get('peer-volume').value=35;a.get('peer-volume').dispatchEvent(new a.window.Event('input'));assert.equal(a.get('peer-volume-label').textContent,'35%');a.get('peer-volume-save').click();await flush();assert.equal(a.calls.find(([c])=>c==='set_peer_volume')[1].peer,'peer');assert.equal(a.calls.find(([c])=>c==='set_peer_volume')[1].volume,.35);
});
test('video can be collapsed without stopping the call or remote stream',async t=>{
 const a=await setup(t);a.session({...connected,voice:true,call:{id:'call',status:'active',peer:'peer'},peers:[{...connected.peers[0],voice:true,visual_media:true}]});
 await a.app.handleEvent({type:'visual_available',room:'room',peer:'peer',control:{session:'cap',media:'call',kind:'screen',profile:{codec:'jpeg',width:640,height:360,fps:10}}});a.get('visual-grid').querySelector('.visual-watch').click();await flush();await a.app.handleEvent({type:'visual_frame',room:'room',peer:'peer',kind:'screen',session:'cap',media:'call',data_url:'data:image/jpeg;base64,YWJj'});a.get('collapse-video').click();assert.equal(a.get('visual-grid').hidden,true);assert.equal(a.get('visual-grid').children.length,1);assert.equal(a.calls.some(([c])=>c==='end_call'),false);a.get('collapse-video').click();assert.equal(a.get('visual-grid').hidden,false);
});

test('pointer tab reordering preserves native file-drop support and the current selection',async t=>{
 const native={minimize:async()=>{},toggleMaximize:async()=>{},close:async()=>{},startDragging:async()=>{},onDragDropEvent:async()=>()=>{}};
 const a=await setup(t,native);a.session(connected);await a.app.handleEvent({type:'room_state',snapshot:{...connected,room:{id:'other',name:'Other',capacity:2}}});
 const entries=[...a.get('conversation-tabs').querySelectorAll('.conversation-tab-entry')],source=entries.find(e=>e.dataset.key==='room'),target=entries.find(e=>e.dataset.key==='other');assert.equal(source._tab.draggable,false);
 a.window.document.elementFromPoint=()=>target;target.getBoundingClientRect=()=>({top:0,height:54});
 source._tab.dispatchEvent(new a.window.PointerEvent('pointerdown',{button:0,pointerId:1,clientX:10,clientY:0,bubbles:true}));
 a.window.document.dispatchEvent(new a.window.PointerEvent('pointermove',{pointerId:1,clientX:10,clientY:50,bubbles:true,cancelable:true}));
 a.window.document.dispatchEvent(new a.window.PointerEvent('pointerup',{pointerId:1,bubbles:true}));await flush();
 assert.ok(a.calls.some(([c])=>c==='save_conversation_layout'));assert.equal(a.get('room-title').textContent,'Peer');
});

test('choosing a room emoji preserves its image when a personal emoji has the same name',async t=>{
 const a=await setup(t);a.session(connected);const old=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(c,args)=>c==='personal_emojis'?[{id:'personal',name:'same_name',hash:'a'.repeat(64)}]:c==='room_emojis'?[{id:'room-emoji',name:'same_name',hash:'b'.repeat(64)}]:old(c,args);
 await a.app.openEmojis(true,'personal');a.get('custom-grid').querySelector('.emoji-library-choose').click();
 await a.app.openEmojis(true,'room');a.get('custom-grid').querySelector('.emoji-library-choose').click();a.submit();await flush();
 assert.equal(a.calls.find(([c])=>c==='send_text')[1].emojis[0].hash,'b'.repeat(64));
});


test('room channels preserve separate drafts and send to the selected channel',async t=>{
 const a=await setup(t);a.session(structuredClone(channelRoom));let selected='general';const invoke=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='select_text_channel'){selected=args.channel;return {...structuredClone(channelRoom),text_channel:selected};}return invoke(command,args);};
 a.input('General draft');a.get('room-channels').querySelector('[data-channel="projects"] .channel-select').click();await flush();await flush();
 assert.equal(a.get('message').value,'');assert.equal(a.get('room-title').textContent,'# projects');a.input('Project draft');
 a.get('room-channels').querySelector('[data-channel="general"] .channel-select').click();await flush();await flush();assert.equal(a.get('message').value,'General draft');
 a.submit();await flush();const call=a.calls.findLast(([command])=>command==='send_text');assert.equal(call[1].channel,'general');assert.equal(call[1].room,'room');
});
test('channel history does not show live messages from another channel',async t=>{
 const a=await setup(t);a.session(structuredClone(channelRoom));await a.app.handleEvent({type:'message',room:'room',channel:'projects',id:'hidden-project',peer:'peer',name:'Peer',text:'Other channel message',emojis:[],timestamp:1});
 assert.equal(a.get('timeline').textContent.includes('Other channel message'),false);
 await a.app.handleEvent({type:'message',room:'room',channel:'general',id:'visible-general',peer:'peer',name:'Peer',text:'Current channel message',emojis:[],timestamp:2});assert.equal(a.get('timeline').textContent.includes('Current channel message'),true);
});
test('voice channel rows keep speaking elements stable and join the explicit target',async t=>{
 const a=await setup(t);const snapshot={...structuredClone(channelRoom),peers:[{...connected.peers[0],voice:true,voice_channel:'voice-lounge',speaking:true}]};a.session(snapshot);
 const person=a.get('room-channels').querySelector('[data-channel="voice-lounge"] [data-peer="peer"]');assert.ok(person);const dot=person.querySelector('.speaking-dot');assert.ok(dot.classList.contains('speaking'));
 a.app.render({...snapshot,peers:[{...snapshot.peers[0],speaking:false}]});assert.equal(a.get('room-channels').querySelector('[data-peer="peer"]'),person);assert.equal(dot.classList.contains('speaking'),false);
 a.get('room-channels').querySelector('[data-channel="voice-lounge"] .channel-select').click();await flush();assert.ok(a.calls.some(([command,args])=>command==='join_voice_channel' && args.room==='room' && args.channel==='voice-lounge'));
});
test('channel management belongs to the owner and is cleared on Lock',async t=>{
 const a=await setup(t);a.session(structuredClone(channelRoom));a.get('room-channels').querySelector('.channel-manage').click();assert.equal(a.get('channel-editor-dialog').open,true);
 await a.app.lockAccount();assert.equal(a.get('channel-editor-dialog').open,false);assert.equal(a.get('channel-name').value,'');assert.equal(a.get('room-channels').children.length,0);
 a.session({...structuredClone(channelRoom),room:{...channelRoom.room,owner:'other'}});assert.ok([...a.get('room-channels').querySelectorAll('.channel-manage,.channel-add')].every(button=>button.hidden));
});
test('Risibank sends the raw image immediately and preserves a draft',async t=>{
 const a=await setup(t);a.session(connected);a.input('My unfinished draft');const previous=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='risibank_browse')return {medias:[{id:123,slug:'test-meme',cache_url:'https://risibank.fr/cache/medias/123/full.png'}],has_more:false};if(command==='risibank_image')return 'data:image/png;base64,AAAA';return previous(command,args);};
 a.get('gif-button').click();await flush();a.get('risibank-grid').querySelector('.risibank-item').click();await flush();assert.ok(a.calls.some(([command,args])=>command==='send_risibank_image' && args.room==='room'));assert.equal(a.get('message').value,'My unfinished draft');assert.equal(a.calls.some(([command])=>command==='send_text'),false);assert.equal(a.get('risibank-dialog').open,false);
});
test('Risibank search and late results cannot reveal content after Lock',async t=>{
 const a=await setup(t);a.session(connected);const invoke=a.window.__TAURI__.core.invoke;let finish;
 a.window.__TAURI__.core.invoke=async(command,args)=>command==='risibank_browse'?new Promise(resolve=>{finish=resolve;}):invoke(command,args);
 a.get('gif-button').click();await flush();await a.app.lockAccount();finish({medias:[{id:1,slug:'private-result',cache_url:'https://risibank.fr/cache/medias/1/full.png'}],has_more:false});await flush();assert.equal(a.get('risibank-dialog').open,false);assert.equal(a.get('risibank-grid').textContent,'');assert.equal(a.get('risibank-password'),null);assert.equal(a.get('risibank-username'),null);assert.equal(a.get('gif-button').disabled,true);
});

test('Risibank preview loading settles on decoded images and on failed downloads',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;let resolveImage;
 a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='risibank_browse')return {medias:[{id:12,slug:'loading-preview',cache_url:'https://risibank.fr/cache/medias/12/full.png'}],has_more:false};if(command==='risibank_image')return new Promise(resolve=>{resolveImage=resolve;});return previous(command,args);};
 a.get('gif-button').click();await flush();const tile=a.get('risibank-grid').querySelector('.risibank-item');assert.equal(tile.getAttribute('aria-busy'),'true');assert.ok(tile.querySelector('.meme-loading'));resolveImage('data:image/png;base64,AAAA');await flush();tile.querySelector('img').dispatchEvent(new a.window.Event('load'));assert.equal(tile.querySelector('.meme-loading'),null);assert.equal(tile.getAttribute('aria-busy'),'false');
 a.get('risibank-dialog').close();a.window.__TAURI__.core.invoke=async(command,args)=>command==='risibank_image'?Promise.reject(Error('Offline')):command==='risibank_browse'?{medias:[{id:13,slug:'failed-preview',cache_url:'https://risibank.fr/cache/medias/13/full.png'}],has_more:false}:previous(command,args);
 a.get('gif-button').click();await flush();const failed=a.get('risibank-grid').querySelector('.risibank-item');assert.equal(failed.getAttribute('aria-busy'),'false');assert.equal(failed.querySelector('.meme-loading'),null);assert.match(failed.textContent,/Preview unavailable/);assert.equal(failed.disabled,false);
});

test('chat appearance changes preview independently and Cancel restores saved values',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();
 for(const [id,value]of [['message-font-size',20],['message-spacing',16],['corner-radius',22],['chat-image-size',160]]){a.get(id).value=value;a.get(id).dispatchEvent(new a.window.Event('input'));}
 const root=a.window.document.documentElement;assert.equal(root.style.getPropertyValue('--chat-font-size'),'20px');assert.equal(root.style.getPropertyValue('--chat-image-size'),'160px');assert.equal(a.get('message-font-size-value').textContent,'20 px');
 a.get('settings-cancel').click();assert.equal(root.style.getPropertyValue('--chat-font-size'),'15px');assert.equal(root.style.getPropertyValue('--chat-image-size'),'240px');
});

test('channel participant rows retain portraits, names and speaking state across updates',async t=>{
 const a=await setup(t);const state={...structuredClone(channelRoom),avatar_hash:'d'.repeat(64),voice:true,voice_channel:'voice-general',peers:[{...connected.peers[0],name:'A deliberately very long participant name',voice:true,voice_channel:'voice-general',speaking:true}]};a.session(state);
 const channel=a.get('room-channels').querySelector('[data-channel="voice-general"]'),peer=channel.querySelector('[data-peer="peer"]');assert.equal(peer.querySelector('.channel-person-name').textContent,state.peers[0].name);assert.equal(peer.querySelector('.speaking-dot').getAttribute('aria-label'),'Speaking');assert.ok(channel.querySelector('[data-peer="self"] .avatar'));
 a.session({...state,peers:[{...state.peers[0],speaking:false}]});assert.equal(channel.querySelector('[data-peer="peer"]'),peer);assert.equal(peer.querySelector('.speaking-dot').getAttribute('aria-label'),'Silent');
});


test('GIPHY settings and provider are absent',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();assert.equal(a.get('giphy-api-key'),null);assert.equal(a.get('giphy-configure'),null);assert.equal(a.calls.some(([command])=>command.startsWith('giphy_')),false);
});
test('favorites use a separate star action without sending a meme',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;const media={id:5,slug:'favorite-meme',cache_url:'https://risibank.fr/cache/medias/5/full.png'};let favorites=[];
 a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='media_library')return {recent:[],favorites};if(command==='risibank_browse')return {medias:[media],has_more:false};if(command==='risibank_image')return 'data:image/png;base64,AAAA';if(command==='favorite_risibank'){favorites=args.active?[media]:[];return {recent:[],favorites};}return previous(command,args);};
 a.get('gif-button').click();await flush();a.get('risibank-grid').querySelector('.meme-favorite').click();await flush();assert.equal(favorites.length,1);assert.equal(a.calls.some(([command])=>command==='send_risibank_image'),false);a.window.document.querySelector('[data-meme-mode="favorites"]').click();await flush();assert.ok(a.get('risibank-grid').textContent.includes('favorite meme'));
});
test('private unread badge excludes group room messages and clears on Lock',async t=>{
 const a=await setup(t);a.session(connected);
 const group={...channelRoom,room:{...channelRoom.room,id:'group'}};
 const second={...connected,room:{id:'second-private',name:'Second private chat',capacity:2}};
 await a.app.handleEvent({type:'room_state',snapshot:group});
 await a.app.handleEvent({type:'room_state',snapshot:second});
 assert.equal(a.get('private-unread-count').hidden,true);
 const message=(room,id,peer='peer')=>({type:'message',room,id,peer,name:'Peer',text:'Hello',emojis:[],timestamp:1});
 await a.app.handleEvent(message('group','group-unread'));
 assert.equal(a.get('private-unread-count').hidden,true);
 await a.app.handleEvent(message('room','private-unread'));
 await a.app.handleEvent(message('room','private-unread'));
 await a.app.handleEvent(message('second-private','second-unread'));
 await a.app.handleEvent(message('room','self-message','self'));
 assert.equal(a.get('private-unread-count').textContent,'2');
 assert.equal(a.get('private-unread-count').hidden,false);
 assert.match(a.get('rail-home').getAttribute('aria-label'),/2 unread private messages/);
 const previous=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=(command,args)=>command==='mark_read'?Promise.resolve({through:args.through,ids:args.ids}):previous(command,args);
 a.window.document.hasFocus=()=>true;
 a.window.document.body.dataset.homeOpen='false';
 a.window.document.body.dataset.voiceView='false';
 a.get('timeline').dispatchEvent(new a.window.Event('scroll'));
 await new Promise(resolve=>setTimeout(resolve,2100));
 assert.ok(a.calls.some(([command])=>command==='chat_view'));
 assert.equal(a.get('private-unread-count').textContent,'1','Reading one private conversation keeps the others unread');
 await a.app.lockAccount();assert.equal(a.get('private-unread-count').hidden,true);
});
test('Home shows Friends in the main canvas and keeps private tabs with their controls on the left',async t=>{
 const a=await setup(t);a.session(structuredClone(channelRoom));a.get('rail-home').click();await flush();assert.equal(a.window.document.body.dataset.homeOpen,'true');assert.equal(a.get('navigation-friends').parentElement.id,'home-content');assert.equal(a.get('private-tab-category').parentElement.id,'home-direct-tabs');
 a.get('navigation-rooms-button').click();assert.equal(a.window.document.body.dataset.homeOpen,'false');assert.equal(a.get('room-channels').hidden,false);
});


test('late camera approval cannot attach to a new join of the same voice channel',async t=>{
 const a=await setup(t);let resolve,stopped=0;const track={stop(){stopped++;}},stream={getTracks:()=>[track],getVideoTracks:()=>[track]};Object.defineProperty(a.window.navigator,'mediaDevices',{value:{getUserMedia:()=>new Promise(r=>resolve=r)},configurable:true});
 const voice={...structuredClone(channelRoom),voice:true,voice_channel:'voice-general',voice_session:'first-join',peers:[{...connected.peers[0],voice:true,voice_channel:'voice-general',voice_session:'remote-join',visual_media:true,visual_revision:3}]};a.session(voice);a.get('camera-button').click();await flush();a.get('capture-options-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();a.app.render({...voice,voice_session:'second-join'});resolve(stream);await flush();assert.equal(stopped,1);assert.equal(a.calls.some(([command])=>command==='start_visual'),false);
});
test('channel notification overrides preserve the parent room preference',async t=>{
 const a=await setup(t);const config={...structuredClone(settings),room_notifications:{room:{scope:'all',sound:true}}};a.app.session(structuredClone(channelRoom),config);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>command==='get_settings'?structuredClone(config):previous(command,args);
 a.get('room-notifications-button').click();await flush();a.get('notification-target').value='channel';a.get('notification-target').dispatchEvent(new a.window.Event('change'));a.get('room-notification-scope').value='muted';a.get('room-notifications-save').click();await flush();const saved=a.calls.findLast(([command])=>command==='save_settings');assert.equal(saved[1].settings.room_notifications.general.scope,'muted');assert.equal(saved[1].settings.room_notifications.room.scope,'all');
});

test('joining voice keeps the selected text channel and its draft',async t=>{
 const a=await setup(t);let current=structuredClone(channelRoom);a.session(current);a.input('Keep my general draft');const previous=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='join_voice_channel'){current={...current,voice:true,voice_channel:args.channel,voice_session:'new-join'};return current;}if(command==='snapshot')return current;if(command==='room_snapshots')return [current];return previous(command,args);};
 a.get('room-channels').querySelector('[data-channel="voice-general"] .channel-select').click();await flush();assert.notEqual(a.window.document.body.dataset.voiceView,'true');assert.equal(a.get('message').value,'Keep my general draft');assert.equal(a.get('room-title').textContent,'# general');
 a.get('room-channels').querySelector('[data-channel="voice-general"] .channel-select').click();await flush();assert.equal(a.window.document.body.dataset.voiceView,'true');assert.equal(a.get('voice-channel-view').hidden,false);
});
test('recent emojis appear first and survive reopening the picker',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;let recent=['👍','😀'];a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='media_library')return {recent,favorites:[]};if(command==='record_recent_emoji'){recent=[args.value,...recent.filter(v=>v!==args.value)];return {recent,favorites:[]};}return previous(command,args);};
 await a.app.openEmojis(false);assert.equal(a.get('unicode-grid').firstChild.textContent,'👍');a.get('unicode-grid').querySelector('[aria-label="😀"]').click();await flush();await a.app.openEmojis(false);assert.equal(a.get('unicode-grid').firstChild.textContent,'😀');assert.equal(a.get('unicode-grid').children[1].textContent,'👍');assert.ok(a.get('unicode-grid').children.length>250);
});
test('pasting an image sends it without changing the text draft',async t=>{
 const a=await setup(t);a.session(connected);a.input('Unfinished draft');const file=new a.window.File(['pixels'],'capture.png',{type:'image/png'}),event=new a.window.Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(event,'clipboardData',{value:{items:[{type:'image/png',getAsFile:()=>file}],types:['Files']}});a.get('message').dispatchEvent(event);for(let i=0;i<100&&!a.calls.some(([command])=>command==='send_clipboard_image');i++)await new Promise(r=>setTimeout(r,10));
 assert.equal(event.defaultPrevented,true);const sent=a.calls.find(([command])=>command==='send_clipboard_image');assert.ok(sent);assert.equal(sent[1].room,'room');assert.equal(sent[1].data,Buffer.from('pixels').toString('base64'));assert.equal(a.get('message').value,'Unfinished draft');assert.equal(a.calls.some(([command])=>command==='send_text'),false);
 const text=new a.window.Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(text,'clipboardData',{value:{items:[],types:['text/plain']}});a.get('message').dispatchEvent(text);assert.equal(text.defaultPrevented,false);
});
test('profile cards show bios, open a large photo and clear private labels on Lock',async t=>{
 const a=await setup(t),data='data:image/png;base64,AAAA';const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>command==='emoji_asset'?data:previous(command,args);a.session({...connected,peers:[{...connected.peers[0],name:'PrivateProfile',bio:'PRIVATE BIO',avatar_hash:'a'.repeat(64)}]});a.get('conversation-identity').click();await flush();assert.equal(a.get('details-dialog').classList.contains('profile-overlay'),true);assert.equal(a.get('peer-bio').textContent,'PRIVATE BIO');a.get('peer-avatar').click();assert.equal(a.get('profile-photo-dialog').open,true);assert.equal(a.get('photo-viewer-image').getAttribute('src'),data);await a.app.lockAccount();assert.equal(a.window.document.body.innerHTML.includes('PRIVATE BIO'),false);assert.equal(a.window.document.body.innerHTML.includes('PrivateProfile'),false);assert.equal(a.get('photo-viewer-image').getAttribute('src'),null);
});
test('custom appearance previews cancel and Tor changes relock the account',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();a.get('appearance-background-color-enabled').checked=true;a.get('appearance-background-color').value='#ffffff';a.get('appearance-background-color').dispatchEvent(new a.window.Event('input'));assert.equal(a.window.document.documentElement.style.getPropertyValue('--custom-bg'),'#FFFFFF');a.get('settings-cancel').click();assert.equal(a.window.document.documentElement.style.getPropertyValue('--custom-bg'),'');
 const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>command==='save_settings'?true:previous(command,args);a.get('settings-button').click();await flush();a.get('tor-enabled').checked=true;a.get('settings-save').click();await flush();assert.equal(a.get('auth-dialog').open,true);assert.equal(a.get('message').disabled,true);
});


test('native clipboard fallback sends a screenshot once and keeps the draft',async t=>{
 const a=await setup(t);a.session(connected);a.input('Keep this draft');const previous=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(command,args)=>command==='read_clipboard_image'?'cGl4ZWxz':previous(command,args);
 a.get('message').dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'v',code:'KeyV',ctrlKey:true,bubbles:true}));
 for(let i=0;i<100&&!a.calls.some(([command])=>command==='send_clipboard_image');i++)await new Promise(resolve=>setTimeout(resolve,10));
 const sent=a.calls.filter(([command])=>command==='send_clipboard_image');assert.equal(sent.length,1);assert.equal(sent[0][1].data,'cGl4ZWxz');assert.equal(a.get('message').value,'Keep this draft');
});

test('native Linux screen preview stays lease-scoped and unsupported sound stays disabled',async t=>{
 const a=await setup(t,null,{native:true});const voice={...structuredClone(channelRoom),voice:true,voice_channel:'voice-general',voice_session:'join-local',peers:[{...connected.peers[0],voice:true,voice_channel:'voice-general',voice_session:'remote-join',visual_media:true,visual_revision:3}]};a.session(voice);
 const control={kind:'screen',media:'voice-general',session:'screen-source',profile:{codec:'jpeg',width:640,height:360,fps:15}};
 const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(command,args)=>{if(command==='start_linux_screen'){a.calls.push([command,args]);return control;}return previous(command,args);};
 a.get('screen-button').click();await flush();assert.match(a.get('capture-options-hint').textContent,/desktop portal/);assert.equal(a.get('capture-system-audio').disabled,true);
 a.get('capture-options-form').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();await flush();
 const started=a.calls.find(([command])=>command==='start_linux_screen');assert.ok(started);assert.equal(started[1].lease,'join-local');assert.equal(a.calls.some(([command])=>command==='start_visual'),false);
 const sound=[...a.get('visual-grid').querySelectorAll('button')].find(button=>button.textContent==='Share sound');assert.ok(sound);assert.equal(sound.disabled,true);
 await a.app.handleEvent({type:'native_screen_frame',request:started[1].request,room:'room',control,sequence:1,timestamp:0,key:true,data:'AAAA'});
 assert.equal(a.get('visual-grid').querySelector('img').getAttribute('src'),'data:image/jpeg;base64,AAAA');assert.ok(a.calls.some(([command])=>command==='acknowledge_screen_preview'));
 a.app.render({...voice,voice_session:'join-replacement'});await flush();assert.ok(a.calls.some(([command,args])=>command==='stop_linux_screen'&&args.request===started[1].request));assert.equal(a.get('visual-grid').children.length,0);
});


test('discussion threads reply in the original room without changing the main draft',async t=>{
 const a=await setup(t);a.session(connected);a.input('Main draft');const root={type:'message',room:'room',channel:null,id:'root',peer:'peer',name:'Peer',text:'Original',timestamp:1};
 const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='thread_history'?{entries:[{room:'room',timestamp:1,event:root}],total:1,offset:0}:previous(c,args);
 await a.window.OchatWorkspace.openThread(root);assert.equal(a.get('thread-dialog').open,true);assert.match(a.get('thread-messages').textContent,/Original/);a.get('thread-message').value='Thread reply';a.get('thread-composer').dispatchEvent(new a.window.Event('submit',{cancelable:true}));await flush();const send=a.calls.find(([c])=>c==='send_text');assert.equal(send[1].text,'Thread reply');assert.equal(send[1].replyTo.id,'root');assert.equal(send[1].room,'room');assert.equal(a.get('message').value,'Main draft');a.get('thread-dialog').close();assert.equal(a.get('thread-messages').textContent,'');
});
test('room access exposes role permissions and controlled invitation options',async t=>{
 const a=await setup(t);a.session({...connected,room:{...connected.room,owner:'self'}});const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='room_management'?{owner:'self',policy:null,permissions:{invitations:true},invitations:[],members:[{id:'peer',name:'Peer'}]}:previous(c,args);
 const open=[...a.get('room-actions-dialog').querySelectorAll('button')].find(b=>b.textContent==='Roles & invitations…');open.click();await flush();assert.equal(a.get('room-management').open,true);const role=a.get('role-members').querySelector('[data-peer="peer"]');role.value='moderator';a.get('save-room-access').click();await flush();const saved=a.calls.find(([c])=>c==='save_room_roles');assert.equal(saved[1].roles.peer,'moderator');assert.equal(saved[1].permissions.member.channels,false);a.get('invite-lifetime').value='3600';a.get('invite-uses').value='5';a.get('create-controlled-invite').click();await flush();assert.ok(a.calls.some(([c,args])=>c==='create_controlled_invitation'&&args.maximumUses===5&&args.lifetimeSeconds===3600));
 await a.app.lockAccount();assert.equal(a.get('room-management').open,false);assert.equal(a.get('role-members').children.length,0);assert.equal(a.get('controlled-invitation-output').value,'');
});
test('privacy dashboard and availability controls stay local and clear on lock',async t=>{
 const a=await setup(t);a.session(connected);const previous=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>c==='privacy_dashboard'?{tor_enabled:true,tor_ready:true,relay_enabled:true,external_previews:false,sessions:[{room:{name:'Group'},peers:[{id:'peer',name:'Peer',route:'relay',latency_ms:50}]}],blocked:['blocked-id']}:previous(c,args);
 await a.window.OchatWorkspace.openDashboard();assert.match(a.get('privacy-summary').textContent,/direct fallback disabled/);assert.match(a.get('blocked-contacts').textContent,/blocked-id/);a.get('availability-select').value='busy';a.get('availability-select').dispatchEvent(new a.window.Event('change'));await flush();assert.ok(a.calls.some(([c,args])=>c==='set_availability'&&args.status==='busy'));await a.app.lockAccount();assert.equal(a.get('privacy-dashboard').open,false);assert.equal(a.get('privacy-connections').textContent,'');assert.equal(a.get('blocked-contacts').textContent,'');
});

test('an invitation response arriving after Lock cannot repopulate private fields',async t=>{
 const a=await setup(t);a.session({...connected,room:{...connected.room,owner:'self'}});let deliver;const previous=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=async(c,args)=>c==='room_management'?{owner:'self',policy:null,permissions:{invitations:true},invitations:[],members:[]}:c==='create_controlled_invitation'?new Promise(resolve=>{deliver=resolve;}):previous(c,args);
 a.get('room-access-button').click();await flush();a.get('create-controlled-invite').click();await flush();assert.equal(typeof deliver,'function');await a.app.lockAccount();deliver('ochat-invite:private-pending-token');await flush();assert.equal(a.get('controlled-invitation-output').value,'');assert.equal(a.get('room-management-name').textContent,'');assert.equal(a.get('room-management-note').textContent,'');assert.equal(a.get('thread-context').textContent,'');
});


test('chat image requests coalesce, reuse the session cache and discard it on reset',async t=>{
 const a=await setup(t);a.session(connected);let requests=0,deliver;const previous=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=(c,args)=>c==='message_image'?(requests++,new Promise(resolve=>deliver=resolve)):previous(c,args);
 const event={room:'room',channel:null,peer:'peer',name:'Peer',text:'ochat-image:'+'c'.repeat(64)},make=()=>{const parent=a.window.document.createElement('div');a.get('timeline').append(parent);a.window.OchatPersonalization.image(parent,event);return parent.querySelector('.inline-image');};
 const first=make(),second=make();const one=first._load(),two=second._load();await flush();assert.equal(requests,1);deliver('data:image/png;base64,AAAA');await Promise.all([one,two]);assert.ok(first.querySelector('img'));assert.ok(second.querySelector('img'));
 const cached=make();await cached._load();assert.equal(requests,1);assert.ok(cached.querySelector('img'));
 a.window.OchatPersonalization.reset();const pending=make(),load=pending._load();await flush();assert.equal(requests,2);a.window.OchatPersonalization.reset();deliver('data:image/png;base64,AAAA');await load;assert.equal(pending.querySelector('img'),null,'Late image data must not be restored after reset');
 const fresh=make(),again=fresh._load();await flush();assert.equal(requests,3);deliver('data:image/png;base64,AAAA');await again;
});


test('a real thumbnail is shown blurred during a coalesced load and cleared on Lock',async t=>{
 const a=await setup(t);a.session(connected);a.window.__TAURI__.core.Channel=class {onmessage=()=>{};};let deliver,channel;const original=a.window.__TAURI__.core.invoke;
 a.window.__TAURI__.core.invoke=(c,args)=>c==='message_image'?(channel=args.onPreview,new Promise(resolve=>deliver=resolve)):original(c,args);
 const event={room:'room',peer:'peer',name:'Peer',text:'ochat-image:'+'b'.repeat(64)},parent=a.window.document.createElement('div');a.get('timeline').append(parent);a.window.OchatPersonalization.image(parent,event);const frame=parent.querySelector('.inline-image'),loading=frame._load();await flush();
 channel.onmessage({data:'data:image/png;base64,AAAA',width:640,height:360});const thumb=frame.querySelector('.message-image-blur');assert.ok(thumb);thumb.dispatchEvent(new a.window.Event('load'));assert.equal(frame.querySelector('.message-image-media').classList.contains('has-preview'),true);
 deliver('data:image/png;base64,BBBB');await loading;frame.querySelector('.message-image-full').dispatchEvent(new a.window.Event('load'));assert.equal(frame.querySelector('.message-image-media').classList.contains('ready'),true);
 await a.app.lockAccount();channel.onmessage({data:'data:image/png;base64,AAAA',width:640,height:360});assert.equal(a.get('timeline').querySelector('.message-image-blur'),null);
});

test('screen focus preserves live surfaces and pop out keeps the selected session',async t=>{
 const a=await setup(t);a.session({...channelRoom,voice:true,voice_channel:'voice-general',voice_session:'local',peers:[{...connected.peers[0],voice:true,voice_channel:'voice-general',visual_media:true,visual_revision:3}]});
 const control={session:'share',media:'voice-general',kind:'screen',profile:{codec:'jpeg',width:640,height:360,fps:10}};
 await a.app.handleEvent({type:'visual_available',room:'room',peer:'peer',control});const card=a.get('visual-grid').firstElementChild,canvas=card.querySelector('canvas');assert.equal(card.querySelector('.visual-focus').disabled,true);card.querySelector('.visual-watch').click();await flush();assert.equal(card.classList.contains('visual-focused'),true);
 card.querySelector('.visual-focus').click();assert.equal(card.classList.contains('visual-focused'),false);assert.equal(card.querySelector('canvas'),canvas);
 [...card.querySelectorAll('button')].find(n=>n.textContent==='Pop out').click();await flush();const pop=a.calls.find(([c])=>c==='open_visual_viewer');assert.equal(pop[1].room,'room');assert.equal(pop[1].session,'share');assert.equal(pop[1].peer,'peer');
 card.querySelector('.visual-watch').click();await flush();assert.equal(a.get('visual-grid').classList.contains('has-focus'),false);
});

test('native preferences group existing controls without changing labels or save behavior',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();assert.equal(a.get('settings-dialog').classList.contains('native-preferences'),true);assert.ok(a.get('theme-choice').closest('.preference-group'));assert.ok(a.get('input-device').closest('.preference-group'));assert.equal(a.get('mute-button').parentElement.classList.contains('visual-controls'),true);
 a.get('message-sound').checked=false;a.get('settings-save').click();await flush();assert.equal(a.calls.find(([c])=>c==='save_settings')[1].settings.message_sound,false);
});


test('opening a modal closes the composer picker before Escape reaches the active layer',async t=>{
 const a=await setup(t);a.session(connected);a.input('Keep this draft');a.get('emoji-button').click();await flush();assert.equal(a.get('emoji-dialog').open,true);
 a.get('search-button').click();assert.equal(a.get('emoji-dialog').open,false);assert.equal(a.get('search-dialog').open,true);assert.equal(a.window.document.activeElement,a.get('search-query'));assert.equal(a.get('message').value,'Keep this draft');
});

test('Tor browser links need explicit consent and Copy or Lock never opens the browser',async t=>{
 const a=await setup(t);a.app.session(connected,{...structuredClone(settings),tor_enabled:true});const copied=[];a.window.navigator.clipboard.writeText=async text=>copied.push(text);
 await a.app.handleEvent({type:'message',room:'room',id:'tor-link',peer:'peer',name:'Peer',text:'https://example.com/path',timestamp:1,emojis:[]});const link=a.get('timeline').querySelector('a[href="https://example.com/path"]');link.click();await flush();assert.equal(a.get('external-link-dialog').open,true);assert.equal(a.calls.some(([c])=>c==='open_link'),false);
 a.get('external-link-copy').click();await flush();assert.deepEqual(copied,['https://example.com/path']);assert.equal(a.calls.some(([c])=>c==='open_link'),false);
 for(const [type,button] of [['auxclick',1],['contextmenu',2]]){const event=new a.window.MouseEvent(type,{button,bubbles:true,cancelable:true});link.dispatchEvent(event);await flush();assert.equal(event.defaultPrevented,true);assert.equal(a.get('external-link-dialog').open,true);a.get('external-link-dialog').querySelector('.close-dialog').click();assert.equal(a.calls.some(([c])=>c==='open_link'),false);}
 link.click();await flush();a.get('external-link-open').click();await flush();assert.ok(a.calls.some(([c,args])=>c==='open_link'&&args.url==='https://example.com/path'&&args.torBrowserConfirmed===true));
 const count=a.calls.filter(([c])=>c==='open_link').length;link.click();await flush();await a.app.lockAccount();a.get('external-link-open').click();await flush();assert.equal(a.calls.filter(([c])=>c==='open_link').length,count);assert.equal(a.get('external-link-url').textContent,'');
});

test('audio devices and test precede advanced shortcuts without losing shortcut controls',async t=>{
 const a=await setup(t);a.session(connected);a.get('settings-button').click();await flush();const panel=a.window.document.querySelector('[data-settings-panel="audio"]');assert.equal(panel.firstElementChild.id,'audio-device-preferences');assert.ok(panel.firstElementChild.contains(a.get('input-device')));assert.ok(panel.firstElementChild.contains(a.get('test-microphone')));assert.equal(a.get('audio-shortcuts').open,false);assert.ok(a.get('audio-shortcuts').contains(a.get('microphone-shortcut-input')));a.get('audio-shortcuts').open=true;assert.ok(a.get('microphone-shortcut'));
});

test('call quality polls levels without repeatedly enumerating microphone devices',async t=>{
 const a=await setup(t);a.session({...connected,voice:true});const previous=a.window.__TAURI__.core.invoke;let enumerations=0;
 a.window.__TAURI__.core.invoke=async(c,args)=>c==='audio_devices'?(enumerations++,[]):c==='voice_diagnostics'?{audio:structuredClone(settings.audio),snapshot:{...connected,voice:true},level:.1}:previous(c,args);
 a.get('voice-diagnostics-button').click();await flush();assert.equal(enumerations,1);await a.app.refreshDiagnostics();await a.app.refreshDiagnostics();assert.equal(enumerations,1);a.get('diagnostics-refresh-devices').click();await flush();assert.equal(enumerations,2);assert.equal(a.get('voice-diagnostics-button').textContent,'Call quality');a.get('voice-diagnostics-dialog').close();
 a.get('dock-audio-settings').click();await flush();assert.equal(a.get('settings-dialog').open,true);assert.equal(a.window.document.querySelector('[data-settings-tab="audio"]').classList.contains('selected'),true);
});


test('macOS device-name hint does not request microphone access while viewing settings',async t=>{
 const a=await setup(t);Object.defineProperty(a.window.navigator,'userAgent',{value:'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)',configurable:true});a.session(connected);a.get('settings-button').click();await flush();assert.equal(a.get('microphone-permission-hint').hidden,false);assert.equal(a.calls.some(([c])=>['prepare_capture','start_voice','test_microphone'].includes(c)),false);
});

test('room roles appear in members and profiles and never leak into private chats',async t=>{
 const a=await setup(t),peer={...connected.peers[0],role:'moderator',online:true},room={...channelRoom,role:'owner',peers:[peer],members:[peer]};a.session(room);
 const member=a.get('members').querySelector('[data-avatar-peer="peer"]').closest('.member');assert.equal(member.querySelector('.member-role').textContent,'Moderator');
 member.querySelector('.member-profile').click();await flush();assert.equal(a.get('peer-room-role').textContent,'Moderator');
 a.session({...room,peers:[{...peer,role:'administrator'}],members:[{...peer,role:'administrator'}]});assert.equal(a.get('members').querySelector('[data-avatar-peer="peer"]').closest('.member'),member);assert.equal(member.querySelector('.member-role').textContent,'Administrator');
 a.session({...connected,role:'owner',peers:[peer],members:[peer]});assert.equal(a.get('members').querySelector('.member-role'),null);assert.equal(a.get('peer-room-role').textContent,'');
});

test('voice mute and deafen badges update without replacing channel rows or focus',async t=>{
 const a=await setup(t),peer={...connected.peers[0],online:true,voice:true,voice_channel:'voice-general',voice_session:'peer-session',muted:true,deafened:false},room={...channelRoom,voice:true,muted:false,deafened:true,voice_channel:'voice-general',voice_session:'self-session',peers:[peer],members:[peer]};a.session(room);
 const local=a.get('room-channels').querySelector('.channel-person[data-peer="self"]');assert.equal(local.querySelector('.voice-state-muted').hidden,false);assert.equal(local.querySelector('.voice-state-deafened').hidden,false);
 const row=a.get('room-channels').querySelector('.channel-person[data-peer="peer"]');assert.ok(row);row.focus();const badge=row.querySelector('.voice-state-badges');assert.ok(badge);assert.equal(badge.querySelector('.voice-state-muted').hidden,false);assert.equal(badge.querySelector('.voice-state-deafened').hidden,true);
 const updated={...peer,muted:true,deafened:true};a.session({...room,peers:[updated],members:[updated]});assert.equal(a.get('room-channels').querySelector('.channel-person[data-peer="peer"]'),row);assert.equal(row.querySelector('.voice-state-badges'),badge);assert.equal(a.window.document.activeElement,row);assert.equal(badge.querySelector('.voice-state-deafened').hidden,false);row.click();assert.equal(a.get('peer-voice').textContent,'Deafened');a.window.OchatProfiles.close(false);
 a.session({...room,peers:[{...peer,muted:null,deafened:null}],members:[]});assert.equal(badge.hidden,true);
 await a.app.lockAccount();assert.equal(a.window.document.querySelector('.voice-state-badges,.member-role'),null);
});

test('background voice uses its own room mute state and role badges require verified values',async t=>{
 const a=await setup(t),peer={...connected.peers[0],online:true,voice:true,voice_channel:'voice-general',voice_session:'peer-session',muted:true,deafened:true,role:'owner<script>'},room={...channelRoom,voice:true,voice_channel:'voice-general',voice_session:'self-session',peers:[peer],members:[peer]};a.session(room);assert.equal(a.get('members').querySelector('.member-role'),null);
 a.session({...connected,room:{...connected.room,id:'private-other'},voice:false,peers:[{...connected.peers[0],muted:false,deafened:false}]});
 const dock=a.get('voice-roster').querySelector('[data-avatar-peer="peer"]')?.closest('.voice-person');assert.ok(dock);assert.equal(dock.querySelector('.voice-state-deafened').hidden,false);
});


test('message context menu anchors to the pointer and retains reply copy and permission-aware actions',async t=>{
 const a=await setup(t);a.session({...connected,permissions:{pins:false}});Object.defineProperty(a.window,'innerWidth',{configurable:true,value:480});Object.defineProperty(a.window,'innerHeight',{configurable:true,value:520});await a.app.handleEvent({type:'message',room:'room',id:'context',peer:'peer',name:'Peer',text:'Literal <message>',timestamp:1,emojis:[]});
 const row=a.get('timeline').querySelector('[data-message-id="context"]');row.dispatchEvent(new a.window.MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:470,clientY:510}));
 const menu=a.get('message-tools-dialog');assert.equal(menu.open,true);assert.ok(parseFloat(menu.style.left)<=248);assert.ok(parseFloat(menu.style.top)<=212);const items=[...a.get('message-tools-actions').querySelectorAll('button')];assert.deepEqual(items.map(n=>n.textContent),['Reply','Add reaction…','Open thread','Copy text','Pin message']);assert.equal(items.at(-1).disabled,true);
 items.find(n=>n.textContent==='Copy text').click();await flush();assert.equal(menu.open,false);assert.ok(a.calls.some(([c,args])=>c==='copy_text'&&args.text==='Literal <message>'));
 row.querySelector('.message-toolbar button:last-child').click();[...a.get('message-tools-actions').children].find(n=>n.textContent==='Reply').click();await flush();assert.equal(a.get('reply-draft').hidden,false);assert.ok(a.window.document.activeElement===a.get('message'),'Reply focuses composer');
});

test('message context menu supports keyboard navigation Escape and outside dismissal',async t=>{
 const a=await setup(t);a.session(connected);await a.app.handleEvent({type:'message',room:'room',id:'keyboard-menu',peer:'self',name:'Test',text:'Mine',timestamp:1,emojis:[]});const more=a.get('timeline').querySelector('.message-toolbar button:last-child');more.focus();more.click();await flush();const menu=a.get('message-tools-dialog'),items=[...a.get('message-tools-actions').children];assert.ok(a.window.document.activeElement===items[0],'First item has focus');
 menu.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'End',bubbles:true}));assert.ok(a.window.document.activeElement===items.at(-1),'Last item has focus');menu.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));assert.ok(a.window.document.activeElement===items[0],'First item has focus');
 a.window.document.dispatchEvent(new a.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await flush();assert.equal(menu.open,false);assert.ok(a.window.document.activeElement===more,'Escape restores the opener');
 more.click();await flush();a.get('timeline').dispatchEvent(new a.window.Event('scroll'));assert.equal(menu.open,true);a.get('timeline').dispatchEvent(new a.window.WheelEvent('wheel',{deltaY:30}));assert.equal(menu.open,false);more.click();await flush();a.get('message').dispatchEvent(new a.window.MouseEvent('pointerdown',{bubbles:true}));assert.equal(menu.open,false);
});

test('message context menu dismisses on history changes and cannot act after Lock',async t=>{
 const a=await setup(t);a.session(connected);a.input('Draft retained');const event={room:'room',id:'private-menu',peer:'self',name:'Test',text:'Secret',timestamp:1};a.app.openMessageTools(event);const copy=[...a.get('message-tools-actions').children].find(n=>n.textContent==='Copy text');await a.app.loadRoom('room');assert.equal(a.get('message-tools-dialog').open,false);assert.equal(a.get('message').value,'Draft retained');
 a.app.openMessageTools(event);await a.app.lockAccount();copy.click();await flush();assert.equal(a.calls.some(([c])=>c==='copy_text'),false);assert.equal(a.get('message-tools-dialog').open,false);assert.equal(a.get('message-tools-actions').children.length,0);
});

test('Room access keeps native role selects and every permission and invitation action',async t=>{
 const a=await setup(t);a.session(channelRoom);const original=a.window.__TAURI__.core.invoke;a.window.__TAURI__.core.invoke=async(c,args)=>{if(c==='room_management')return {owner:'self',policy:{roles:{peer:'moderator'},permissions:{moderator:{invitations:true,channels:false,calls:true,pins:false}}},members:[{id:'peer',name:'Peer with a very long readable name'}],invitations:[],permissions:{invitations:true}};return original(c,args);};a.get('room-access-button').click();await flush();
 assert.equal(a.get('room-management').open,true);assert.equal(a.get('role-members').querySelector('[data-peer="peer"]').value,'moderator');assert.equal(a.get('role-members').querySelector('[data-peer="self"]').disabled,true);assert.equal(a.get('role-permissions').querySelectorAll('input[type=checkbox]').length,12);assert.equal(a.get('create-controlled-invite').disabled,false);assert.equal(a.get('invite-uses').value,'1');assert.ok(a.get('copy-controlled-invite'));
 a.get('role-permissions').querySelector('[data-role="moderator"][data-permission="channels"]').checked=true;a.get('save-room-access').click();await flush();const save=a.calls.find(([c])=>c==='save_room_roles');assert.equal(save[1].roles.peer,'moderator');assert.equal(save[1].permissions.moderator.channels,true);
});
