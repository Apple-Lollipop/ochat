'use strict';
const $ = id => document.getElementById(id);
const tauri = window.__TAURI__;
let emojiScope = 'room', emojiLoadGeneration = 0;
const chosenDraftEmojis=new Map();
let unlocked = false, state = null, settings = null, rooms = [], selectedRoom = null;
const archivedChannelViews=new Map(),archivedTextChannels=new Map();
function selectedChannel(room=selectedRoom) { const snapshot=snapshotFor(room);if(snapshot && 'text_channel' in snapshot)return snapshot.text_channel || null;return snapshot?.room?.capacity===2?null:archivedTextChannels.get(room) || null; }
function channelContextFor(room=selectedRoom){const live=snapshotFor(room);if(live)return live;const channels=archivedChannelViews.get(room),saved=rooms.find(r=>r.id===room);return channels && saved?{id:state?.id,name:state?.name,room:saved,channels,text_channel:selectedChannel(room),peers:[],members:roomMembers.get(room)||[],voice:false,connected:false}:null;}
function draftKey(room=selectedRoom,channel=selectedChannel(room)) { return channel?room+'/'+channel:room || 'new'; }
function eventChannel(event) {return event.channel || event.offer?.channel || null;}
function inSelectedChannel(event) {return event.room===selectedRoom && (!selectedChannel() || eventChannel(event)===selectedChannel() || (!eventChannel(event) && state?.channels?.find(c=>c.id===selectedChannel())?.name==='general'));}
async function selectTextChannel(channel) {
  closeMessageMenu(false);
  const room=selectedRoom,generation=sessionGeneration,oldKey=draftKey();
  drafts.set(oldKey,$('message').value);stopTyping();await invoke('set_typing',{active:false,room:selectedRoom,channel:selectedChannel()});
  const next=await invoke('select_text_channel',{room,channel});
  if(!unlocked || generation!==sessionGeneration || selectedRoom!==room)return;
  if(next?.room){state=next;roomSnapshots.set(room,next);}else archivedTextChannels.set(room,channel);$('message').value=drafts.get(draftKey()) || '';
  closeMentions();render();await loadRoom(room);$('message').focus();
}

let emojiEntries = [], editingEmoji = null, recoveryRequired = false, accountExists = false;
let creatingDirect = false;
let friends = [], friendGeneration = 0;
const friendRetries = new Set();
let toastTimer, typingTimer, typingPulse, audioPoll, confirmCallback, cursor = [0, 0];
let membersKey = '', roomsKey = '', loadedRoom = null, historyGeneration = 0, sessionGeneration = 0, assetGeneration = 0;
const assets = new Map(), assetLoads = new Map(), drafts = new Map(), liveOffers = new Map(), shownMessages = new Set();
const roomSnapshots = new Map(), roomMembers = new Map(), unread = new Map();
const pendingRoomMembers=new Set();
const deletedRooms = new Set();
let conversationLayout={order:[],pinned:[]}, savedLayout={order:[],pinned:[]}, layoutQueue=Promise.resolve(), layoutRevision=0;
const attentionRooms=new Set(), seenLiveMessages=new Set();
let tabDrag=null, tabScrollFrame=null;
const previewCache=new Map();
let previewEpoch=0;
const previewObserver=typeof IntersectionObserver==='function'?new IntersectionObserver(entries=>{
  for(const entry of entries)if(entry.isIntersecting){previewObserver.unobserve(entry.target);if(unlocked && settings?.link_previews!==false)entry.target._load?.();}
},{root:$('timeline'),rootMargin:'100px'}):null;
let friendsRenderKey='',lastTabSelection=null;
const panelOpeners=new Map();
const dialogReturnFocus=new Map();
const introducedProfiles=new Set();
const replyDrafts = new Map(), messageCache = new Map();
let talkRoom = null, talkTimer = null, talkSource = null, searchGeneration = 0, searchOffset = 0, searchRoom = null, notificationRoom = null;
let talkSequence=Date.now()*1000;
const readMarkers = new Map(), selectedMentions = new Map();
let detailsModal=false, navigationModal=true, restoreDetailsOnWide=false, detailsClosingForResize=false;
let inspectedPeer=null, inspectedRoom=null;
let readTimer = null, viewTimer = null, mentionOptions = [], mentionIndex = 0, mentionRange = null, historyLoading = false;
function atBottom() { const timeline=$('timeline');return timeline.scrollHeight-timeline.scrollTop-timeline.clientHeight<60; }
function readingRoom() {return unlocked && document.body.dataset.homeOpen!=='true' && document.body.dataset.voiceView!=='true' && !!selectedRoom && !historyLoading && !hasBlockingDialog() && document.visibilityState !== 'hidden' && document.hasFocus() && atBottom();}
function reportView() {if(!tauri)return;invoke('chat_view',{room:unlocked?selectedRoom:null,channel:selectedChannel(),reading:readingRoom()}).catch(()=>{});}
function scheduleRead() {
  acknowledgeConversation();
  clearTimeout(viewTimer);viewTimer=setTimeout(reportView,100);
  if(readTimer || !readingRoom())return;
  const room=selectedRoom,scope=draftKey(),channel=selectedChannel(),generation=sessionGeneration;
  readTimer=setTimeout(async()=>{
    readTimer=null;if(!readingRoom() || scope!==draftKey() || generation!==sessionGeneration)return;
    const rows=[...$('timeline').querySelectorAll('.message[data-message-id]')];if(!rows.length)return;
    const through=Math.max(...rows.map(row=>Number(row.dataset.timestamp)));
    const ids=rows.filter(row=>Number(row.dataset.timestamp)===through).map(row=>row.dataset.messageId);
    try {const marker=await invoke('mark_read',{room,channel,through,ids});if(!unlocked || generation!==sessionGeneration || scope!==draftKey())return;readMarkers.set(scope,marker);rows.forEach(row=>row.classList.remove('unread'));$('timeline').querySelector('.unread-divider')?.remove();const remaining=$('timeline').querySelectorAll('.message.unread').length;if(remaining)unread.set(room,remaining);else unread.delete(room);roomsKey='';renderRooms();if(channel){const counts=await invoke('unread_counts');if(unlocked && generation===sessionGeneration && counts && typeof counts==='object'){for(const id of Object.keys(counts))unread.set(id,counts[id]);if(!counts[room])unread.delete(room);roomsKey='';renderRooms();}const view=await invoke('channel_view',{room});if(unlocked && generation===sessionGeneration && scope===draftKey() && Array.isArray(view?.channels))window.OchatChannels?.setUnread(view.unread || {},view.channels);}}catch{}
  },2000);
}
function isUnread(event) {if(event.peer===state?.id)return false;const marker=readMarkers.get(draftKey(event.room || selectedRoom,eventChannel(event) || selectedChannel(event.room || selectedRoom)));return !marker || event.timestamp>marker.through || (event.timestamp===marker.through && !marker.ids.includes(event.id));}
function mentionCandidates() {const members=(state?.members || state?.peers || []);return members.map(peer=>({...peer,token:members.filter(p=>p.name===peer.name).length>1?`${peer.name}#${peer.id.slice(0,8)}`:peer.name}));}
function closeMentions() {$('mention-picker').hidden=true;mentionOptions=[];mentionRange=null;}
function renderMentionPicker() {
  const picker=$('mention-picker');picker.replaceChildren();
  mentionOptions.forEach((peer,index)=>{const button=el('button',`mention-option${index===mentionIndex?' selected':''}`);button.type='button';button.setAttribute('role','option');button.setAttribute('aria-selected',String(index===mentionIndex));button.append(el('strong','',peer.name),el('small','',`${peer.online===false?'Offline':'Online'}${peer.token!==peer.name?' · '+peer.id.slice(0,8):''}`));button.onmousedown=event=>event.preventDefault();button.onclick=()=>chooseMention(index);picker.append(button);});picker.hidden=!mentionOptions.length;positionMentions();
}
function updateMentions() {
  const field=$('message'),before=field.value.slice(0,field.selectionStart),match=/(?:^|\s)@([^@\n]*)$/.exec(before);
  if(!isActiveRoom() || !match || match[1].length>64){closeMentions();return;}
  const query=match[1].toLocaleLowerCase();mentionOptions=mentionCandidates().filter(peer=>peer.name.toLocaleLowerCase().includes(query) || peer.token.toLocaleLowerCase().includes(query)).slice(0,8);mentionIndex=0;mentionRange=[before.lastIndexOf('@'),field.selectionStart];renderMentionPicker();
}
function chooseMention(index) {const peer=mentionOptions[index],range=mentionRange;if(!peer || !range)return;let mentions=selectedMentions.get(draftKey());if(!mentions){mentions=new Map();selectedMentions.set(draftKey(),mentions);}mentions.set(peer.token,{peer:peer.id,name:peer.token});$('message').setRangeText(`@${peer.token} `,range[0],range[1],'end');closeMentions();$('message').focus();onInput();}
function mentionBoundary(text,index,length) {const before=text.slice(0,index).at(-1),after=[...text.slice(index+length)][0];return (!before || /\s|[([{\"']/.test(before)) && (!after || !/[\p{L}\p{N}_#]/u.test(after));}
function containsMention(text,name) {const token=`@${name}`;let start=0,index;while((index=text.indexOf(token,start))>=0){if(mentionBoundary(text,index,token.length))return true;start=index+token.length;}return false;}
function mentionsForDraft(text) {const mentions=new Map(selectedMentions.get(draftKey()) || []);for(const peer of mentionCandidates()) if(containsMention(text,peer.token))mentions.set(peer.token,{peer:peer.id,name:peer.token});return [...mentions.values()].filter(mention=>containsMention(text,mention.name));}
function appendMentionText(container,text,mentions) {
  let index=0;const tokens=mentions.map(m=>({token:`@${m.name}`,peer:m.peer})).sort((a,b)=>b.token.length-a.token.length);
  while(index<text.length){let found=null;for(const token of tokens){let position=text.indexOf(token.token,index);while(position>=0 && !mentionBoundary(text,position,token.token.length))position=text.indexOf(token.token,position+token.token.length);if(position>=0 && (!found || position<found.position))found={...token,position};}if(!found){container.append(document.createTextNode(text.slice(index)));break;}container.append(document.createTextNode(text.slice(index,found.position)));const mark=el('span','mention-token',found.token);mark.title=found.peer;container.append(mark);index=found.position+found.token.length;}
}
function messageKey(room, reference) {return `${room}:${reference.peer}:${reference.id}`;}
// Resolve people by public identity, never by a display name or a saved room title.
function snapshotFor(room) {
  return state?.room?.id===room ? state : roomSnapshots.get(room);
}
function conversationContext(id) {
  const snapshot=snapshotFor(id),room=snapshot?.room || allRooms().find(room=>room.id===id);
  const known=[...(snapshot?.members || roomMembers.get(id) || []),...(snapshot?.peers || [])];
  const members=[...new Map(known.map(peer=>[peer.id,peer])).values()].filter(peer=>peer.id!==state?.id).map(peer=>{
    const live=snapshot?.peers?.find(current=>current.id===peer.id);
    return {...peer,tor_enabled:live?.tor_enabled??null,online:!!live && live.online!==false,voice:!!live?.voice,speaking:!!live?.speaking};
  });
  const direct=room?.capacity===2;
  const friend=direct ? friends.find(friend=>friend.direct_room===id) : null;
  const connected=(snapshot?.peers || []).filter(peer=>peer.id!==state?.id);
  const peer=direct ? (friend ? {id:friend.peer,name:friend.name} : connected.length===1 ? connected[0] : members.length===1 ? members[0] : null) : null;
  const person=peer ? peerDetails(peer.id,id,peer.name) : null;
  return {room,snapshot,members,direct,peer:person,title:person?.name || room?.name || 'Ochat'};
}
function peerDetails(id,room,fallback='Unknown participant') {
  if(id===state?.id)return {id,tor_enabled:!!settings?.tor_enabled,name:state.name,avatar_hash:state.avatar_hash,bio:state.bio || '',online:!!snapshotFor(room),voice:!!activeVoice(),speaking:!!activeVoice()?.speaking && !activeVoice()?.muted && !activeVoice()?.deafened,muted:activeVoice()?.muted,deafened:activeVoice()?.deafened,route:'self'};
  const snapshots=[state,...roomSnapshots.values()].filter(snapshot=>snapshot?.room);
  const local=snapshotFor(room)?.peers?.find(peer=>peer.id===id);
  const live=local || snapshots.flatMap(snapshot=>snapshot.peers || []).find(peer=>peer.id===id);
  const saved=(snapshotFor(room)?.members || roomMembers.get(room) || []).find(peer=>peer.id===id)
    || [...roomMembers.values()].flat().find(peer=>peer.id===id);
  const friend=friends.find(friend=>friend.peer===id);
  const vocal=snapshots.flatMap(snapshot=>snapshot.peers || []).find(peer=>peer.id===id && peer.voice);
  const profile=live || (friend?.profile_known ? friend : saved),online=!!live && live.online!==false || !!friend?.presence_online;
  return {id,tor_enabled:online?(live?.tor_enabled??friend?.tor_enabled??null):null,name:profile?.name || friend?.name || fallback,availability:live?.availability||friend?.availability,bio:profile?.bio??'',avatar_hash:profile?.avatar_hash,online,voice:!!vocal,speaking:!!vocal?.speaking && vocal?.muted!==true && vocal?.deafened!==true,muted:vocal?.muted,deafened:vocal?.deafened,route:live?.route || (friend?.presence_online?friend.route:null),latency_ms:live?.latency_ms,here:!!local};
}
function renderIdentity(room) {
  const context=conversationContext(selectedRoom),person=context.peer;
  if(inspectedRoom!==selectedRoom){window.OchatProfiles?.close(false);inspectedPeer=null;inspectedRoom=selectedRoom;}
  const channel=channelContextFor()?.channels?.find(c=>c.id===selectedChannel());
  $('room-title').textContent=!context.direct && channel?'# '+channel.name:context.title;
  $('room-title').title=channel?context.title+' → '+channel.name:context.title;
  $('conversation-kind').textContent=!room?'Your conversations':context.direct?'Private chat':'Group conversation';
  if(context.direct && person)$('room-subtitle').dataset.presence=person.here?'online':'offline';else delete $('room-subtitle').dataset.presence;
  $('room-subtitle').textContent=!room?'Choose a friend or open a group':!context.snapshot?'Disconnected · saved messages':person?`${person.here?'Online in this chat':'Offline in this chat'} · ${person.id.slice(0,8)}`:context.direct?'Waiting for the other participant':`${context.members.length+1} members · ${(context.snapshot.peers?.length || 0)+1} online`;
  profileAvatar($('conversation-avatar'),person?.name || room?.name || 'Ochat',context.direct?person?.avatar_hash:context.room?.image_hash,person?.id || '');
  window.OchatCommunity?.badge($('conversation-identity'),context.direct?person:null);
  const group=!!room && !context.direct;
  $('room-illustration-section').hidden=!group;
  if(group){profileAvatar($('room-illustration'),room.name,context.room?.image_hash);$('illustration-name').textContent=room.name;const owner=context.room?.owner===state.id;$('room-illustration-actions').hidden=!owner;$('illustration-hint').textContent=owner?'Shared with everyone. Choose a static PNG, JPEG or WebP image.':'The room owner manages this shared image.';$('choose-room-illustration').disabled=!context.snapshot || !!$('choose-room-illustration').dataset.busy;$('remove-room-illustration').hidden=!context.room?.image_hash;$('remove-room-illustration').disabled=!context.snapshot || !!$('remove-room-illustration').dataset.busy;}
  $('conversation-avatar').classList.toggle('group-avatar',!!room && !context.direct);
  $('conversation-identity').disabled=!room;
  $('conversation-identity').setAttribute('aria-label',person?`View ${person.name}'s profile`:'View conversation details');
  $('details-toggle').textContent=context.direct && person?'Profile':'Members';
  $('room-actions-name').textContent=room?context.title:'';
}
function clearPeerCard() {
  $('peer-card').hidden=true;$('profile-back').hidden=true;
  for(const key of ['avatarHash','avatarName','avatarPeer'])delete $('peer-avatar').dataset[key];
  delete $('peer-voice').dataset.speaking;
  for(const id of ['peer-avatar','peer-name','peer-presence','peer-connection','peer-voice','peer-relationship','peer-identity'])$(id).textContent='';
  for(const id of ['peer-message','peer-add','peer-copy'])$(id).onclick=null;
}
function renderPeerCard() {
  if(!unlocked || !state)return;
  if(inspectedRoom!==selectedRoom){window.OchatProfiles?.close(false);inspectedPeer=null;inspectedRoom=selectedRoom;}
  const context=conversationContext(selectedRoom),target=inspectedPeer || context.peer;
  const person=target ? peerDetails(target.id,target.room || selectedRoom,target.name) : null;
  const floating=!!inspectedPeer?.floating;const focusedMember=!floating && !!inspectedPeer && (!context.direct || context.peer?.id!==person?.id);
  $('details-title').textContent=person&&!floating?'Profile':context.direct?'Private chat':'Group details';
  $('profile-back').hidden=!focusedMember || !context.room;
  $('profile-back').textContent=context.direct?'← Back to chat profile':'← Back to members';
  $('details-overview').hidden=focusedMember;
  $('participants-section').hidden=!!context.direct && !!context.peer;
  $('peer-card').hidden=!person;
  if(!person){clearPeerCard();return;}
  const self=person.id===state.id,friend=friends.find(friend=>friend.peer===person.id);
  profileAvatar($('peer-avatar'),person.name,person.avatar_hash,person.id);
  $('peer-name').textContent=person.name;window.OchatPersonalization?.profile(person);window.OchatCommunity?.badge($('peer-name'),person);
  $('peer-presence').textContent=person.online?({away:'Away',busy:'Do not disturb'})[person.availability]||'Online':'Offline';
  $('peer-presence').classList.toggle('online',person.online);
  const route=self?'self':!person.online?'offline':person.route==='direct'?'direct':person.route==='relay'?'relay':'unknown';
  const connection=$('peer-connection');connection.dataset.route=route;connection.textContent=route==='self'?'This device':route==='offline'?'Disconnected':route==='direct'?'Direct':route==='relay'?'Relayed':'Connected · route unavailable';
  if(!self && person.online && Number.isFinite(person.latency_ms) && person.latency_ms>0)connection.textContent+=' · '+Math.round(person.latency_ms)+' ms';
  connection.title=route==='relay'&&settings?.tor_enabled?'Encrypted connection through Tor on this device and an Iroh relay':person.online&&!self?'Current encrypted connection from this device to this peer':route==='offline'?'No active connection to this peer':'This is your local account';
  $('peer-voice').textContent=person.voice&&person.deafened?'Deafened':person.voice&&person.muted?'Microphone muted':person.speaking?'Speaking now':person.voice?'In a voice chat':'Not in voice';
  $('peer-voice').dataset.speaking=String(person.speaking); window.OchatFeatures?.profile(person);
  $('peer-relationship').textContent=self?'You':friend?.status==='accepted'?'Friend':friend?.status==='incoming'?'Sent you a friend request':friend?.status==='outgoing'?'Your request is pending':'Not in your friends';
  $('peer-identity').textContent=person.id;
  $('verify-peer-button').hidden=self;
  const verified=verificationCache.get(person.id);$('peer-verification-status').textContent=self?'This is your identity':verified?.verified?'Verified identity':'Identity not verified';
  if(!self && !verificationCache.has(person.id)){verificationCache.set(person.id,{loading:true});const generation=sessionGeneration;invoke('peer_verification',{peer:person.id}).then(data=>{if(unlocked && generation===sessionGeneration){verificationCache.set(person.id,data);if($('peer-identity').textContent===person.id)$('peer-verification-status').textContent=data?.verified?'Verified identity':data?.same_name_verified?'Different identity with a familiar name — compare again.':'Identity not verified';}}).catch(()=>verificationCache.delete(person.id));}

  $('peer-copy').onclick=()=>action($('peer-copy'),async()=>{await invoke('copy_text',{text:person.id});toast('Public identity copied.');});
  const message=$('peer-message'),add=$('peer-add');
  message.hidden=self || friend?.status!=='accepted' || (context.direct && context.peer?.id===person.id);
  message.disabled=!!message.dataset.busy;
  message.onclick=()=>action(message,async()=>{window.OchatProfiles?.close(false);closePanel('details-dialog',false);await messageFriend(friend);});
  add.hidden=self || friend?.status==='accepted' || friend?.status==='outgoing';
  add.disabled=!!add.dataset.busy;add.textContent=friend?.status==='incoming'?'Accept friend request':'Add friend';
  add.onclick=()=>action(add,async()=>{if(friend?.status==='incoming')await invoke('respond_friend',{peer:person.id,accept:true});else await invoke('add_friend',{peer:person.id});await refreshFriends();});
  window.OchatWorkspace?.render();
}
function showPeer(id,name,room=selectedRoom,opener=document.activeElement) {
  if(!unlocked)return;
  const floating=!!conversationContext(room).room && !conversationContext(room).direct;
  if(!floating)window.OchatProfiles?.close(false);
  inspectedPeer={id,name,room,floating};inspectedRoom=selectedRoom;
  if(floating){releaseTalk();renderPeerCard();window.OchatProfiles?.open(opener);return;}
  restoreDetailsOnWide=false;const origin=document.activeElement,returnFocus=$('navigation-dialog').contains(origin) && navigationModal?(panelOpeners.get('navigation-dialog') || $('navigation-toggle')):origin,dialog=$('details-dialog');renderPeerCard();
  releaseTalk();prepareDialog(dialog.id);panelOpeners.set(dialog.id,returnFocus);
  detailsModal=true;dialog.classList.add('profile-overlay');
  if(dialog.open)dialog.close();dialog.showModal();
  $('details-toggle').setAttribute('aria-expanded','true');dialog.scrollTop=0;
}
async function refreshFriends() {
  const generation=++friendGeneration,session=sessionGeneration,result=await invoke('list_friends');
  if(!unlocked || session!==sessionGeneration || generation!==friendGeneration)return;
  friends=Array.isArray(result)?result:[];membersKey='';render();
}
function refreshFriendsFromNavigation() {
  const generation=sessionGeneration;
  refreshFriends().catch(error=>{if(unlocked && generation===sessionGeneration)toast(error);});
}
async function messageFriend(friend) {
  releaseTalk();const generation=sessionGeneration;
  try{const room=await invoke('open_direct',{peer:friend.peer});if(!unlocked || generation!==sessionGeneration)return;deletedRooms.delete(room);await refreshRooms();await selectRoom(room);await refreshFriends();}
  catch(error){if(unlocked && generation===sessionGeneration)toast(`Could not reach ${friend.name}. They must be online. ${error}`);}
}
async function retryFriendRequest(friend,button) {
  if(!unlocked || friendRetries.has(friend.peer))return;
  const generation=sessionGeneration;friendRetries.add(friend.peer);
  try{await action(button,async()=>{
    const result=await invoke('retry_friend_request',{peer:friend.peer});
    if(!unlocked || generation!==sessionGeneration)return;
    await refreshFriends();if(!unlocked || generation!==sessionGeneration)return;
    toast(result.status!=='outgoing'?'This request is no longer pending.':result.request_error?'The contact is still unreachable. Open Connection details for the last error.':result.delivered?'Request delivered. Waiting for your friend to accept.':'Request queued. Keep both accounts unlocked; Ochat will retry automatically.');
  });}finally{if(unlocked && generation===sessionGeneration){friendRetries.delete(friend.peer);friendsRenderKey='';renderFriends();}}
}
function renderFriends() {
  const key=JSON.stringify([friends,[state,...roomSnapshots.values()].flatMap(s=>s?.peers?.map(p=>[p.id,p.name,p.avatar_hash,p.tor_enabled,p.online])||[])]);if(key===friendsRenderKey)return;friendsRenderKey=key;
  const incoming=friends.filter(f=>f.status==='incoming'),outgoing=friends.filter(f=>f.status==='outgoing'),accepted=friends.filter(f=>f.status==='accepted');
  const label=`Friends${incoming.length?' ('+incoming.length+')':''}`;$('friends-button').textContent=label;$('navigation-friends-button').textContent=label;
  const container=$('friend-list');container.replaceChildren();
  for(const [id,title,entries,empty] of [['received-friend-list','Received requests',incoming,'No received requests.'],['sent-friend-list','Sent requests',outgoing,'No sent requests.'],['accepted-friend-list','Friends',accepted,'Add a friend using their contact ID.']]){
    const section=el('section','friend-section'),heading=el('h3','',title),count=el('span','',String(entries.length)),list=el('div','');list.id=id;heading.append(count);section.append(heading,list);container.append(section);
    for(const friend of entries){
      const person=peerDetails(friend.peer,friend.direct_room,friend.name),online=person.online,row=el('div','friend-row'),profile=el('button','friend-heading'),avatar=el('span','avatar'),dot=el('span',`presence-dot ${online?'online':'offline'}`);
      row.dataset.peer=friend.peer;row.dataset.search=(person.name+' '+friend.peer).toLocaleLowerCase();profile.type='button';profile.setAttribute('aria-label',`View ${person.name}'s profile`);profile.onclick=()=>showPeer(friend.peer,person.name,friend.direct_room);profileAvatar(avatar,person.name,person.avatar_hash,person.id);dot.setAttribute('role','img');dot.setAttribute('aria-label',online?'Online':'Offline');avatar.append(dot);profile.append(avatar,el('strong','',person.name));window.OchatCommunity?.badge(profile,person);row.append(profile);
      if(friend.status==='accepted'){
        row.append(el('small','friend-presence',online?'Online':'Offline'));
        const controls=el('div','friend-controls'),message=el('button','text-button','Message'),remove=el('button','text-button danger','Remove');message.onclick=()=>action(message,()=>messageFriend(friend));remove.onclick=()=>confirm('Remove friend?',`Remove ${friend.name} from your friends? Saved messages remain on this device.`,'Remove',async()=>{await invoke('respond_friend',{peer:friend.peer,accept:false});await refreshFriends();});controls.append(message,remove);row.append(controls);
      }else if(friend.status==='incoming'){
        row.append(el('small','','Wants to add you'));const controls=el('div','friend-controls');for(const accept of [true,false]){const button=el('button','text-button',accept?'Accept':'Decline');button.type='button';button.onclick=()=>action(button,async()=>{await invoke('respond_friend',{peer:friend.peer,accept});await refreshFriends();});controls.append(button);}row.append(controls);
      }else {
        row.append(el('small','request-status',friend.request_delivered?'Request sent · awaiting acceptance':'Waiting to reach this contact'));
        const controls=el('div','friend-controls'),retry=el('button','text-button',friendRetries.has(friend.peer)?'Retrying…':'Retry');retry.type='button';retry.disabled=friendRetries.has(friend.peer);retry.setAttribute('aria-label',`Retry friend request to ${person.name}`);retry.onclick=()=>retryFriendRequest(friend,retry);controls.append(retry);row.append(controls);
        if(!friend.request_delivered)row.append(el('small','request-help','Keep both accounts unlocked. If needed, add this contact again using a freshly copied contact ID.'));
        if(friend.request_error){const details=el('details','request-error'),summary=el('summary','','Connection details');details.append(summary,el('p','',friend.request_error));row.append(details);}
      }
      const identity=el('small','friend-identity',friend.peer.slice(0,8));identity.title=friend.peer;row.append(identity);list.append(row);
    }
    if(!entries.length)list.append(el('p','empty-small',empty));
  }
  window.OchatCommunity?.friends({accepted:accepted.length,incoming:incoming.length,outgoing:outgoing.length});
}
function beginReply(event) {replyDrafts.set(draftKey(),event);renderReply();$('message').focus();}
function renderReply() {
  const reply=replyDrafts.get(draftKey());$('reply-draft').hidden=!reply;
  $('reply-author').textContent=reply?`Replying to ${reply.peer===state?.id?'yourself':reply.name}`:'';
  $('reply-excerpt').textContent=reply?reply.text.slice(0,160):'';
}
function focusMessage(reference) {
  const row=[...$('timeline').querySelectorAll('[data-message-id]')].find(row=>row.dataset.messageId===reference.id && row.dataset.peer===reference.peer);
  if(!row){toast('The original message is unavailable on this device.');return false;}
  window.OchatTimeline?.pause();row.scrollIntoView?.({block:'center',behavior:'instant'});row.classList.add('message-target');row.tabIndex=-1;row.focus({preventScroll:true});setTimeout(()=>row.classList.remove('message-target'),2000);return true;
}
async function jumpMessage(room,reference) {
  const generation=sessionGeneration;
  releaseTalk();
  if(room!==selectedRoom)await selectRoom(room);
  if(!unlocked || generation!==sessionGeneration || selectedRoom!==room || deletedRooms.has(room))return;
  if(channelContextFor(room)?.channels?.length){const entry=await invoke('message_by_id',{room,reference});if(!unlocked || generation!==sessionGeneration || selectedRoom!==room)return;const channel=entry?.event?.channel;if(channel && channel!==selectedChannel(room))await selectTextChannel(channel);}
  const visible=[...$('timeline').querySelectorAll('[data-message-id]')].some(row=>row.dataset.messageId===reference.id && row.dataset.peer===reference.peer);
  if(visible)focusMessage(reference);else await loadRoom(room,reference);
}
function messageSummary(text) { return /^ochat-image:[a-f0-9]{64}$/.test(text||'')?'Image':text||''; }
function replyQuote(room,reference) {
  const button=el('button','reply-quote','Original message unavailable');button.type='button';button.title='Go to original message';button.dataset.replyId=reference.id;button.dataset.replyPeer=reference.peer;button.dataset.replyRoom=room;
  const fill=event=>{button.replaceChildren(el('strong','',event.peer===state?.id?'You':event.name),el('span','',event.deleted?'Message deleted':messageSummary(event.text).slice(0,160)));};
  const cached=messageCache.get(messageKey(room,reference));if(cached)fill(cached);
  else {const generation=sessionGeneration,history=historyGeneration;invoke('message_by_id',{room,reference}).then(entry=>{if(!unlocked || generation!==sessionGeneration || history!==historyGeneration || deletedRooms.has(room) || !entry?.event)return;messageCache.set(messageKey(room,reference),entry.event);fill(entry.event);}).catch(()=>{});}
  button.onclick=()=>action(button,()=>jumpMessage(room,reference));return button;
}
async function runSearch(offset=0) {
  const query=$('search-query').value.trim();if(!query)return;
  const generation=++searchGeneration,session=sessionGeneration;
  const room=$('search-scope').value==='room'?searchRoom:null;
  $('search-results').replaceChildren();$('search-status').textContent='Searching…';$('search-next').hidden=$('search-previous').hidden=true;
  try {
    const channel=room?$('search-channel')?.value || null:null;const page=await invoke('search_history',{query,room,channel,offset});
    if(!unlocked || session!==sessionGeneration || generation!==searchGeneration || !$('search-dialog').open)return;
    searchOffset=page.offset;$('search-status').textContent=page.total?`${page.offset+1}–${Math.min(page.offset+50,page.total)} of ${page.total} results`:'No saved messages match your search.';
    for(const entry of page.entries){if(deletedRooms.has(entry.room))continue;const event=entry.event,button=el('button','search-result');button.type='button';const roomName=rooms.find(r=>r.id===entry.room)?.name || 'Saved room';const channelName=snapshotFor(entry.room)?.channels?.find(c=>c.id===event.channel)?.name;const name=roomName+(channelName?' / #'+channelName:'');button.append(el('strong','',`${event.name} · ${name}`),el('time','',new Date(entry.timestamp).toLocaleString('en-US')),el('span','',messageSummary(event.text)));button.onclick=()=>action(button,async()=>{$('search-dialog').close();if(selectedRoom!==entry.room)await selectRoom(entry.room);if(event.channel && selectedChannel()!==event.channel)await selectTextChannel(event.channel);await jumpMessage(entry.room,{id:event.id,peer:event.peer});});$('search-results').append(button);}
    $('search-previous').hidden=page.offset===0;$('search-next').hidden=page.offset+50>=page.total;
  }catch(error){if(unlocked && generation===searchGeneration)$('search-status').textContent=String(error);}
}
function renderTalk() {
  const voice=activeVoice();const active=unlocked && !!voice?.voice && settings?.audio.push_to_talk;
  if(talkRoom && (!active || voice.muted || talkRoom!==voice.room.id))releaseTalk();
  $('talk-button').hidden=!active;$('talk-button').disabled=!!voice?.muted;
  $('talk-button').textContent=talkRoom?'Talking…':'Hold to talk';$('talk-button').setAttribute('aria-label',talkRoom?'Talking, release to stop':'Hold to talk');$('talk-button').setAttribute('aria-pressed',String(!!talkRoom));
}
function releaseTalk() {
  const room=talkRoom;talkRoom=null;talkSource=null;clearInterval(talkTimer);talkTimer=null;
  if(room)invoke('hold_to_talk',{room,pressed:false,sequence:++talkSequence}).catch(()=>{});
  if($('talk-button')){$('talk-button').textContent='Hold to talk';$('talk-button').setAttribute('aria-label','Hold to talk');$('talk-button').setAttribute('aria-pressed','false');}
  if(unlocked && activeVoice()?.voice && settings?.audio.push_to_talk)$('voice-hint').textContent=activeVoice().muted?'Microphone muted · listening only':'Push-to-talk · listening';
}
function pressTalk(source) {
  const voice=activeVoice();if(talkRoom || !unlocked || !voice?.voice || voice.muted || !settings?.audio.push_to_talk || hasBlockingDialog() || document.visibilityState==='hidden')return false;
  talkRoom=voice.room.id;talkSource=source;const room=talkRoom,generation=sessionGeneration;
  const pulse=()=>{if(!unlocked || generation!==sessionGeneration || room!==talkRoom)return;invoke('hold_to_talk',{room,pressed:true,sequence:++talkSequence}).catch(error=>{if(room===talkRoom){releaseTalk();toast(error);}});};pulse();talkTimer=setInterval(pulse,250);renderTalk();$('voice-hint').textContent='Transmitting while held';return true;
}
function editableTarget(target) {return target?.closest?.('input,textarea,select,[contenteditable="true"]');}
function syncKeyed(container,items,key,create,update) {
  const existing=new Map([...container.children].map(node=>[node.dataset.key,node]));
  const wanted=new Set(items.map(key));
  for(const node of [...container.children])if(!wanted.has(node.dataset.key))node.remove();
  items.forEach((item,index)=>{const id=key(item);let node=existing.get(id);if(!node){node=create(item);node.dataset.key=id;}update(node,item);if(container.children[index]!==node)container.insertBefore(node,container.children[index] || null);});
}
function emptyList(container,text) {if(!container.children.length)container.append(el('p','empty-small',text));}
function speechSlot() {const slot=el('span','speech-slot'),dot=speakingIcon();slot.append(dot);return {slot,dot};}
function setSpeaking(dot,active) {dot.hidden=!active;dot.setAttribute('aria-hidden',String(!active));}
function createMember() {
  const row=el('div','member');row._avatar=el('span','avatar');row._presence=el('span','presence-dot');row._presence.setAttribute('role','img');
  row._info=el('div','member-info');row._name=el('strong','member-name');row._nameText=el('span','');const speech=speechSlot();row._speech=speech.dot;row._name.append(row._nameText,speech.slot);row._meta=el('small','member-meta');row._info.append(row._name,row._meta);
  row._profile=el('button','member-profile');row._profile.type='button';row._profile.append(row._avatar,row._info);
  row._friend=el('button','text-button friend-action');row._friend.type='button';row.append(row._profile,row._friend);return row;
}
function updateMember(row,peer) {
  const online=peer.online!==false;row.classList.toggle('offline',!online);row._nameText.textContent=peer.name;
  profileAvatar(row._avatar,peer.name,peer.avatar_hash,peer.id);if(row._presence.parentNode!==row._avatar)row._avatar.append(row._presence);
  row._presence.className=`presence-dot ${online?'online':'offline'}`;row._presence.setAttribute('aria-label',online?'Online':'Offline');
  row._meta.textContent=!online?'Offline':peer.voice?'In voice':'Online';
  setSpeaking(row._speech,online && peer.voice && peer.speaking);window.OchatCommunity?.badge(row._name,peer);
  row._profile.setAttribute('aria-label',`View ${peer.name}'s profile`);row._profile.onclick=()=>showPeer(peer.id,peer.name,selectedRoom,row._profile);
  const friend=friends.find(f=>f.peer===peer.id),button=row._friend;button.hidden=peer.id===state.id;button.textContent=friend?.status==='accepted'?'Message':friend?.status==='incoming'?'Accept':friend?.status==='outgoing'?'Pending':'Add friend';button.disabled=friend?.status==='outgoing';button.setAttribute('aria-label',`${button.textContent} ${peer.name}`);
  button.onclick=()=>action(button,async()=>{if(friend?.status==='accepted')await messageFriend(friend);else if(friend?.status==='incoming'){await invoke('respond_friend',{peer:peer.id,accept:true});await refreshFriends();}else{const sent=await invoke('add_friend',{peer:peer.id});await refreshFriends();toast(sent?'Friend request sent.':'Request saved. It will retry when the peer is reachable.');}});
}
function syncVoiceList(container,peers,room=selectedRoom) {
  syncKeyed(container,peers,p=>p.id,()=>{const row=el('button','voice-person');row.type='button';row._avatar=el('span','avatar voice-avatar');row._avatar.setAttribute('aria-hidden','true');row._name=el('span','voice-person-name');const speech=speechSlot();row._speech=speech.dot;row.append(row._avatar,row._name,speech.slot);return row;},(row,peer)=>{const person=peerDetails(peer.id,room,peer.name);profileAvatar(row._avatar,person.name,peer.avatar_hash || person.avatar_hash,peer.id);row._name.textContent=peer.name;row.classList.toggle('is-speaking',!!peer.speaking);setSpeaking(row._speech,!!peer.speaking);row.setAttribute('aria-label',`View ${peer.name}'s profile${peer.speaking?' · Speaking':''}`);row.onclick=()=>showPeer(peer.id,peer.name,room,row);});
  if(!peers.length)emptyList(container,'Nobody in voice.');
}
function activeVoice() {
  if(!unlocked)return null;
  const snapshots=new Map([...roomSnapshots]);if(state?.room)snapshots.set(state.room.id,state);
  return [...snapshots.values()].find(snapshot=>snapshot.room && !deletedRooms.has(snapshot.room.id) && snapshot.voice) || null;
}
function renderDock() {
  const voice=activeVoice();$('voice-bar').hidden=!voice;const active=!!voice;
  $('stop-voice').hidden=!active;
  if(!voice){releaseTalk();$('voice-roster').replaceChildren();renderTalk();updateVoiceTimer();return;}
  $('voice-label').textContent=conversationContext(voice.room.id).title+(voice.voice_channel?' → '+(voice.channels?.find(c=>c.id===voice.voice_channel)?.name || 'Voice'):'');$('stop-voice').textContent=voice.call?'Hang up':'Leave';$('stop-voice').title=voice.call?'Hang up':'Leave voice';$('stop-voice').setAttribute('aria-label',$('stop-voice').title);
  $('mute-button').textContent=voice.muted?'Unmute':'Mute';$('voice-hint').textContent=voice.muted?'Microphone muted · listening only':settings?.audio.push_to_talk?(talkRoom?'Transmitting while held':'Push-to-talk · listening'):'Microphone active';
  $('mute-button').title=$('voice-hint').textContent;$('mute-button').setAttribute('aria-pressed',String(!!voice.muted));
  const peers=[{id:voice.id,name:`${voice.name} (you)`,speaking:voice.speaking},...voice.peers.filter(p=>p.voice && (!voice.voice_channel || p.voice_channel===voice.voice_channel))];syncVoiceList($('voice-roster'),peers,voice.room.id);renderTalk();updateVoiceTimer();
}
function hasBlockingDialog() {return [...document.querySelectorAll('dialog[open]')].some(dialog=>dialog.id==='emoji-dialog'&&dialog.classList.contains('emoji-popover')?false:dialog.id==='navigation-dialog'?navigationModal:dialog.id==='details-dialog'?detailsModal:true);}
function closePanel(id,restore=true) {if(id==='navigation-dialog' && unlocked && window.innerWidth>=900)return;const dialog=$(id);if(!dialog?.open)return;if(!restore)panelOpeners.delete(id);dialog.close();}
function prepareDialog(id) {
  if(id!=='navigation-dialog')closePanel('navigation-dialog',false);
  if(id!=='room-actions-dialog')closePanel('room-actions-dialog',false);
  if(id!=='details-dialog' && window.innerWidth<1180)closePanel('details-dialog',false);
}
function openDetails() {
  $('details-dialog').classList.remove('profile-overlay');
  restoreDetailsOnWide=false;const dialog=$('details-dialog');if(dialog.open){closePanel(dialog.id);return;}
  inspectedPeer=null;inspectedRoom=selectedRoom;renderPeerCard();
  releaseTalk();prepareDialog(dialog.id);panelOpeners.set(dialog.id,$('details-toggle'));detailsModal=window.innerWidth<1180;
  if(detailsModal)dialog.showModal();else dialog.show();$('details-toggle').setAttribute('aria-expanded','true');
}
function luminance(rgb) {return rgb.map(value=>{value/=255;return value<=0.04045?value/12.92:((value+0.055)/1.055)**2.4;}).reduce((sum,value,index)=>sum+value*[0.2126,0.7152,0.0722][index],0);}
function contrast(a,b) {const values=[luminance(a),luminance(b)].sort((a,b)=>b-a);return (values[0]+0.05)/(values[1]+0.05);}
function mixColor(a,b,amount) {return a.map((value,index)=>Math.round(value*(1-amount)+b[index]*amount));}
function parseColor(hex) {return [1,3,5].map(index=>parseInt(hex.slice(index,index+2),16));}
function colorHex(rgb) {return '#'+rgb.map(value=>value.toString(16).padStart(2,'0')).join('');}
function accentTokens(hex,dark,surfaceColors=null) {
  const base=parseColor(/^#[\da-f]{6}$/i.test(hex)?hex:'#007AFF'),surfaces=(surfaceColors || (dark?['#313338','#2b2d31','#383a40','#2b2d31']:['#ffffff','#f2f3f5','#e3e5e8','#f2f3f5'])).map(parseColor),target=dark?[255,255,255]:[0,0,0],alpha=dark?0.18:0.12;
  let accent=base;
  for(let step=0;step<=100;step++){accent=mixColor(base,target,step/100);if(surfaces.every(surface=>contrast(accent,surface)>=4.5 && contrast(accent,mixColor(surface,accent,alpha))>=4.5))break;}
  const foreground=contrast(accent,[255,255,255])>=contrast(accent,[0,0,0])?[255,255,255]:[0,0,0];
  return {accent:colorHex(accent),foreground:colorHex(foreground),hover:colorHex(mixColor(accent,foreground[0]===255?[0,0,0]:[255,255,255],0.08)),selection:`rgba(${accent.join(',')},${alpha})`};
}
function applyAccent(hex) {
  const theme=document.documentElement.dataset.theme,dark=theme==='dark' || (theme==='system' && !!window.matchMedia?.('(prefers-color-scheme: dark)').matches),root=document.documentElement,computed=getComputedStyle(root),surfaces=['--bg','--panel','--field','--sidebar'].map(name=>computed.getPropertyValue(name).trim()),tokens=accentTokens(hex,dark,surfaces.every(value=>/^#[0-9a-f]{6}$/i.test(value))?surfaces:null);
  root.style.setProperty('--accent',tokens.accent);root.style.setProperty('--accent-hover',tokens.hover);root.style.setProperty('--accent-foreground',tokens.foreground);root.style.setProperty('--selection',tokens.selection);root.style.setProperty('--focus',tokens.accent+'33');
}
const welcomeTemplate = $('welcome').cloneNode(true);
const unicodeEmojis = '🫶 🥹 🫨 🫩 🫥 🫣 🤭 🥲 😮‍💨 😵‍💫 🙂‍↕️ 🙂‍↔️ 😶‍🌫️ 😌 😋 😛 😜 🤪 😝 🤑 🤐 🤢 🤮 🤧 🤒 🤕 🥶 🥵 🧐 🤓 🥸 😈 👿 😺 😸 😹 😻 😼 😽 🙀 😿 😾 🙈 🙉 🙊 👶 🧒 👦 👧 🧑 👨 👩 🧔 👴 👵 👮 👷 🧑‍💻 🧑‍🎨 🧑‍🚀 🧑‍🍳 👑 🎩 🧢 🕶️ 🦁 🐯 🐨 🐷 🐮 🐵 🐔 🐧 🐦 🦉 🦋 🐝 🐢 🐍 🐬 🦈 🐙 🦄 🐲 🌹 🌸 🌷 🌺 🌴 🌵 🍀 🍁 🍂 🍄 🌍 🌎 🌏 🌕 🌞 🌝 ⚡ 🌧️ ❄️ 🌊 🍎 🍏 🍊 🍋 🍌 🍉 🍇 🍓 🫐 🍒 🍑 🥭 🍍 🥑 🍅 🥕 🌶️ 🥐 🍞 🧀 🍔 🍟 🌭 🌮 🥙 🍣 🍜 🍝 🍰 🎂 🍫 🍿 🍦 🥤 🍷 🍺 🧋 ⚽ 🏀 🏈 ⚾ 🎾 🏐 🎱 🏓 🏸 🥊 🏆 🏅 🎮 🕹️ 🎲 🧩 🎸 🎹 🥁 🎤 🎬 🎨 🖌️ 🛫 🚀 🛸 🚗 🚕 🚌 🚲 🏍️ 🚆 ⛵ 🏖️ 🏔️ 🏠 🏡 🏙️ 💻 ⌨️ 🖱️ 📱 💡 📚 📖 📝 📌 📎 🔒 🔑 🛡️ 🧰 ⚙️ 🧪 🔬 🩺 💊 🧬 🩷 🩵 🩶 🤎 🧡 💖 💗 💓 💞 💕 💘 💝 💟 ☮️ ♻️ 🔔 🔕 📣 🚩 🏳️‍🌈 🏳️‍⚧️ 🫰 🫵 🤌 🤏 🤘 🤙 ✌️ 🖖 🖐️ ✋ 🤚 🫱 🫲 🫳 🫴 🫷 🫸 🦾 🦿 👣 💅 💃 🕺 🧘 🏃 🚶 🧍 🧎 👍🏻 👍🏼 👍🏽 👍🏾 👍🏿 🙏🏻 🙏🏼 🙏🏽 🙏🏾 🙏🏿 👋🏻 👋🏼 👋🏽 👋🏾 👋🏿 😀 😃 😄 😁 😆 😅 😂 🙂 🙃 😉 😊 😍 🥰 😘 😎 🤔 🤨 😐 😴 😭 😢 😤 😡 🤯 🥳 🤗 🤫 🫡 🫠 😇 🤖 👻 💀 🐱 🐶 🦊 🐼 🐸 🌻 🌈 ☀️ 🌙 ⭐ 🔥 ✨ ❤️ 💙 💜 💚 💛 🖤 🤍 💔 💯 ✅ ❌ 🎉 🎈 🎁 🎵 🎧 📷 🖼️ 🍕 ☕ 🍻 👍 👎 👏 🙌 👋 🤝 🙏 💪 👀 💬'.split(' ');

function el(tag, cls, text) { const node = document.createElement(tag); if (cls) node.className = cls; if (text !== undefined) node.textContent = text; return node; }
function initials(name = '') { return [...name.trim()].slice(0, 2).join('').toUpperCase(); }
function profileAvatar(node,name,hash,peer = '') {
  if (node.dataset.avatarHash === (hash || '') && node.dataset.avatarName === name && node.dataset.avatarPeer === peer) return;
  node.dataset.avatarHash = hash || ''; node.dataset.avatarName = name; node.dataset.avatarPeer = peer; node.textContent = initials(name);
  if (!hash) return;
  const generation = sessionGeneration, epoch = assetGeneration;
  if (!assetLoads.has(hash) && !assets.has(hash)) assetLoads.set(hash,invoke('emoji_asset',{hash}).then(data => { if (data && unlocked && generation === sessionGeneration && epoch === assetGeneration) assets.set(hash,data); return data; }).catch(() => null).finally(() => assetLoads.delete(hash)));
  Promise.resolve(assets.get(hash) || assetLoads.get(hash)).then(data => { if (!data || !unlocked || generation !== sessionGeneration || epoch !== assetGeneration || node.dataset.avatarHash !== hash) return; const image = el('img','profile-photo'); image.src = data; image.alt = '';image.draggable=false; node.querySelector('.profile-photo')?.remove(); node.prepend(image); });
}
function toast(text) { $('toast').textContent = String(text); $('toast').hidden = false; clearTimeout(toastTimer); toastTimer = setTimeout(() => { $('toast').hidden = true; }, 7000); }
function size(bytes) { if (bytes < 1024) return `${bytes} B`; if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`; if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`; return `${(bytes / 1024 ** 3).toFixed(2)} GiB`; }
function invoke(command, args = {}) { if (!tauri) return Promise.reject(new Error('Open Ochat in its native desktop application.')); return tauri.core.invoke(command, args); }
function show(id) {
  if(id!=='message-tools-dialog')closeMessageMenu(false);
  if(id!=='emoji-dialog')window.OchatEmojiPopover?.close(false);
  releaseTalk();const origin=document.activeElement,dialog=$(id);
  if(!dialog.open && !['navigation-dialog','details-dialog','room-actions-dialog'].includes(id))dialogReturnFocus.set(id,$('navigation-dialog').contains(origin)?$('navigation-toggle'):$('room-actions-dialog').contains(origin)?$('room-actions-toggle'):origin);
  prepareDialog(id);const error=dialog.querySelector('.form-error');if(error)error.textContent='';if(!dialog.open)dialog.showModal();
}
function isActiveRoom() { return !!state?.room && selectedRoom === state.room.id; }
function matchesPassword(first, second) { if ($(first).value !== $(second).value) throw new Error('Passwords do not match.'); }
function clearPasswords() {window.OchatPasswords?.reset();document.querySelectorAll('input[type="password"],input[data-private-password]').forEach(input => { input.value = ''; });}
async function action(button, fn) {
  if (button?.dataset.busy) return;
  if (button) { button.dataset.busy = 'true'; button.disabled = true; }
  try { return await fn(); } catch (error) { toast(error); }
  finally { if (button) { delete button.dataset.busy; button.disabled = false; } if (unlocked) render(); }
}
async function formAction(form, fn) {
  const button = form.querySelector('[type="submit"]'); if (button.disabled) return;
  const error = form.querySelector('.form-error'), label = button.textContent; error.textContent = ''; button.disabled = true; button.textContent = 'Working…';
  try { await fn(); } catch (e) { error.textContent = String(e); }
  finally { button.disabled = false; button.textContent = label; }
}
function confirm(title, description, label, callback) { $('confirm-title').textContent = title; $('confirm-description').textContent = description; $('confirm-action').textContent = label; confirmCallback = callback; show('confirm-dialog'); }
function applyTheme(theme = 'system', accent = settings?.accent_color || '#007AFF') { document.documentElement.dataset.theme = theme; document.documentElement.style.colorScheme = theme === 'system' ? 'light dark' : theme;applyAccent(accent);window.OchatPersonalization?.apply(settings?.appearance || {}); }
function setAuthMode(mode) {
  $('create-account-form').hidden = mode !== 'create'; $('unlock-form').hidden = mode !== 'unlock'; $('import-form').hidden = mode !== 'import';
  $('auth-title').textContent = mode === 'create' ? 'Create your account' : mode === 'unlock' ? 'Welcome back' : 'Import encrypted backup';
  $('auth-description').textContent = mode === 'import' ? 'Restore your profile, messages and emojis on this device.' : 'Your messages stay encrypted on this device.';
  $('auth-import').hidden = mode === 'import'; $('auth-back').hidden = mode !== 'import'; $('replace-row').hidden = !accountExists;
}
async function completeAccount(reply, generation = sessionGeneration) {
  if (generation !== sessionGeneration) { reply.recovery_key = null; return; }
  unlocked = true; state = reply.snapshot; settings = reply.settings; rooms = reply.rooms.filter(room => !deletedRooms.has(room.id)); membersKey = ''; roomsKey = ''; selectedRoom = state.room?.id || rooms[0]?.id || null;setNavigationSection('friends');
  const layout=await invoke('get_conversation_layout');
  if(!unlocked || generation!==sessionGeneration){reply.recovery_key=null;return;}
  conversationLayout=normalizeLayout(layout);savedLayout=normalizeLayout(layout);
  const restored=await invoke('room_snapshots');
  if(!unlocked || generation!==sessionGeneration)return;
  for(const snapshot of restored || [])if(snapshot.room){roomSnapshots.set(snapshot.room.id,snapshot);roomMembers.set(snapshot.room.id,snapshot.members || snapshot.peers);}
  hydrateRoomMembers();
  if(reply.shortcut_error)toast(reply.shortcut_error);
  await refreshFriends();if(!unlocked || generation!==sessionGeneration){reply.recovery_key=null;return;}applyTheme(settings.theme); clearPasswords(); $('auth-dialog').close(); setNavigationSection('friends');render(); const counts=await invoke('unread_counts');if(unlocked && generation===sessionGeneration && counts && typeof counts==='object')for(const [room,count] of Object.entries(counts)){unread.set(room,count);if(count>0)attentionRooms.add(room);}renderRooms();await loadRoom(selectedRoom);
  if (reply.recovery_key) { showRecovery(reply.recovery_key, true); reply.recovery_key = null; }
}
function showRecovery(key, required) { if (!unlocked) return; recoveryRequired = required; $('recovery-value').value = key; $('recovery-confirm').checked = false; $('recovery-confirm-row').hidden = !required; $('recovery-done').disabled = required; show('recovery-dialog'); }
async function lockAccount() {
  window.OchatFeatures?.reset();window.OchatPhotoCrop?.reset();window.OchatTimeline?.reset();chosenDraftEmojis.clear();
  window.OchatPresentation?.reset();window.OchatDiscord?.reset();restoreDetailsOnWide=false;
  clearMessageTools();
  pendingRoomMembers.clear();$('add-friend-form').querySelector('.form-error').textContent='';$('room-illustration').textContent='O';setNavigationSection('rooms');$('navigation-friends-button').textContent='Friends';
  previewEpoch++;previewCache.clear();previewObserver?.disconnect();
  endTabDrag();conversationLayout={order:[],pinned:[]};savedLayout={order:[],pinned:[]};layoutRevision++;layoutQueue=Promise.resolve();attentionRooms.clear();seenLiveMessages.clear();
  panelOpeners.clear();dialogReturnFocus.clear();introducedProfiles.clear();
  inspectedPeer=null;inspectedRoom=null;clearPeerCard();$('room-illustration-section').hidden=true;$('illustration-name').textContent='';$('conversation-avatar').textContent='O';delete $('room-subtitle').dataset.presence;$('conversation-identity').disabled=true;$('conversation-kind').textContent='Your conversations';$('room-actions-name').textContent='';$('connection-summary').textContent='';
  $('conversation-identity').setAttribute('aria-label','View conversation details');$('room-title').removeAttribute('title');$('message').setAttribute('aria-label','Message');$('message').placeholder='Write a draft…';$('send-button').title='Send message';$('voice-button').title='Join voice';delete $('peer-voice').dataset.speaking;
  releaseTalk();$('settings-button').disabled=true;$('navigation-toggle').disabled=true;$('friends-button').disabled=true;$('add-friend-button').disabled=true;$('copy-contact-id').disabled=true;$('copy-contact-id').textContent='Copy my contact ID';$('friends-button').textContent='Friends';$('room-voice-roster').replaceChildren();$('room-voice-elapsed').textContent='00:00';$('voice-label').textContent='Voice connected';$('voice-hint').textContent='';$('voice-elapsed').textContent='00:00';$('conversation-panel').removeAttribute('aria-labelledby');membersKey='';roomsKey='';friendsRenderKey='';lastTabSelection=null;$('private-conversation-tabs').replaceChildren();$('room-conversation-tabs').replaceChildren();$('private-tab-count').textContent='0';$('room-tab-count').textContent='0';$('details-toggle').disabled=true;$('room-actions-toggle').disabled=true;$('lock-button').disabled=true;friends=[];friendRetries.clear();friendGeneration++;$('friend-list').replaceChildren();$('talk-button').hidden=true;$('reconnect-panel').hidden=true;searchGeneration++;replyDrafts.clear();messageCache.clear();searchRoom=null;notificationRoom=null;$('search-results').replaceChildren();$('search-query').value='';$('search-status').textContent='';$('room-notifications-name').textContent='';renderReply();
  historyLoading=false;$('history-context').hidden=true;
  clearTimeout(readTimer);clearTimeout(viewTimer);readTimer=null;readMarkers.clear();selectedMentions.clear();closeMentions();
  window.OchatWorkspace?.reset();
  sessionGeneration++; stopTyping(); stopAudioPoll(); unlocked = false; window.OchatChannels?.reset(); window.OchatGifPicker?.reset();window.OchatPersonalization?.reset();window.OchatCommunity?.reset();emojiLoadGeneration++; historyGeneration++; selectedRoom = null; loadedRoom = null; state = null; settings = null; rooms = []; assets.clear(); assetLoads.clear(); drafts.clear(); liveOffers.clear(); shownMessages.clear();
  emojiEntries = []; editingEmoji = null; confirmCallback = null; recoveryRequired = false; roomSnapshots.clear(); archivedChannelViews.clear();archivedTextChannels.clear();roomMembers.clear(); unread.clear(); deletedRooms.clear(); assetGeneration++;
  $('voice-roster').replaceChildren(); $('voice-elapsed').hidden = true; $('delete-room').hidden = true;
  $('stop-voice').hidden = true; $('voice-button').hidden = false;
  document.querySelectorAll('dialog[open]').forEach(d => d.close());
  for (const id of ['recovery-value','invitation-output','invitation-input','import-key','message','room-name','emoji-name','account-name','friend-contact-id']) $(id).value = '';
  for (const id of ['call-description','confirm-description','settings-error','toast']) $(id).textContent = '';
  $('toast').hidden = true; clearPasswords();
  document.querySelectorAll('[data-avatar-hash]').forEach(node => { delete node.dataset.avatarHash; delete node.dataset.avatarName; delete node.dataset.avatarPeer; node.querySelector('.profile-photo')?.remove(); }); $('profile-photo-preview').textContent = 'O';
  $('timeline').replaceChildren(welcomeTemplate.cloneNode(true)); bindWelcome(); $('direct-room-list').replaceChildren();$('group-room-list').replaceChildren();$('direct-count').textContent='0';$('group-count').textContent='0'; $('members').replaceChildren(); $('custom-grid').replaceChildren(); $('typing-status').textContent = ''; $('identity-value').value = ''; $('profile-name').textContent = 'Account locked'; $('profile-avatar').textContent = 'O'; $('profile-status').textContent = 'Private peer-to-peer messaging'; $('room-title').textContent = 'Ochat'; $('room-subtitle').textContent = 'Account locked'; $('voice-bar').hidden = true; $('leave-button').hidden = true; $('reconnect-button').hidden = true;
  for (const id of ['forget-room','new-room','new-direct-chat','join-button','settings-button','lock-button','invite-button','attach-button','emoji-button','send-button','message','voice-button','shared-emojis-button','clear-history','search-button','room-notifications-button']) $(id).disabled = true;
  $('composer-status').textContent = 'Unlock your account to start.';window.OchatDiscord?.reset();
  try { await invoke('lock_account'); } finally { accountExists = await invoke('account_status'); setAuthMode(accountExists ? 'unlock' : 'create'); show('auth-dialog'); }
}

function render(next) {
  if (next) state = next;
  if (!unlocked || !state) return;
  const room = rooms.find(r => r.id === selectedRoom) || (state.room?.id === selectedRoom ? state.room : null);
  $('activity-button').disabled=false;
  $('settings-button').disabled=false;$('navigation-toggle').disabled=false;$('friends-button').disabled=false;$('add-friend-button').disabled=false;$('copy-contact-id').disabled=!!$('copy-contact-id').dataset.busy;$('lock-button').disabled=false;$('details-toggle').disabled=!selectedRoom;$('room-actions-toggle').disabled=!selectedRoom; $('profile-name').textContent = state.name; profileAvatar($('profile-avatar'),state.name,state.avatar_hash,state.id); $('profile-status').textContent = 'Your account';
  renderIdentity(room);
  for (const id of ['new-room','new-direct-chat','join-button','settings-button','lock-button','emoji-button']) $(id).disabled = false;
  $('invite-button').disabled = !isActiveRoom() || !!$('invite-button').dataset.busy;
  $('leave-button').hidden = !isActiveRoom(); $('reconnect-button').hidden = !room || isActiveRoom();$('reconnect-panel').hidden=!room || isActiveRoom();renderFriends();
  $('connection-summary').textContent = !room ? '' : 'End-to-end encrypted · only conversation participants can read messages';
  $('attach-button').disabled = !isActiveRoom() || !!$('attach-button').dataset.busy;
  $('shared-emojis-button').disabled = !selectedRoom;
  $('forget-room').disabled = !selectedRoom;$('clear-history').disabled = !selectedRoom;$('search-button').disabled = false;$('room-notifications-button').disabled = !selectedRoom;renderReply();
  $('delete-room').hidden = !room || room.owner !== state.id;
  const members = conversationContext(selectedRoom).members;
  $('member-total').textContent = `${isActiveRoom() ? state.peers.length + 1 : 0} online`;
  $('network-note').textContent = settings?.tor_enabled ? 'Encrypted relay through Tor. Direct connections disabled.' : settings?.relay_only ? 'Encrypted Iroh relay only. Direct connections disabled.' : state.relay_enabled ? 'Direct connections preferred. Encrypted relay when needed.' : 'Direct connections only. Some networks may block connections.';
  const peers = [...state.peers].sort((a,b) => a.id.localeCompare(b.id));
  const key = JSON.stringify([selectedRoom,isActiveRoom(),state.name,state.avatar_hash,state.voice,state.speaking,members,friends]);
  if (key !== membersKey) { membersKey=key;const list=selectedRoom?[{id:state.id,name:`${state.name} (you)`,tor_enabled:!!settings?.tor_enabled,avatar_hash:state.avatar_hash,voice:state.voice,speaking:state.speaking,online:isActiveRoom(),route:'self'},...[...members].sort((a,b)=>Number(b.online!==false)-Number(a.online!==false)||a.name.localeCompare(b.name))]:[];syncKeyed($('members'),list,p=>p.id,createMember,updateMember);if(!list.length)emptyList($('members'),'No participants.');window.OchatProfiles?.groups(); }
  const privateRoom = room?.capacity === 2;
  const otherVoice=activeVoice()?.room?.id!==selectedRoom ? activeVoice() : null;
  $('voice-heading').textContent = privateRoom ? 'Private call' : 'Group voice';
  $('voice-description').textContent = state.voice ? (privateRoom ? 'Your private call is connected.' : 'You are connected to this voice room.') : privateRoom ? 'The other participant chooses whether to accept.' : 'Everyone joins voluntarily. Your microphone stays off until you join.';
  $('voice-button').textContent = privateRoom ? 'Call' : state.voice ? 'Voice connected' : 'Join voice';
  $('voice-button').disabled = state.permissions?.calls===false || !isActiveRoom() || state.voice || !!state.call || !!otherVoice || (privateRoom && !state.peers.length) || !!$('voice-button').dataset.busy;
  $('voice-button').title=otherVoice?`Leave voice in ${conversationContext(otherVoice.room.id).title} first`:!isActiveRoom()?'Reconnect to use voice':privateRoom && !state.peers.length?'The other participant must be online':privateRoom?'Start a private call':'Join group voice';
  $('voice-button').hidden = isActiveRoom() && state.voice;
  const typing = isActiveRoom() ? peers.filter(p => p.typing).map(p => p.name) : [];
  const label = typing.length === 1 ? `${typing[0]} is typing…` : typing.length === 2 ? `${typing[0]} and ${typing[1]} are typing…` : typing.length ? `${typing.length} people are typing…` : '';
  if ($('typing-status').textContent !== label) $('typing-status').textContent = label;
  window.OchatWorkspace?.render();
  renderComposer(); renderRooms(); renderCall(); renderVoiceRoster(); renderDock(); renderPeerCard();window.OchatFeatures?.sync();window.OchatGifPicker?.sync();syncNavigationLayout();window.OchatDiscord?.sync(conversationContext(selectedRoom),unlocked,Array.isArray(state?.channels));window.OchatChannels?.sync(channelContextFor(selectedRoom),unlocked);window.OchatPresence?.sync();
  if(window.innerWidth>=1180 && selectedRoom && !introducedProfiles.has(selectedRoom) && !hasBlockingDialog()) {
    introducedProfiles.add(selectedRoom);
    if(!$('details-dialog').open){const focused=document.activeElement;openDetails();focused?.focus({preventScroll:true});}
  }
}
function speakingIcon() { const icon = el('span','speaking-icon'); icon.setAttribute('role','img'); icon.setAttribute('aria-label','Speaking'); icon.title = 'Speaking'; icon.classList.add('speaking-dot'); return icon; }
function elapsedLabel(started) {
  const elapsed=Math.max(0,Math.floor((Date.now()-started)/1000)),seconds=String(elapsed%60).padStart(2,'0'),minutes=String(Math.floor(elapsed/60)%60).padStart(2,'0'),hours=Math.floor(elapsed/3600);return hours?`${hours}:${minutes}:${seconds}`:`${minutes}:${seconds}`;
}
function updateVoiceTimer() {window.OchatChannels?.updateTimers();
  const active=activeVoice(),started=unlocked && active?.voice_started_at;
  $('voice-elapsed').hidden=!started;$('voice-elapsed').textContent=started?elapsedLabel(started):'00:00';
  const selected=unlocked && isActiveRoom() && state.voice_started_at;
  $('room-voice-elapsed').hidden=!selected;$('room-voice-elapsed').textContent=selected?elapsedLabel(selected):'00:00';
}
function renderVoiceRoster() {
  const peers=isActiveRoom()?state.peers.filter(p=>p.voice):[];
  if(isActiveRoom() && state.voice)peers.unshift({id:state.id,name:`${state.name} (you)`,speaking:state.speaking});
  syncVoiceList($('room-voice-roster'),peers);updateVoiceTimer();
}
setInterval(updateVoiceTimer,1000);

function renderComposer() {
  const connected = !!selectedRoom && !(Array.isArray(state.channels) && state.room?.capacity!==2 && !state.text_channel);
  const context=conversationContext(selectedRoom);
  $('message').disabled = !unlocked; $('message').placeholder = connected ? `Message ${context.title}…` : context.room ? `Draft for ${context.title}…` : 'Write a draft…';
  $('message').setAttribute('aria-label',context.room?`Message ${context.title}`:'Message');
  $('send-button').disabled = !connected || !$('message').value.trim() || !!$('send-button').dataset.busy;
  $('send-button').title = !selectedRoom ? 'Choose a friend or a room first' : !isActiveRoom() ? 'Queue this message until peers reconnect' : !state.peers.length ? 'Queue this message until a participant reconnects' : `Send to ${context.title}`;
  const status = !selectedRoom ? 'Local draft · choose a friend or open a room to send.' : !isActiveRoom() || !state.peers.length ? 'Encrypted outbox · delivered when you and the recipient are online.' : !connected ? 'Choose a text channel to send.' : context.direct ? `To ${context.title} · Private chat` : `To everyone in ${context.title}`;
  if ($('composer-status').textContent !== status) $('composer-status').textContent = status;
}
function normalizeLayout(value) {
  const order=[...new Set(Array.isArray(value?.order)?value.order:[])].filter(id=>typeof id==='string' && !deletedRooms.has(id));
  return {order,pinned:[...new Set(Array.isArray(value?.pinned)?value.pinned:[])].filter(id=>order.includes(id))};
}
function saveLayout(next) {
  conversationLayout=normalizeLayout(next);const desired=normalizeLayout(next),revision=++layoutRevision,generation=sessionGeneration;
  renderRooms();
  layoutQueue=layoutQueue.catch(()=>{}).then(async()=>{
    if(!unlocked || generation!==sessionGeneration)return;
    try {
      const result=await invoke('save_conversation_layout',{layout:desired});
      if(!unlocked || generation!==sessionGeneration)return;
      savedLayout=normalizeLayout(result);
      if(revision===layoutRevision){conversationLayout=normalizeLayout(savedLayout);renderRooms();}
    } catch(error) {
      if(!unlocked || generation!==sessionGeneration)return;
      if(revision===layoutRevision){conversationLayout=normalizeLayout(savedLayout);renderRooms();}
      toast(`Could not save conversation order: ${error}`);
    }
  });
  return layoutQueue;
}
function pinConversation(id) {
  const order=allRooms().map(room=>room.id),pinned=new Set(conversationLayout.pinned);
  if(pinned.has(id))pinned.delete(id);else pinned.add(id);
  return saveLayout({order,pinned:[...pinned]});
}
function moveConversation(id,target,after=false) {
  if(id===target || conversationLayout.pinned.includes(id)!==conversationLayout.pinned.includes(target) || (allRooms().find(r=>r.id===id)?.capacity===2)!==(allRooms().find(r=>r.id===target)?.capacity===2))return;
  const order=allRooms().map(room=>room.id).filter(room=>room!==id),index=order.indexOf(target);
  if(index<0 || deletedRooms.has(id))return;
  order.splice(index+Number(after),0,id);return saveLayout({order,pinned:conversationLayout.pinned});
}
let pointerTabDrag=null,suppressTabClickUntil=0;
function endTabDrag() {
  pointerTabDrag=null;
  tabDrag=null;cancelAnimationFrame(tabScrollFrame);tabScrollFrame=null;
  $('conversation-tabs').querySelectorAll('.dragging,[data-drop]').forEach(node=>{node.classList.remove('dragging');delete node.dataset.drop;});
}
function scrollDraggedTabs() {
  if(!tabDrag)return;
  const strip=$('conversation-tabs'),rect=strip.getBoundingClientRect(),vertical=strip.getAttribute('aria-orientation')==='vertical',position=vertical?tabDrag.y:tabDrag.x,start=vertical?rect.top:rect.left,end=vertical?rect.bottom:rect.right;
  if(position>=start-20 && position<=end+20){const delta=position<start+40?-9:position>end-40?9:0;if(vertical)strip.scrollTop+=delta;else strip.scrollLeft+=delta;}
  tabScrollFrame=requestAnimationFrame(scrollDraggedTabs);
}
function wireTabDrag(entry) {
  // Native external file drops on Windows disable HTML5 drag/drop. Keep
  // internal tab ordering on pointer events in that environment.
  entry._tab.draggable=!tauri?.window?.getCurrentWindow;
  entry._tab.onpointerdown=event=>{
    if(event.button!==0 || !unlocked)return;
    pointerTabDrag={entry,id:entry.dataset.key,x:event.clientX,y:event.clientY,pointer:event.pointerId,moving:false};
  };
  entry._tab.addEventListener('click',event=>{if(Date.now()<suppressTabClickUntil){event.preventDefault();event.stopImmediatePropagation();}},true);
  entry._tab.ondragstart=event=>{
    endTabDrag();tabDrag={id:entry.dataset.key,x:event.clientX,y:event.clientY};entry.classList.add('dragging');
    event.dataTransfer.effectAllowed='move';event.dataTransfer.setData('text/plain',entry.dataset.key);
    scrollDraggedTabs();
  };
  entry._tab.ondragend=endTabDrag;
  entry.ondragover=event=>{
    if(!tabDrag)return;tabDrag.x=event.clientX;tabDrag.y=event.clientY;
    const same=conversationLayout.pinned.includes(tabDrag.id)===conversationLayout.pinned.includes(entry.dataset.key) && (allRooms().find(r=>r.id===tabDrag.id)?.capacity===2)===(allRooms().find(r=>r.id===entry.dataset.key)?.capacity===2);
    $('conversation-tabs').querySelectorAll('[data-drop]').forEach(node=>delete node.dataset.drop);
    event.dataTransfer.dropEffect=same?'move':'none';if(!same)return;
    event.preventDefault();const box=entry.getBoundingClientRect();entry.dataset.drop=($('conversation-tabs').getAttribute('aria-orientation')==='vertical'?event.clientY<box.top+box.height/2:event.clientX<box.left+box.width/2)?'before':'after';
  };
  entry.ondrop=event=>{event.preventDefault();const id=tabDrag?.id,after=entry.dataset.drop==='after',valid=!!entry.dataset.drop;endTabDrag();if(id && valid)moveConversation(id,entry.dataset.key,after);};
}
document.addEventListener('pointermove',event=>{
  const drag=pointerTabDrag;if(!drag || drag.pointer!==event.pointerId || !unlocked)return;
  if(!drag.moving && Math.hypot(event.clientX-drag.x,event.clientY-drag.y)<8)return;
  if(!drag.moving){drag.moving=true;tabDrag={id:drag.id,x:event.clientX,y:event.clientY};drag.entry.classList.add('dragging');drag.entry._tab.setPointerCapture?.(event.pointerId);scrollDraggedTabs();}
  event.preventDefault();tabDrag.x=event.clientX;tabDrag.y=event.clientY;
  $('conversation-tabs').querySelectorAll('[data-drop]').forEach(node=>delete node.dataset.drop);
  const target=document.elementFromPoint(event.clientX,event.clientY)?.closest('.conversation-tab-entry');
  if(!target || target===drag.entry || conversationLayout.pinned.includes(drag.id)!==conversationLayout.pinned.includes(target.dataset.key) || (allRooms().find(r=>r.id===drag.id)?.capacity===2)!==(allRooms().find(r=>r.id===target.dataset.key)?.capacity===2))return;
  const box=target.getBoundingClientRect();target.dataset.drop=event.clientY<box.top+box.height/2?'before':'after';
},{passive:false});
document.addEventListener('pointerup',()=>{
  const drag=pointerTabDrag;if(!drag)return;
  const target=$('conversation-tabs').querySelector('[data-drop]'),after=target?.dataset.drop==='after';
  const id=drag.id,moving=drag.moving;endTabDrag();
  if(moving){suppressTabClickUntil=Date.now()+300;if(target)moveConversation(id,target.dataset.key,after);}
});
document.addEventListener('pointercancel',()=>{if(pointerTabDrag)endTabDrag();});
function conversationIsOpen(room) {
  return unlocked && document.body.dataset.homeOpen!=='true' && document.body.dataset.voiceView!=='true' && selectedRoom===room && !historyLoading && !hasBlockingDialog() && document.visibilityState!=='hidden' && document.hasFocus();
}
function acknowledgeConversation() {
  if(conversationIsOpen(selectedRoom) && attentionRooms.delete(selectedRoom))renderRooms();
}
function rememberMessage(event) {
  const key=messageKey(event.room,event);
  if(seenLiveMessages.has(key))return false;
  seenLiveMessages.add(key);if(seenLiveMessages.size>10000)seenLiveMessages.delete(seenLiveMessages.values().next().value);
  return true;
}
function allRooms() {
  const all=[...rooms];for(const snapshot of [state,...roomSnapshots.values()])if(snapshot?.room && !all.some(r=>r.id===snapshot.room.id))all.push(snapshot.room);
  const rank=new Map(conversationLayout.order.map((id,index)=>[id,index])),pins=new Set(conversationLayout.pinned);
  return all.filter(room=>!deletedRooms.has(room.id)).sort((a,b)=>Number(b.capacity===2)-Number(a.capacity===2)||Number(pins.has(b.id))-Number(pins.has(a.id))||(rank.get(a.id)??Infinity)-(rank.get(b.id)??Infinity)||a.name.localeCompare(b.name)||a.id.localeCompare(b.id));
}
function renderRooms() {
  window.OchatPersonalization?.badge();
  const focused=document.activeElement,restoreFocus=$('conversation-tabs').contains(focused);
  const all=allRooms();
  const contexts=all.map(room=>conversationContext(room.id));
  const update=(node,room,tab)=>{
    const button=tab?node._tab:node;
    if(tab){node.classList.toggle('selected',room.id===selectedRoom);node._remove.title='Delete conversation from this device';node._remove.setAttribute('aria-label',`Delete ${conversationContext(room.id).title} from this device`);node._remove.onclick=()=>requestForgetRoom(room.id);}
    const snapshot=(state?.room?.id===room.id?state:roomSnapshots.get(room.id)),count=unread.get(room.id)||0;
    button.classList.toggle('selected',room.id===selectedRoom);const context=conversationContext(room.id);profileAvatar(button._avatar,context.title,context.direct?context.peer?.avatar_hash:context.room?.image_hash,context.peer?.id || '');button._avatar.classList.toggle('group-avatar',!context.direct);button._name.textContent=context.title;button._voice.hidden=!(snapshot?.voice || snapshot?.peers?.some(peer=>peer.voice));button._voice.title='Voice chat in progress';button._badge.hidden=!count;button._badge.textContent=count>99?'99+':String(count);button._badge.setAttribute('aria-label',`${count} unread messages`);button.classList.toggle('has-unread',count>0);if(!tab)button.setAttribute('aria-label',`${context.title}${count?` · ${count} unread`:''}`);button.title=context.direct?`Private chat with ${context.title} · ${room.name}`:`Group · ${room.name}`;
    button._meta.textContent=`${room.capacity===2?'Private chat':'Group'} · ${snapshot?`${snapshot.peers.length+1} online${snapshot.voice?' · In voice':snapshot.call?' · Call':''}`:'Offline'}`;
    if(context.direct && contexts.filter(other=>other.title===context.title).length>1)button._meta.textContent+=` · ${context.peer?.id.slice(0,8) || room.id.slice(0,8)} · ${room.name}`;
    if(tab){
      const pinned=conversationLayout.pinned.includes(room.id),attention=attentionRooms.has(room.id);
      node.classList.toggle('pinned',pinned);node.classList.toggle('needs-attention',attention);
      node._pin.setAttribute('aria-pressed',String(pinned));node._pin.setAttribute('aria-label',`${pinned?'Unpin':'Pin'} ${context.title}`);node._pin.title=`${pinned?'Unpin conversation':'Pin conversation first'} · Drag tabs to reorder within their section`;
      node._pin.onclick=()=>pinConversation(room.id);
      button.setAttribute('aria-label',`${context.title}${pinned?' · Pinned':''}${count?` · ${count} unread`:''}${attention?' · New messages, not yet opened':''}`);
      button.setAttribute('aria-keyshortcuts','Alt+ArrowUp Alt+ArrowDown Alt+ArrowLeft Alt+ArrowRight');button.setAttribute('aria-describedby','tab-order-hint');
      button.setAttribute('aria-selected',String(room.id===selectedRoom));button.tabIndex=room.id===selectedRoom?0:-1;button.id=`conversation-tab-${room.id}`;node.classList.toggle('direct-conversation',context.direct);node.classList.toggle('room-conversation',!context.direct);
    }
    button.onclick=()=>action(button,async()=>{await selectRoom(room.id);closePanel('navigation-dialog');});
  };
  const create=tab=>()=>{const button=el('button',tab?'conversation-tab':'room-item');button.type='button';button._name=el('strong','');button._meta=el('small','');button._avatar=el('span','avatar tab-avatar');button._avatar.setAttribute('aria-hidden','true');const text=el('span','tab-copy');text.append(button._name,button._meta);button._badge=el('span','unread-badge');button._badge.hidden=true;button._voice=el('span','tab-voice-icon','🔊');button._voice.setAttribute('aria-label','Voice chat in progress');button._voice.hidden=true;button.append(button._avatar,text,button._badge,button._voice);if(tab){button.setAttribute('role','tab');button.setAttribute('aria-controls','conversation-panel');const entry=el('div','conversation-tab-entry');entry._tab=button;entry._pin=el('button','tab-pin');entry._pin.type='button';entry._pin.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 3h8m-7 0v6l-3 4v2h12v-2l-3-4V3M12 15v6"/></svg>';entry._remove=el('button','tab-delete','×');entry._remove.type='button';entry.append(button,entry._pin,entry._remove);wireTabDrag(entry);return entry;}return button;};
  const direct=all.filter(room=>room.capacity===2),groups=all.filter(room=>room.capacity!==2);
  $('direct-count').textContent=String(direct.length);$('group-count').textContent=String(groups.length);
  for(const [id,items,empty] of [['direct-room-list',direct,'No private conversations yet.'],['group-room-list',groups,'Create or join a room.']]){syncKeyed($(id),items,r=>r.id,create(false),(node,room)=>update(node,room,false));if(!items.length)emptyList($(id),empty);}
  for(const [id,items] of [['private-conversation-tabs',direct],['room-conversation-tabs',groups]]){syncKeyed($(id),items,r=>r.id,create(true),(node,room)=>update(node,room,true));if(!items.length)emptyList($(id),id.startsWith('private')?'No chats':'No rooms');} $('private-tab-count').textContent=direct.length;$('room-tab-count').textContent=groups.length;
  if(restoreFocus && focused.isConnected && document.activeElement!==focused)focused.focus({preventScroll:true});

  if(selectedRoom){$('conversation-panel').setAttribute('aria-labelledby',`conversation-tab-${selectedRoom}`);if(lastTabSelection!==selectedRoom){lastTabSelection=selectedRoom;$(`conversation-tab-${selectedRoom}`)?.scrollIntoView?.({block:'nearest',inline:'nearest'});}}
}

async function hydrateRoomMembers() {
  const generation=sessionGeneration,ids=allRooms().map(room=>room.id).filter(id=>!roomMembers.has(id) && !pendingRoomMembers.has(id));
  for(const id of ids)pendingRoomMembers.add(id);
  let index=0;
  await Promise.all(Array.from({length:Math.min(4,ids.length)},async()=>{
    while(index<ids.length){
      const id=ids[index++];if(!unlocked || generation!==sessionGeneration)return;
      try {
        const members=await invoke('room_participants',{room:id});
        if(unlocked && generation===sessionGeneration && !deletedRooms.has(id) && allRooms().some(room=>room.id===id) && !snapshotFor(id)){
          roomMembers.set(id,Array.isArray(members)?members:[]);render();
        }
      }catch{}finally{if(generation===sessionGeneration)pendingRoomMembers.delete(id);}
    }
  }));
}
async function refreshRooms() { const generation = sessionGeneration; const result = await invoke('list_rooms'); if (!unlocked || generation !== sessionGeneration) return; rooms = result.filter(room => !deletedRooms.has(room.id)); roomsKey = ''; render();hydrateRoomMembers(); }
async function selectRoom(id) { closeMessageMenu(false);setNavigationSection('rooms');releaseTalk(); const generation = sessionGeneration; stopTyping(); drafts.set(draftKey(),$('message').value); selectedRoom = id; historyLoading=true; reportView(); clearTimeout(readTimer); readTimer=null; closeMentions(); $('message').value = '';  const next = await invoke('select_room',{id}); if (!unlocked || generation !== sessionGeneration || selectedRoom !== id) return; state = next; $('message').value=drafts.get(draftKey()) || ''; membersKey = ''; render(); await loadRoom(id); }
async function loadRoom(id, target = null) {
  closeMessageMenu(false);
  $('history-context').hidden=true;
  historyLoading=true;window.OchatTimeline?.reset();
  previewObserver?.disconnect();const generation = ++historyGeneration; loadedRoom = id; shownMessages.clear(); $('timeline').replaceChildren();
  if (!id) { $('timeline').append(welcomeTemplate.cloneNode(true)); bindWelcome(); emojiEntries = []; historyLoading=false; reportView(); return; }
  const oldScope=draftKey(id);
  try{const view=await invoke('channel_view',{room:id});if(!unlocked || generation!==historyGeneration || selectedRoom!==id)return;if(Array.isArray(view?.channels)){archivedChannelViews.set(id,view.channels);window.OchatChannels?.setUnread(view.unread || {},view.channels);if(view.selected)archivedTextChannels.set(id,view.selected);else archivedTextChannels.delete(id);if(oldScope!==draftKey(id) && !snapshotFor(id)){$('message').value=drafts.get(draftKey(id)) || '';}}}catch{}
  const [history,entries,members,marker] = await Promise.all([invoke('history',{room:id,channel:selectedChannel(id)}),invoke('room_emojis',{room:id}),invoke('room_participants',{room:id}),invoke('read_marker',{room:id,channel:selectedChannel(id)})]);
  if (!unlocked || generation !== historyGeneration || selectedRoom !== id) return;
  emojiEntries = entries;
  readMarkers.set(draftKey(id),marker && typeof marker.through==='number'?marker:{room:id,through:0,ids:[]});
  roomMembers.set(id,members); membersKey = ''; render();
  if (!history.length) {const context=conversationContext(id);$('timeline').append(el('div','system-message',!isActiveRoom()?'No saved messages in this conversation.':state.peers.length?`Start your conversation with ${context.direct?context.title:'everyone in '+context.title}.`:'Invite someone to start the conversation.'));}
  for (const entry of history) if(entry.event.type==='message'){messageCache.set(messageKey(id,entry.event),entry.event);rememberMessage({...entry.event,room:id});}
  const targetIndex=target?history.findIndex(e=>(e.event.id===target.id || e.event.offer?.id===target.id) && e.event.peer===target.peer):-1;
  $('history-context').hidden=targetIndex<0;
  const displayed=targetIndex>=0?history.slice(Math.max(0,targetIndex-150),targetIndex+151):history.slice(-450);
  for (const entry of displayed) { const event = {...entry.event,timestamp:entry.event.timestamp || entry.timestamp}; if (event.type === 'message') addMessage(event); if (event.type === 'member_joined') addJoinNotice(event); if (event.type === 'file_offered') addFile(event, false); }
  for (const entry of history) if (entry.event.type === 'file_decision') fileDecision(entry.event);
  $('timeline').scrollTop = $('timeline').scrollHeight; renderEmojiLibrary();
  historyLoading=false;if(target && displayed.some(e=>e.event.type==='message'&&e.event.id===target.id))focusMessage(target);reportView();if(!target)scheduleRead();
  refreshMessageTools(id).catch(toast);window.OchatPresentation?.conversationChanged(id);
}
function appendEntry(entry) {
  const timeline=$('timeline'),bottom=timeline.scrollHeight-timeline.scrollTop-timeline.clientHeight<120;
  const previous=timeline.lastElementChild;
  if(entry.classList.contains('message')){
    const date=new Date(Number(entry.dataset.timestamp)),last=[...timeline.children].reverse().find(node=>node.classList.contains('message'));
    if(!last || new Date(Number(last.dataset.timestamp)).toDateString()!==date.toDateString()){
      const divider=el('div','day-divider',date.toLocaleDateString('en-US',{weekday:'long',month:'short',day:'numeric',year:'numeric'}));
      divider.setAttribute('role','separator');if(previous?.classList.contains('unread-divider'))timeline.insertBefore(divider,previous);else timeline.append(divider);
    }
    const gap=Number(entry.dataset.timestamp)-Number(previous?.dataset.timestamp);
    entry.classList.toggle('continuation',entry.classList.contains('studio-text') && previous?.classList.contains('studio-text') && previous.dataset.peer===entry.dataset.peer && previous.dataset.room===entry.dataset.room && entry.dataset.hasReply!=='true' && gap>=0 && gap<=120000 && new Date(Number(previous.dataset.timestamp)).toDateString()===date.toDateString());
  }
  timeline.append(entry);window.OchatTimeline?.appended(entry,bottom);
  while(timeline.children.length>500)timeline.firstElementChild.remove();
  const first=timeline.querySelector('.message');if(first)first.classList.remove('continuation');
  if(bottom)timeline.scrollTop=timeline.scrollHeight;
}
function messageFrame(event) {
  const own=event.peer===state?.id,room=event.room || selectedRoom,person=peerDetails(event.peer,room,event.name);
  const row=el('article',`message${own?' own':''}`),avatar=el('button','avatar');avatar.type='button';
  profileAvatar(avatar,person.name,person.avatar_hash,event.peer);avatar.setAttribute('aria-label',`View ${person.name}'s profile`);avatar.onclick=()=>showPeer(event.peer,event.name,room,avatar);row.append(avatar);
  row.dataset.room=room || '';row.dataset.peer=event.peer;row.dataset.timestamp=event.timestamp || Date.now();row.setAttribute('aria-label',`Message from ${own?'you':event.name}, ${new Date(Number(row.dataset.timestamp)).toLocaleString('en-US')}`);
  const body=el('div','message-body'),header=el('div','message-header'),author=el('button','message-author',own?'You':event.name);
  author.type='button';author.setAttribute('aria-label',`View ${own?'your':event.name+"'s"} profile`);author.onclick=()=>showPeer(event.peer,event.name,room,author);
  const time=el('time','',new Date(event.timestamp || Date.now()).toLocaleTimeString('en-US',{hour:'2-digit',minute:'2-digit',hour12:false}));time.title=new Date(event.timestamp || Date.now()).toLocaleString('en-US');
  header.append(author,time);body.append(header);row.append(body);return {row,body};
}
function customImage(reference, container) {
  const generation = sessionGeneration;
  const assetEpoch = assetGeneration;
  const fallback = el('span','emoji-fallback',`:${reference.name}:`); fallback.dataset.emojiHash = reference.hash; fallback.dataset.emojiName = reference.name; container.append(fallback);
  const ready = data => { if (!data || !unlocked || generation !== sessionGeneration || assetEpoch !== assetGeneration || !fallback.parentNode) return; const image = el('img','custom-emoji'); image.src = data; image.alt = `:${reference.name}:`; image.title = image.alt; image.width = 24; image.height = 24; fallback.replaceChildren(image); };
  if (assets.has(reference.hash)) ready(assets.get(reference.hash)); else {
    if (!assetLoads.has(reference.hash)) assetLoads.set(reference.hash,invoke('emoji_asset',{hash:reference.hash}).then(data => { if (data && unlocked && generation === sessionGeneration && assetEpoch === assetGeneration) assets.set(reference.hash,data); return data; }).catch(() => null).finally(() => assetLoads.delete(reference.hash)));
    assetLoads.get(reference.hash).then(ready);
  }
}
function messageLinks(text) {
  const links=[];
  for(const match of text.matchAll(/https?:\/\/[^\s<>"\u0000-\u001f]+/gi)) {
    let value=match[0].replace(/[.,!?;:'\u2019\u201d]+$/g,'');
    for(const [open,close] of [['(',')'],['[',']'],['{','}']])while(value.endsWith(close) && value.split(close).length>value.split(open).length)value=value.slice(0,-1);
    try {const url=new URL(value);if(['https:','http:'].includes(url.protocol) && url.hostname && !url.username && !url.password && value.length<=8192)links.push({value,url:url.href,index:match.index});}catch{}
  }
  return links;
}
function linkAnchor(url,label,className='message-link') {
  const anchor=el('a',className,label);anchor.href=url;anchor.target='_blank';anchor.rel='noopener noreferrer';anchor.title=`Open in browser · ${new URL(url).hostname}`;
  const open=event=>{event.preventDefault();openExternalLink(url).catch(error=>toast(error || 'Could not open the link in your browser.'));};anchor.onclick=open;anchor.onauxclick=event=>{if(event.button===1)open(event);};anchor.oncontextmenu=event=>{if(settings?.tor_enabled)open(event);};return anchor;
}
function appendRichText(container,text,references,mentions) {
  const mapping=new Map(references.map(r=>[r.name,r])),pattern=/:([a-z0-9_]{2,32}):/g;let start=0;
  for(const match of text.matchAll(pattern)){appendMentionText(container,text.slice(start,match.index),mentions);const reference=mapping.get(match[1]);if(reference)customImage(reference,container);else container.append(document.createTextNode(match[0]));start=match.index+match[0].length;}
  appendMentionText(container,text.slice(start),mentions);
}
function appendTextWithEmojis(container,text,references=[],mentions=[]) {
  let start=0;
  for(const link of messageLinks(text)){appendRichText(container,text.slice(start,link.index),references,mentions);container.append(linkAnchor(link.url,link.value));start=link.index+link.value.length;}
  appendRichText(container,text.slice(start),references,mentions);
}
function appendLinkPreviews(body,text) {
  const urls=[...new Set(messageLinks(text).map(link=>link.url))].slice(0,2);
  for(const url of urls){
    const card=el('aside','link-preview'),host=el('small','link-site',new URL(url).hostname),title=linkAnchor(url,new URL(url).hostname,'link-title'),description=el('p','link-description'),button=el('button','text-button','Load preview');
    description.textContent='Load a preview from this website.';button.type='button';button.title='Contacts the website to load its title and image';card.append(host,title,description,button);body.append(card);
    let loading=false;
    card._load=async()=>{
      if(loading || !unlocked || !card.isConnected)return;loading=true;button.disabled=true;button.textContent='Loading…';
      const generation=sessionGeneration,epoch=previewEpoch;
      try {
        if(!previewCache.has(url)){const request=invoke('link_preview',{url}).catch(error=>{if(previewCache.get(url)===request)previewCache.delete(url);throw error;});previewCache.set(url,request);if(previewCache.size>64)previewCache.delete(previewCache.keys().next().value);}
        const result=await previewCache.get(url);
        if(!unlocked || generation!==sessionGeneration || epoch!==previewEpoch || !card.isConnected)return;
        const bottom=atBottom();title.textContent=result.title || new URL(url).hostname;description.textContent=result.description || '';description.hidden=!result.description;
        if(typeof result.image==='string' && /^data:image\/(jpeg|png|gif|webp);base64,/.test(result.image)){
          const anchor=linkAnchor(url,'','link-image'),image=el('img','');image.src=result.image;image.alt='Website preview';image.width=640;image.height=360;anchor.append(image);card.append(anchor);
        }
        button.remove();card._load=null;if(bottom)$('timeline').scrollTop=$('timeline').scrollHeight;
      }catch{
        if(!unlocked || generation!==sessionGeneration || epoch!==previewEpoch || !card.isConnected)return;
        description.textContent='Preview unavailable. You can still open the link.';button.textContent='Retry preview';button.disabled=false;loading=false;
      }
    };
    card._reset=()=>{if(!card._load)return;loading=false;button.disabled=false;button.textContent='Load preview';};button.onclick=()=>card._load?.();
    if(settings?.link_previews!==false){if(previewObserver)previewObserver.observe(card);else queueMicrotask(()=>card._load?.());}
  }
}
function addMessage(event,animate=false) { const key = `${event.peer}:${event.id}`; if (shownMessages.has(key)) return false; shownMessages.add(key); const {row,body} = messageFrame(event),bubble = el('p','bubble'); messageCache.set(messageKey(event.room || selectedRoom,event),event);row.classList.add('studio-text');row.dataset.hasReply=String(!!event.reply_to);row.dataset.peer=event.peer;row.dataset.messageId=event.id;row.dataset.timestamp=event.timestamp || Date.now();row.classList.toggle('unread',isUnread(event));row.classList.toggle('mentions-you',(event.mentions || []).some(mention=>mention.peer===state?.id));if(isUnread(event) && !$('timeline').querySelector('.unread-divider'))appendEntry(el('div','unread-divider','New messages'));if(event.reply_to && !event.deleted)body.append(replyQuote(event.room || selectedRoom,event.reply_to));if(event.deleted || !window.OchatPersonalization?.image(bubble,event))appendTextWithEmojis(bubble,event.deleted?'Message deleted':event.text,event.deleted?[]:event.emojis,event.deleted?[]:event.mentions); body.append(bubble);if(!event.deleted)appendLinkPreviews(body,event.text);const reply=el('button','text-button reply-action','Reply');reply.type='button';reply.setAttribute('aria-label',`Reply to ${event.name}`);reply.onclick=()=>beginReply(event);body.append(reply); enhanceMessage(row,event); appendEntry(row);window.OchatPresentation?.messageAdded(row,animate);scheduleRead();return true; }
function addJoinNotice(event) { const key = `presence:${event.peer}:${event.id}`; if (shownMessages.has(key)) return; shownMessages.add(key); const row = el('div','system-message member-join',`${event.peer === state?.id ? 'You' : event.name} joined the room.`); row.title = new Date(event.timestamp || Date.now()).toLocaleString('en-US'); appendEntry(row); }
function addFile(event,live) {
  const {row,body} = messageFrame(event),card = el('div','file-card'); row.dataset.fileId=event.offer.id;card.dataset.fileId = event.offer.id; card.dataset.peer = event.peer; card.dataset.outgoing = String(event.outgoing); card.append(el('strong','',event.offer.name),el('small','',`${size(event.offer.size)} · BLAKE3 verified on receipt`)); const status = el('div','file-progress',event.outgoing ? 'Offered to participants' : live ? 'Waiting for your approval' : 'Saved file offer · download may no longer be available'); card.append(status,el('div','file-recipients'));
  if (!event.outgoing && isActiveRoom()) { const actions = el('div','file-actions'),accept = el('button','primary','Save file…'),reject = el('button','secondary','Decline'); accept.onclick = () => action(accept,async () => { reject.disabled = true; try { if (await invoke('resume_transfer',{room:event.room,peer:event.peer,id:event.offer.id}) || await invoke('receive_file',{peer:event.peer,id:event.offer.id})) { actions.remove(); liveOffers.delete(`${event.peer}:${event.offer.id}`); } } finally { reject.disabled = false; } }); reject.onclick = () => action(reject,async () => { await invoke('cancel_transfer',{room:event.room,peer:event.peer,id:event.offer.id});await invoke('reject_file',{peer:event.peer,id:event.offer.id}); liveOffers.delete(`${event.peer}:${event.offer.id}`); actions.remove(); status.textContent = 'File declined'; }); const restart=toolButton('Restart download…',async()=>{await invoke('cancel_transfer',{room:event.room,peer:event.peer,id:event.offer.id});if(await invoke('receive_file',{peer:event.peer,id:event.offer.id})){actions.remove();liveOffers.delete(`${event.peer}:${event.offer.id}`);}},'secondary restart-download');restart.hidden=true;actions.append(accept,restart,reject);card.append(actions); }
  body.append(card); appendEntry(row);
}
function filePeerStatus(card,peer,name,status) {
  const list = card.querySelector('.file-recipients'); let row = [...list.children].find(row => row.dataset.peer === peer);
  if (!row) { row = el('div','file-recipient'); row.dataset.peer = peer; list.append(row); }
  row.textContent = `${name} · ${status}`;
}
function fileDecision(event) { document.querySelectorAll('.file-card').forEach(card => { if (card.dataset.fileId === event.id) filePeerStatus(card,event.peer,event.name,event.accepted ? 'Accepted' : 'Declined'); }); }
function transfer(event) { document.querySelectorAll('.file-card').forEach(card => { if (card.dataset.fileId !== event.id || (card.dataset.peer !== event.peer && card.dataset.peer !== state.id)) return; if(event.status==='paused' && card.dataset.outgoing!=='true'){const restart=card.querySelector('.restart-download');if(restart)restart.hidden=false;const resume=card.querySelector('.file-actions .primary');if(resume)resume.textContent='Resume download';} const labels = {sending:'Sending',receiving:'Receiving',received:'Received and verified',sent:'Received and verified',failed:'Transfer interrupted or verification failed',paused:'Paused · click Save file to resume'}, status = `${labels[event.status] || event.status} · ${size(event.bytes)} / ${size(event.total)}`; if (card.dataset.outgoing === 'true') { filePeerStatus(card,event.peer,event.name || event.peer,status); card.querySelector('.file-progress').textContent = 'Delivery by participant'; } else card.querySelector('.file-progress').textContent = `${event.name || event.peer} · ${status}`; if (event.status === 'received') card.querySelector('.file-actions')?.remove(); }); }

function stopTyping() { clearTimeout(typingTimer); clearInterval(typingPulse); typingPulse = null; if (unlocked && state?.room) invoke('set_typing',{active:false,room:selectedRoom,channel:selectedChannel()}).catch(() => {}); }
function onInput() { updateMentions(); $('message').style.height = 'auto'; $('message').style.height = `${Math.min($('message').scrollHeight,130)}px`; renderComposer(); clearTimeout(typingTimer); if (!isActiveRoom() || !$('message').value.trim()) { stopTyping(); return; } invoke('set_typing',{active:true,room:selectedRoom,channel:selectedChannel()}).catch(() => {}); if (!typingPulse) typingPulse = setInterval(() => { if (unlocked && isActiveRoom() && $('message').value.trim()) invoke('set_typing',{active:true,room:selectedRoom,channel:selectedChannel()}).catch(() => {}); },2000); typingTimer = setTimeout(stopTyping,2000); }
function renderCall() { const call = state.call; if (!call || call.status === 'active') { if ($('call-dialog').open) $('call-dialog').close(); return; } profileAvatar($('call-avatar'),call.name,(state.peers.find(p => p.id === call.peer))?.avatar_hash,call.peer); $('call-title').textContent = call.status === 'incoming' ? `${call.name} is calling` : call.status === 'outgoing' ? `Calling ${call.name}…` : 'Connecting…'; $('call-description').textContent = call.status === 'incoming' ? 'Your microphone stays off until you accept.' : 'Your microphone starts only after the call is accepted.'; $('accept-call').hidden = call.status !== 'incoming'; $('decline-call').hidden = call.status !== 'incoming'; $('cancel-call').hidden = call.status === 'incoming'; if (!$('call-dialog').open) show('call-dialog'); }
// Browser navigation leaves Ochat's Tor transport and requires an explicit choice.
let externalLinkChoice=null;
async function openExternalLink(url){
  if(!unlocked)return;
  if(!settings?.tor_enabled)return invoke('open_link',{url});
  externalLinkChoice={url,generation:sessionGeneration};
  $('external-link-url').textContent=url;show('external-link-dialog');
}
$('external-link-dialog').addEventListener('close',()=>{externalLinkChoice=null;$('external-link-url').textContent='';});
$('external-link-copy').onclick=()=>action($('external-link-copy'),async()=>{const choice=externalLinkChoice;if(!choice||!unlocked||choice.generation!==sessionGeneration)return;await navigator.clipboard.writeText(choice.url);if(unlocked&&choice.generation===sessionGeneration){$('external-link-dialog').close();toast('Link copied. Open it in a browser configured for Tor if needed.');}});
$('external-link-open').onclick=()=>action($('external-link-open'),async()=>{const choice=externalLinkChoice;if(!choice||!unlocked||choice.generation!==sessionGeneration)return;await invoke('open_link',{url:choice.url,torBrowserConfirmed:true});if(unlocked&&choice.generation===sessionGeneration)$('external-link-dialog').close();});

function readSettings() { if(!/^#[0-9a-fA-F]{6}$/.test($('accent-hex').value))throw new Error('Use an accent color in #RRGGBB format');return {...settings,...(window.OchatPersonalization?.read() || {}),...(window.OchatFeatures?.readShortcuts() || {}),read_receipts:$('read-receipts').checked,accent_color:$('accent-hex').value.toUpperCase(),desktop_notifications:$('desktop-notifications').checked,notification_previews:$('notification-previews').checked,notification_scope:$('notification-scope').value,theme:$('theme-choice').value,audio:{noise_profile:$('noise-profile').value,automatic_sensitivity:$('automatic-sensitivity').checked,voice_threshold:Number($('voice-threshold').value),push_to_talk:$('voice-mode').value==='ptt',talk_key:$('talk-key').value,input_device:$('input-device').value || null,output_device:$('output-device').value || null,input_volume:Number($('input-volume').value)/100,output_volume:Number($('output-volume').value)/100,noise_suppression:$('noise-profile').value!=='off',echo_cancellation:$('echo-cancellation').checked,automatic_gain:$('automatic-gain').checked},link_previews:$('link-previews').checked,message_sound:$('message-sound').checked,voice_join_sound:$('voice-join-sound').checked,voice_leave_sound:$('voice-leave-sound').checked,ringtone:$('ringtone').checked,sound_volume:Number($('sound-volume').value)/100}; }
function fillSettings() { $('noise-profile').value=settings.audio.noise_suppression===false?'off':settings.audio.noise_profile || 'balanced';$('automatic-sensitivity').checked=settings.audio.automatic_sensitivity!==false;$('voice-threshold').value=settings.audio.voice_threshold || 0.02;$('voice-threshold').disabled=settings.audio.automatic_sensitivity!==false;updateVoiceSensitivity();window.OchatFeatures?.fillShortcuts(settings);$('read-receipts').checked=!!settings.read_receipts;$('settings-save').disabled=false;$('accent-color').value=settings.accent_color || '#007AFF';$('accent-hex').value=(settings.accent_color || '#007AFF').toUpperCase();$('accent-error').textContent='';$('voice-mode').value=settings.audio.push_to_talk?'ptt':'open';$('talk-key').value=settings.audio.talk_key || 'Space'; $('desktop-notifications').checked = settings.desktop_notifications !== false; $('notification-previews').checked = !!settings.notification_previews; $('notification-scope').value = settings.notification_scope || 'mentions_and_direct'; profileAvatar($('profile-photo-preview'),state.name,state.avatar_hash,state.id);window.OchatProfiles?.settings(state); $('remove-profile-photo').hidden = !state.avatar_hash; $('theme-choice').value = settings.theme; $('input-volume').value = Math.round(settings.audio.input_volume*100); $('output-volume').value = Math.round(settings.audio.output_volume*100); $('noise-suppression').checked = settings.audio.noise_suppression; $('echo-cancellation').checked = settings.audio.echo_cancellation; $('automatic-gain').checked = settings.audio.automatic_gain; $('link-previews').checked=settings.link_previews!==false;$('message-sound').checked = settings.message_sound;$('voice-join-sound').checked=settings.voice_join_sound!==false;$('voice-leave-sound').checked=settings.voice_leave_sound!==false; $('ringtone').checked = settings.ringtone; $('sound-volume').value = Math.round(settings.sound_volume*100); $('identity-value').value = state.id; updateRangeLabels(); }
function updateRangeLabels() { for (const id of ['input-volume','output-volume','sound-volume']) $(`${id}-label`).textContent = `${$(id).value}%`; }
async function refreshDevices() { const generation = sessionGeneration, list = await invoke('audio_devices'); if (!unlocked || generation !== sessionGeneration || !settings) return; $('microphone-permission-hint').hidden=!/Macintosh|Mac OS X/.test(navigator.userAgent) || list.some(device=>device.input); for (const [id,input,selected] of [['input-device',true,settings.audio.input_device],['output-device',false,settings.audio.output_device]]) { const select = $(id); const value = select.value || selected || ''; select.replaceChildren(); const first = el('option','','System default'); first.value = ''; select.append(first); for (const device of list.filter(d => d.input === input)) { const option = el('option','',`${device.name}${device.is_default ? ' (default)' : ''}`); option.value = device.id; select.append(option); } if (value && !list.some(d => d.input === input && d.id === value)) { const option = el('option','','Unavailable device — choose another'); option.value = value; select.append(option); } select.value = value; } }
function stopAudioPoll() { clearInterval(audioPoll); audioPoll = null; if (unlocked) invoke('stop_microphone_test').catch(() => {}); }
async function openSettings() {
  const generation = sessionGeneration, next = await invoke('get_settings');
  if (!unlocked || generation !== sessionGeneration) return;
  settings = next; fillSettings(); show('settings-dialog');window.OchatPersonalization?.settingsOpened(); await refreshDevices();
  if (!unlocked || generation !== sessionGeneration || !$('settings-dialog').open) return;
  clearInterval(audioPoll); audioPoll = setInterval(async () => {
    if (!unlocked || !$('settings-dialog').open) return;
    try {
      const status = await invoke('audio_status');
      if (!unlocked || generation !== sessionGeneration || !$('settings-dialog').open) return;
      $('microphone-level').value = status.level;if($('microphone-detection'))$('microphone-detection').textContent=status.speaking?'Speech detected':'Waiting for speech'; $('stop-test').hidden = !status.testing;
      $('test-microphone').disabled = status.testing || state.voice || !!state.call;
      $('test-description').textContent = status.testing ? (status.phase===2?'Original microphone audio…':status.phase===3?'Processed microphone audio…':'Recording five seconds locally…') : 'Records five seconds locally, then plays Original followed by Processed audio. Nothing is sent or saved.';
    } catch {}
  },100);
}

function bindWelcome() { $('welcome-create')?.addEventListener('click',() => requestNewRoom()); $('welcome-join')?.addEventListener('click',() => requestJoin()); }
function requestNewRoom(direct = false) { creatingDirect = direct; $('create-room-title').textContent = direct ? 'Invite one person' : 'New group'; $('room-name-label').textContent = direct ? 'Invitation name' : 'Group name'; $('room-kind-description').textContent = direct ? 'Share the invitation with one person. To message an existing friend, use Message in Conversations → Friends.' : 'Invite people to a shared conversation. No participant limit is set.'; $('create-form').querySelector('[type="submit"]').textContent=direct?'Create invitation':'Create group'; show('create-dialog'); }
function requestJoin() { show('join-dialog'); }
function requestForgetRoom(room) {
  if(!room || !unlocked || deletedRooms.has(room))return;
  const name=conversationContext(room).title;
  confirm(`Delete ${name} from this device?`, 'This permanently removes this tab, its saved messages and shared emoji data from this device and leaves its voice session. Other participants keep their copies, and your friends remain. Downloaded files and existing backups are not deleted. To return later, explicitly join with an invitation or message your friend again. Deleted history will not return.', 'Delete from this device', async()=>{
    const generation=sessionGeneration;
    await invoke('forget_room',{room});
    if(!unlocked || generation!==sessionGeneration)return;
    if(!deletedRooms.has(room))await handleEvent({type:'room_forgotten',room});
  });
}
document.querySelectorAll('.close-dialog').forEach(button => button.addEventListener('click',() => button.closest('dialog').close()));
document.querySelectorAll('dialog').forEach(dialog=>dialog.addEventListener('close',()=>{if(dialog.open)return;const target=dialogReturnFocus.get(dialog.id);dialogReturnFocus.delete(dialog.id);if(target?.isConnected && !target.disabled && !hasBlockingDialog())target.focus({preventScroll:true});}));
$('confirm-action').onclick = () => action($('confirm-action'),async () => { const callback = confirmCallback; confirmCallback = null; $('confirm-dialog').close(); if (callback) await callback(); });
$('auth-dialog').addEventListener('cancel',event => event.preventDefault());
$('create-account-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { matchesPassword('create-password','create-password-confirm'); const generation = sessionGeneration; await completeAccount(await invoke('create_account',{name:$('account-name').value.trim(),password:$('create-password').value}), generation); }); };
$('unlock-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { const generation = sessionGeneration; await completeAccount(await invoke('unlock_account',{password:$('unlock-password').value}), generation); }); };
$('auth-import').onclick = () => { clearPasswords(); setAuthMode('import'); };
$('auth-back').onclick = () => { $('import-key').value = ''; clearPasswords(); setAuthMode(accountExists ? 'unlock' : 'create'); };
$('choose-backup').onclick = () => action($('choose-backup'),async () => { const name = await invoke('choose_backup'); if (name) $('backup-file-name').textContent = name; });
$('import-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { matchesPassword('import-password','import-password-confirm'); if (accountExists && !$('replace-account').checked) throw new Error('Confirm replacement of the existing local account.'); await invoke('import_backup',{recoveryKey:$('import-key').value.trim(),password:$('import-password').value,replace:$('replace-account').checked}); $('import-key').value = ''; clearPasswords(); accountExists = true; setAuthMode('unlock'); toast('Backup verified and imported. Unlock with your new local password.'); }); };
$('recovery-confirm').onchange = () => { $('recovery-done').disabled = !$('recovery-confirm').checked; };
$('recovery-done').onclick = () => { $('recovery-value').value = ''; recoveryRequired = false; $('recovery-dialog').close(); };
$('recovery-dialog').addEventListener('cancel',event => { if (recoveryRequired) event.preventDefault(); else $('recovery-value').value = ''; });
$('recovery-dialog').addEventListener('close',() => { $('recovery-value').value = ''; });
$('copy-recovery').onclick = () => action($('copy-recovery'),async () => { await invoke('copy_text',{text:$('recovery-value').value}); toast('Recovery key copied. Ochat clears it from the clipboard after 60 seconds if unchanged.'); });
$('new-room').onclick = () => requestNewRoom(); $('new-direct-chat').onclick = () => requestNewRoom(true); $('join-button').onclick = requestJoin; bindWelcome();
$('create-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { const invitation = await invoke('create_room',{name:$('room-name').value.trim(),direct:creatingDirect}); state = await invoke('snapshot'); await refreshRooms(); selectedRoom = state.room.id; await loadRoom(selectedRoom); render(); $('create-dialog').close(); $('invitation-output').value = invitation; show('invite-dialog'); }); };
$('join-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { await invoke('join_room',{invitation:$('invitation-input').value.trim()}); $('invitation-input').value = ''; state = await invoke('snapshot'); deletedRooms.delete(state.room.id); await refreshRooms(); selectedRoom = state.room.id; await loadRoom(selectedRoom); render(); $('join-dialog').close(); $('message').focus(); }); };
$('invite-button').onclick = () => action($('invite-button'),async () => { $('invitation-output').value = await invoke('get_invitation'); show('invite-dialog'); });
$('copy-invite').onclick = () => action($('copy-invite'),async () => { await invoke('copy_text',{text:$('invitation-output').value}); toast('Invitation copied. Share it only with people you trust.'); });
$('leave-button').onclick = () => confirm('Leave this room?','Saved messages and emojis stay encrypted on this device. An active call will end.','Leave room',async () => { stopTyping(); await invoke('leave_room'); state = await invoke('snapshot'); render(); });
$('reconnect-button').onclick = () => action($('reconnect-button'),async () => { const id = selectedRoom; await invoke('reopen_room',{id}); state = await invoke('snapshot'); selectedRoom = id; render(); });
$('clear-history').onclick = () => { const room = selectedRoom; confirm('Delete this room’s history?','This removes saved messages from this device. Other participants keep their own copies.','Delete history',async () => { historyGeneration++;await invoke('delete_history',{room});replyDrafts.delete(room);for(const key of messageCache.keys())if(key.startsWith(room+':'))messageCache.delete(key);await loadRoom(room); }); };
$('delete-room').onclick = () => { const room = selectedRoom; confirm('Delete this room for everyone?','The room and its history will be removed from each Ochat app when it reconnects to a participant who knows about the deletion. Downloaded files and exported copies are not removed. This cannot be undone.','Delete room',async () => { await invoke('delete_room',{room}); }); };
$('forget-room').onclick=()=>requestForgetRoom(selectedRoom);
$('composer').onsubmit = event => { event.preventDefault(); if (!$('message').value.trim()) return; if (!selectedRoom) { toast('Choose a conversation first. Your draft is kept.'); return; } action($('send-button'),async () => { const text = $('message').value,room=selectedRoom,channel=selectedChannel(),scope=draftKey(); const reply=replyDrafts.get(scope);const chosen=[...(chosenDraftEmojis.get(scope)?.values() || [])].filter(e=>text.includes(`:${e.name}:`));await invoke('send_text',{room,channel,text,emojis:chosen,mentions:mentionsForDraft(text),replyTo:reply?{id:reply.id,peer:reply.peer}:null});if(replyDrafts.get(scope)===reply)replyDrafts.delete(scope);renderReply(); if (draftKey()===scope && $('message').value === text) $('message').value = ''; drafts.set(scope,'');chosenDraftEmojis.delete(scope);selectedMentions.delete(scope);closeMentions(); $('message').style.height = 'auto'; stopTyping(); $('message').focus(); }); };
$('message').addEventListener('input',onInput); $('message').addEventListener('blur',()=>{stopTyping();closeMentions();}); $('message').addEventListener('keydown',event => { if(!$('mention-picker').hidden && !event.isComposing){if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();mentionIndex=(mentionIndex+(event.key==='ArrowDown'?1:-1)+mentionOptions.length)%mentionOptions.length;renderMentionPicker();return;}if(['Enter','Tab'].includes(event.key)){event.preventDefault();chooseMention(mentionIndex);return;}if(event.key==='Escape'){event.preventDefault();closeMentions();return;}} if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) { event.preventDefault(); $('composer').requestSubmit(); } });
$('timeline').addEventListener('scroll',scheduleRead);window.addEventListener('focus',scheduleRead);document.addEventListener('focusin',scheduleRead);document.addEventListener('click',scheduleRead);window.addEventListener('blur',()=>{clearTimeout(readTimer);readTimer=null;reportView();});document.addEventListener('visibilitychange',()=>{reportView();scheduleRead();});
$('cancel-reply').onclick=()=>{replyDrafts.delete(draftKey());renderReply();$('message').focus();};
$('latest-messages').onclick=()=>action($('latest-messages'),()=>loadRoom(selectedRoom));
$('search-button').onclick=()=>{fillSearchChannels();searchGeneration++;searchRoom=selectedRoom;$('search-results').replaceChildren();$('search-status').textContent='';$('search-query').value='';$('search-scope').value=selectedRoom?'room':'all';$('search-scope').querySelector('[value="room"]').disabled=!selectedRoom;$('search-next').hidden=$('search-previous').hidden=true;show('search-dialog');$('search-query').focus();};
$('search-form').onsubmit=event=>{event.preventDefault();runSearch();};
$('search-query').oninput=()=>{searchGeneration++;$('search-results').replaceChildren();$('search-status').textContent='';$('search-next').hidden=$('search-previous').hidden=true;};
$('search-scope').onchange=()=>{searchGeneration++;if($('search-query').value.trim())runSearch();};
$('search-next').onclick=()=>runSearch(searchOffset+50);$('search-previous').onclick=()=>runSearch(Math.max(0,searchOffset-50));
$('search-dialog').addEventListener('close',()=>{searchGeneration++;$('search-query').value='';$('search-results').replaceChildren();$('search-status').textContent='';});
$('room-notifications-button').onclick=()=>action($('room-notifications-button'),async()=>{
  const room=selectedRoom,generation=sessionGeneration,next=await invoke('get_settings');if(!unlocked || generation!==sessionGeneration || room!==selectedRoom || deletedRooms.has(room))return;
  settings=next;notificationRoom=room;$('room-notifications-dialog').dataset.parentRoom=room;$('notification-target').value='room';const channel=channelContextFor(room)?.channels?.find(c=>c.id===selectedChannel(room));$('notification-target').options[1].disabled=!channel;const preference=settings.room_notifications?.[room] || {};
  $('room-notifications-name').textContent=conversationContext(room).title;$('room-notification-scope').value=preference.scope || 'inherit';
  for(const [id,key] of [['sound','sound'],['preview','previews'],['ring','ringtone']])$('room-notification-'+id).value=preference[key]==null?'inherit':preference[key]?'on':'off';
  $('room-notifications-error').textContent='';show('room-notifications-dialog');
});
$('room-notifications-save').onclick=()=>action($('room-notifications-save'),async()=>{
  const room=notificationRoom,generation=sessionGeneration;if(!room || deletedRooms.has(room))return;
  const next=await invoke('get_settings');if(!unlocked || generation!==sessionGeneration || deletedRooms.has(room))return;
  const preference={scope:$('room-notification-scope').value==='inherit'?null:$('room-notification-scope').value};for(const [id,key] of [['sound','sound'],['preview','previews'],['ring','ringtone']]){const value=$('room-notification-'+id).value;preference[key]=value==='inherit'?null:value==='on';}
  next.room_notifications={...next.room_notifications,[room]:preference};
  try{await invoke('save_settings',{settings:next});if(!unlocked || generation!==sessionGeneration)return;settings=next;$('room-notifications-dialog').close();}catch(error){$('room-notifications-error').textContent=String(error);}
});
document.addEventListener('keydown',event=>{
  if(event.code===(settings?.audio.talk_key || 'Space') && !event.repeat && !event.isComposing && !editableTarget(event.target) && pressTalk('key'))event.preventDefault();
},true);
document.addEventListener('keyup',event=>{if(talkSource==='key' && event.code===(settings?.audio.talk_key || 'Space')){event.preventDefault();releaseTalk();}},true);
$('talk-button').addEventListener('pointerdown',event=>{if(event.button!==0)return;if(pressTalk('pointer')){event.preventDefault();$('talk-button').setPointerCapture?.(event.pointerId);}});
for(const event of ['pointerup','pointercancel','lostpointercapture'])$('talk-button').addEventListener(event,releaseTalk);
$('talk-button').addEventListener('keydown',event=>{if(['Space','Enter'].includes(event.code) && !event.repeat){event.preventDefault();pressTalk('button-key');}});
$('talk-button').addEventListener('keyup',event=>{if(talkSource==='button-key' && ['Space','Enter'].includes(event.code)){event.preventDefault();releaseTalk();}});
window.addEventListener('blur',releaseTalk);document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='hidden')releaseTalk();});document.addEventListener('focusin',event=>{if(editableTarget(event.target))releaseTalk();});window.addEventListener('pagehide',releaseTalk);
$('attach-button').onclick = () => action($('attach-button'),() => invoke('send_file'));
$('voice-button').onclick = () => action($('voice-button'),() => invoke(state.room.capacity === 2 ? 'call_peer' : 'start_voice'));
$('stop-voice').onclick = () => {releaseTalk();const voice=activeVoice();if(!voice)return;return action($('stop-voice'),() => invoke(voice.call ? 'end_call' : 'stop_voice',{room:voice.room.id}));};
$('mute-button').onclick = () => {releaseTalk();const voice=activeVoice();if(!voice)return;return action($('mute-button'),() => invoke('set_muted',{room:voice.room.id,muted:!voice.muted}));};
$('accept-call').onclick = () => action($('accept-call'),async () => { try { await invoke('accept_call',{id:state.call.id}); } catch (e) { $('call-error').textContent = String(e); throw e; } });
$('decline-call').onclick = $('cancel-call').onclick = () => action(null,() => invoke('end_call'));
$('call-dialog').addEventListener('cancel',event => { event.preventDefault(); action(null,() => invoke('end_call')); });
$('settings-button').onclick = () => action($('settings-button'),openSettings);
function setNavigationSection(section) {
  for(const name of ['rooms','friends']){const selected=name===section;$(`navigation-${name}`).hidden=!selected;$(`navigation-${name}-button`).classList.toggle('selected',selected);$(`navigation-${name}-button`).setAttribute('aria-pressed',String(selected));}
  $('friends-button').setAttribute('aria-pressed',String(section==='friends'));document.body.dataset.navigationSection=section;window.OchatDiscord?.sync(conversationContext(selectedRoom),unlocked,Array.isArray(state?.channels));window.OchatChannels?.sync(snapshotFor(selectedRoom),unlocked);
}
function syncNavigationLayout() {
  if(!unlocked)return;window.OchatDiscord?.layout();
  const dialog=$('navigation-dialog'),docked=window.innerWidth>=900,focused=document.activeElement;
  if(docked){
    if(dialog.open && navigationModal){panelOpeners.delete(dialog.id);dialog.close();}
    navigationModal=false;
    if(!dialog.open){dialog.show();if(focused?.isConnected)focused.focus({preventScroll:true});}
    $('navigation-toggle').setAttribute('aria-expanded','true');
  }else{
    if(dialog.open && !navigationModal){panelOpeners.delete(dialog.id);dialog.close();if(dialog.contains(focused))$('navigation-toggle').focus({preventScroll:true});}
    navigationModal=true;$('navigation-toggle').setAttribute('aria-expanded',String(dialog.open));
  }
}
function openNavigation(opener=$('navigation-toggle')) {
  syncNavigationLayout();const dialog=$('navigation-dialog');
  if(!dialog.open){panelOpeners.set(dialog.id,opener);navigationModal=true;show(dialog.id);$('navigation-toggle').setAttribute('aria-expanded','true');}
}
$('navigation-toggle').onclick=()=>{if($('navigation-dialog').open && navigationModal){closePanel('navigation-dialog');return;}setNavigationSection('rooms');openNavigation();};
$('add-friend-button').onclick=()=>{if(!unlocked)return;$('friend-contact-id').value='';show('add-friend-dialog');$('friend-contact-id').focus();};
$('copy-contact-id').onclick=()=>action($('copy-contact-id'),async()=>{
  const generation=sessionGeneration,label=$('copy-contact-id').textContent;$('copy-contact-id').textContent='Preparing contact ID…';
  try{let identifier;try{identifier=await invoke('contact_id');}catch(error){if(unlocked && generation===sessionGeneration)throw error;return;}if(!unlocked || generation!==sessionGeneration)return;await invoke('copy_text',{text:identifier});if(unlocked && generation===sessionGeneration)toast('Contact ID copied. Share it with the person you want to add.');}
  finally{if(generation===sessionGeneration)$('copy-contact-id').textContent=label;}
}).finally(()=>{if(!unlocked)$('copy-contact-id').disabled=true;});
$('add-friend-form').onsubmit=event=>{event.preventDefault();formAction(event.currentTarget,async()=>{
  const generation=sessionGeneration,identifier=$('friend-contact-id').value.trim();if(!identifier)throw Error('Paste your friend’s contact ID.');
  let result;try{result=await invoke('send_friend_request',{identifier});}catch(error){if(unlocked && generation===sessionGeneration)throw error;return;}if(!unlocked || generation!==sessionGeneration)return;
  await refreshFriends();if(!unlocked || generation!==sessionGeneration)return;
  $('friend-contact-id').value='';if($('add-friend-dialog').open){$('add-friend-dialog').close();setNavigationSection('friends');openNavigation($('friends-button'));}
  toast(result.status==='accepted'?'You are friends. Choose Message to start a private conversation.':result.status==='blocked'?'This request was dismissed. It will not be retried automatically.':(result.delivered && !result.request_error)?'Request sent. You can follow it in Sent requests.':'Contact not reachable yet. The saved request will retry while Ochat is unlocked. See Sent requests → Connection details or use Retry.');
});};
$('friends-button').onclick=()=>{refreshFriendsFromNavigation();setNavigationSection('friends');openNavigation($('friends-button'));$('navigation-friends-button').focus({preventScroll:true});};
for(const section of ['rooms','friends'])$(`navigation-${section}-button`).onclick=()=>{setNavigationSection(section);if(section==='friends')refreshFriendsFromNavigation();};
$('details-toggle').onclick=openDetails;
$('conversation-identity').onclick=()=>{const context=conversationContext(selectedRoom);if(context.peer)showPeer(context.peer.id,context.peer.name);else if(!$('details-dialog').open)openDetails();else {inspectedPeer=null;renderPeerCard();}};
$('profile-back').onclick=()=>{window.OchatProfiles?.close();inspectedPeer=null;renderPeerCard();$('details-dialog').scrollTop=0;const target=$('members').querySelector('.member-profile');(target || $('details-toggle')).focus({preventScroll:true});};
$('room-actions-toggle').onclick=()=>{panelOpeners.set('room-actions-dialog',$('room-actions-toggle'));show('room-actions-dialog');};
for(const id of ['navigation-dialog','details-dialog','room-actions-dialog'])$(id).addEventListener('close',()=>{
  if($(id).open)return;if(id==='details-dialog'){if(!detailsClosingForResize)restoreDetailsOnWide=false;detailsClosingForResize=false;}const button=id==='navigation-dialog'?$('navigation-toggle'):id==='details-dialog'?$('details-toggle'):$('room-actions-toggle');button.setAttribute('aria-expanded','false');
  const opener=panelOpeners.get(id);panelOpeners.delete(id);if(opener?.isConnected && !hasBlockingDialog())opener.focus({preventScroll:true});reportView();scheduleRead();
});
document.addEventListener('keydown',event=>{if(event.key==='Escape' && $('details-dialog').open && !hasBlockingDialog()){event.preventDefault();closePanel('details-dialog');}});
document.addEventListener('keydown',event=>{if(event.key==='Escape')endTabDrag();});
document.addEventListener('keydown',event=>{const tabRoot=event.target.closest?.('#conversation-tabs,#home-direct-tabs');if(!tabRoot)return;if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End'].includes(event.key))return;const tabs=[...tabRoot.querySelectorAll('[role="tab"]')],index=tabs.indexOf(document.activeElement);if(index<0)return;event.preventDefault();if(event.altKey && ['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)){const id=tabs[index].parentElement.dataset.key,group=tabs.filter(tab=>conversationLayout.pinned.includes(tab.parentElement.dataset.key)===conversationLayout.pinned.includes(id)),position=group.indexOf(tabs[index]),target=group[position+(['ArrowRight','ArrowDown'].includes(event.key)?1:-1)];if(target)moveConversation(id,target.parentElement.dataset.key,['ArrowRight','ArrowDown'].includes(event.key));return;}const next=event.key==='Home'?0:event.key==='End'?tabs.length-1:(index+(['ArrowRight','ArrowDown'].includes(event.key)?1:-1)+tabs.length)%tabs.length;tabs[next].focus({preventScroll:true});tabs[next].scrollIntoView?.({block:'nearest',inline:'nearest'});});
window.addEventListener('resize',()=>{
  syncNavigationLayout();const dialog=$('details-dialog'),modal=dialog.classList.contains('profile-overlay') || window.innerWidth<1180;
  if(!modal && restoreDetailsOnWide && !dialog.open && selectedRoom && !hasBlockingDialog()){const focused=document.activeElement;dialog.show();detailsModal=false;restoreDetailsOnWide=false;$('details-toggle').setAttribute('aria-expanded','true');focused?.focus({preventScroll:true});}
  if(dialog.open && modal!==detailsModal){if(modal && !dialog.contains(document.activeElement)){detailsClosingForResize=true;closePanel(dialog.id,false);restoreDetailsOnWide=true;detailsModal=modal;return;}if(modal && [...document.querySelectorAll('dialog[open]')].some(d=>d.id!=='details-dialog' && (d.id!=='navigation-dialog' || navigationModal))){closePanel(dialog.id,false);detailsModal=modal;return;}const focused=dialog.contains(document.activeElement)?document.activeElement:null,scroll=dialog.scrollTop;panelOpeners.delete(dialog.id);dialog.close();detailsModal=modal;if(modal){closePanel('navigation-dialog',false);dialog.showModal();}else dialog.show();panelOpeners.set(dialog.id,$('details-toggle'));$('details-toggle').setAttribute('aria-expanded','true');dialog.scrollTop=scroll;focused?.focus({preventScroll:true});}
  positionMentions();
});
function positionMentions(){if($('mention-picker').hidden)return;const panel=$('conversation-panel').getBoundingClientRect(),composer=$('composer').getBoundingClientRect();$('mention-picker').style.bottom=`${Math.max(80,panel.bottom-composer.top+6)}px`;}
function previewAccent(value) {const valid=/^#[0-9a-f]{6}$/i.test(value);$('accent-error').textContent=valid?'':'Use an accent color in #RRGGBB format';$('settings-save').disabled=!valid;if(valid){$('accent-color').value=value;$('accent-hex').value=value.toUpperCase();applyAccent(value);}}
$('accent-color').oninput=()=>previewAccent($('accent-color').value);
$('accent-hex').oninput=()=>previewAccent($('accent-hex').value);
$('accent-reset').onclick=()=>previewAccent('#007AFF');
window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener?.('change',()=>applyAccent($('settings-dialog').open && /^#[0-9a-f]{6}$/i.test($('accent-hex').value)?$('accent-hex').value:settings?.accent_color || '#007AFF'));
document.querySelectorAll('[data-settings-tab]').forEach(button => button.onclick = () => { document.querySelectorAll('[data-settings-tab]').forEach(b => b.classList.toggle('selected',b === button)); document.querySelectorAll('[data-settings-panel]').forEach(panel => { panel.hidden = panel.dataset.settingsPanel !== button.dataset.settingsTab; }); });
$('theme-choice').onchange = () => applyTheme($('theme-choice').value);
for (const id of ['input-volume','output-volume','sound-volume']) $(id).oninput = updateRangeLabels;
$('refresh-devices').onclick = () => action($('refresh-devices'),refreshDevices);
$('settings-save').onclick = () => action($('settings-save'),async () => { const next = readSettings(); try { const relock=await invoke('save_settings',{settings:next});if(relock===true){settings=next;await lockAccount();toast('Connection mode changed. Unlock your account to reconnect.');return;} settings = next;for(const row of $('timeline').querySelectorAll('[data-message-id]'))renderDelivery(row);previewEpoch++;previewCache.clear();previewObserver?.disconnect();for(const card of $('timeline').querySelectorAll('.link-preview')){card._reset?.();if(next.link_previews && card._load)previewObserver?previewObserver.observe(card):card._load();}applyTheme(next.theme,next.accent_color); $('settings-dialog').close(); } catch (e) { $('settings-error').textContent = String(e); throw e; } });
$('settings-cancel').onclick = () => { applyTheme(settings.theme); $('settings-dialog').close(); };
$('settings-dialog').addEventListener('close',() => { stopAudioPoll(); if (settings) applyTheme(settings.theme); });
$('test-microphone').onclick = () => action($('test-microphone'),() => invoke('test_microphone',{settings:readSettings()}));
$('stop-test').onclick = () => action($('stop-test'),() => invoke('stop_microphone_test'));
$('test-notification').onclick = () => action($('test-notification'),async()=>{await invoke('test_notification');toast('Test notification requested. Your system settings may suppress it.');});
for(const [id,remove] of [['choose-room-illustration',false],['remove-room-illustration',true]])$(id).onclick=()=>action($(id),async()=>{const room=selectedRoom,generation=sessionGeneration;if(await (remove?invoke('set_room_illustration',{room,remove}):window.OchatPhotoCrop.choose({kind:'room',room}))){if(unlocked && generation===sessionGeneration)await refreshRooms();}});
$('lock-button').onclick = () => action($('lock-button'),lockAccount);
$('reveal-key-button').onclick = () => show('reauth-dialog');
$('reauth-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { const generation = sessionGeneration; const key = await invoke('reveal_key',{password:$('reauth-password').value}); if (!unlocked || generation !== sessionGeneration) return; $('reauth-password').value = ''; $('reauth-dialog').close(); showRecovery(key,false); }); };
$('change-password-button').onclick = () => show('password-dialog');
$('password-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { matchesPassword('new-password','new-password-confirm'); await invoke('change_password',{oldPassword:$('old-password').value,newPassword:$('new-password').value}); clearPasswords(); $('password-dialog').close(); toast('Local password changed. Your recovery key stays the same.'); }); };
$('copy-identity').onclick = () => action($('copy-identity'),async () => { await invoke('copy_text',{text:state.id}); toast('Public identity copied.'); });
$('choose-profile-photo').onclick = () => action($('choose-profile-photo'),async () => { const generation = sessionGeneration; if (await window.OchatPhotoCrop.choose({kind:'profile'})) { const next = await invoke('snapshot'); if (!unlocked || generation !== sessionGeneration) return; state = next; membersKey = ''; render(); profileAvatar($('profile-photo-preview'),state.name,state.avatar_hash,state.id);window.OchatProfiles?.settings(state); $('remove-profile-photo').hidden = !state.avatar_hash; } });
$('remove-profile-photo').onclick = () => action($('remove-profile-photo'),async () => { const generation = sessionGeneration; await invoke('remove_profile_photo'); const next = await invoke('snapshot'); if (!unlocked || generation !== sessionGeneration) return; state = next; membersKey = ''; render(); profileAvatar($('profile-photo-preview'),state.name,state.avatar_hash,state.id);window.OchatProfiles?.settings(state); $('remove-profile-photo').hidden = true; });
$('export-backup').onclick = () => action($('export-backup'),async () => { if (await invoke('export_backup')) toast('Encrypted backup saved. Keep your recovery key separately.'); });
$('privacy-button').onclick = () => show('privacy-dialog');

function insertEmoji(text) {
  const origin=document.activeElement;window.OchatPersonalization?.rememberEmoji(text);const input=$('message');input.setRangeText(text,cursor[0],cursor[1],'end');cursor=[input.selectionStart,input.selectionEnd];if(!$('emoji-dialog').classList.contains('emoji-popover'))$('emoji-dialog').close();if(origin?.closest?.('#emoji-dialog')&&$('emoji-dialog').open)origin.focus({preventScroll:true});else input.focus({preventScroll:true});onInput(); }
for (const emoji of unicodeEmojis) { const button = el('button','unicode-emoji',emoji); button.setAttribute('aria-label',emoji); button.onclick = () => insertEmoji(emoji); $('unicode-grid').append(button); }
function setEmojiTab(custom) { $('personal-tab').classList.toggle('selected',custom && emojiScope==='personal'); $('unicode-panel').hidden = custom; $('custom-panel').hidden = !custom; $('unicode-tab').classList.toggle('selected',!custom); $('custom-tab').classList.toggle('selected',custom && emojiScope==='room'); }
async function openEmojis(custom,scope='room',compact=$('emoji-dialog').classList.contains('emoji-popover')) {
  const ticket=++emojiLoadGeneration,generation=sessionGeneration,room=selectedRoom;
  const current=()=>unlocked && generation===sessionGeneration && selectedRoom===room && ticket===emojiLoadGeneration;
  cursor=[$('message').selectionStart||0,$('message').selectionEnd||0];
  const entries=scope==='personal'?await invoke('personal_emojis'):(room?await invoke('room_emojis',{room}):[]);
  if(!current())return;
  await window.OchatPersonalization?.recentEmojis();if(!current())return;
  emojiScope=scope;emojiEntries=entries;renderEmojiLibrary();setEmojiTab(custom);if(compact)window.OchatEmojiPopover.open();else show('emoji-dialog');
}
function renderEmojiLibrary() {
  $('custom-grid').replaceChildren(); $('add-emoji').disabled = (emojiScope==='room' && !isActiveRoom()) || emojiEntries.length >= 100; $('emoji-library-hint').textContent=emojiScope==='personal'?'Your private library. Only emojis used in sent messages are shared. A personal shortcut takes priority over a room emoji with the same name.':'Shared room library. Everyone can add, rename or remove emojis. Existing messages keep their images.'; $('add-emoji').textContent=emojiScope==='personal'?'Add personal photo emoji…':'Add to room library…';
  if (!emojiEntries.length) $('custom-grid').append(el('p','empty-small',emojiScope==='personal'?'Your personal library is empty.':selectedRoom ? 'No custom emojis in this room yet.' : 'Open a room to use its shared library.'));
  for (const entry of (window.OchatPersonalization?.recentEntries(emojiEntries) || emojiEntries)) { const row = el('div','emoji-library-row'),choose = el('button','emoji-library-choose'); customImage(entry,choose); choose.append(el('span','',`:${entry.name}:`)); choose.disabled=!isActiveRoom(); choose.onclick = () => {let chosen=chosenDraftEmojis.get(draftKey());if(!chosen){chosen=new Map();chosenDraftEmojis.set(draftKey(),chosen);}chosen.set(entry.name,{name:entry.name,hash:entry.hash});insertEmoji(`:${entry.name}:`);}; row.append(choose); const rename = el('button','text-button','Rename'),remove = el('button','text-button danger','Remove'); rename.disabled = remove.disabled = emojiScope==='room' && !isActiveRoom(); rename.onclick = () => { editingEmoji = entry; $('emoji-name').value = entry.name; $('emoji-edit-title').textContent = emojiScope==='personal'?'Rename personal emoji':'Rename room emoji'; $('emoji-edit-submit').textContent = 'Rename'; show('emoji-edit-dialog'); }; remove.onclick = () => confirm(emojiScope==='personal'?'Remove personal emoji?':'Remove this room emoji?',emojiScope==='personal'?'Existing sent messages keep their image.':`Everyone will see this change. Existing messages keep the image for :${entry.name}:.`,'Remove emoji',async () => { const scope=emojiScope;await invoke(scope==='personal'?'edit_personal_emoji':'edit_emoji',{id:entry.id,name:entry.name,removed:true}); if(scope==='personal' && unlocked){const generation=sessionGeneration;const entries=await invoke('personal_emojis');if(unlocked && generation===sessionGeneration){emojiEntries=entries;renderEmojiLibrary();}} }); row.append(rename,remove); $('custom-grid').append(row); }
}
$('emoji-button').onclick = () => {if($('emoji-dialog').open&&$('emoji-dialog').classList.contains('emoji-popover')){$('emoji-dialog').close();return;}return action($('emoji-button'),() => openEmojis(false,'room',true));}; $('shared-emojis-button').onclick = () => action($('shared-emojis-button'),() => openEmojis(true)); $('unicode-tab').onclick = () => setEmojiTab(false); $('custom-tab').onclick = () => action($('custom-tab'),()=>openEmojis(true,'room')); $('personal-tab').onclick=()=>action($('personal-tab'),()=>openEmojis(true,'personal')); $('my-emojis-button').onclick=()=>action($('my-emojis-button'),()=>openEmojis(true,'personal'));
$('add-emoji').onclick = () => { editingEmoji = null; $('emoji-name').value = ''; $('emoji-edit-title').textContent = emojiScope==='personal'?'Add personal emoji':'Add room emoji'; $('emoji-edit-submit').textContent = 'Choose photo…'; show('emoji-edit-dialog'); };
$('emoji-edit-form').onsubmit = event => { event.preventDefault(); formAction(event.currentTarget,async () => { if (emojiScope==='room' && !isActiveRoom()) throw new Error('Reconnect to this room to edit its shared library.'); const scope=emojiScope,generation=sessionGeneration; const name = $('emoji-name').value.trim(); if (editingEmoji) await invoke(scope==='personal'?'edit_personal_emoji':'edit_emoji',{id:editingEmoji.id,name,removed:false}); else if (!await invoke(scope==='personal'?'import_personal_emoji':'import_emoji',{name})) return; if(!unlocked || generation!==sessionGeneration)return; $('emoji-edit-dialog').close(); emojiEntries = scope==='personal'?await invoke('personal_emojis'):await invoke('room_emojis',{room:selectedRoom}); if(unlocked && generation===sessionGeneration)renderEmojiLibrary(); }); };

async function handleEvent(event) {
  window.OchatWorkspace?.onEvent(event);
  if(event.type==='native_screen_frame' || event.type==='native_screen_stopped' || event.type==='visual_available' || event.type==='visual_keyframe' || event.type==='visual_frame' || event.type==='visual_stopped' || event.type==='visual_tick' || event.type==='visual_packet' || event.type==='system_audio_state'){if(unlocked && !deletedRooms.has(event.room))window.OchatFeatures?.mediaEvent(event);return;}
  if(event.type==='message_updated'){if(!unlocked || deletedRooms.has(event.room))return;rememberUpdate(event.update);applyMessageUpdate(event.update);refreshActivity().catch(()=>{});if(collectionRoom===event.room && $('collection-dialog').open){collectionEntries=collectionEntries.map(e=>e.event.type==='message'?{...e,event:projectedEvent({...e.event,room:e.room})}:e).filter(e=>!e.event.deleted);renderCollection();}return;}
  if(event.type==='delivery_changed'){if(unlocked && event.room===selectedRoom)refreshDeliveries(event.room).catch(toast);return;}
  if(event.type==='friends_changed'){if(unlocked){await refreshFriends();refreshActivity().catch(()=>{});}return;}
  const removedLocally=event.type==='room_forgotten';
  if(removedLocally && deletedRooms.has(event.room))return;
  if (event.type === 'room_deleted' || removedLocally) deletedRooms.add(event.room);
  if (!unlocked) return;
  if (event.type === 'room_deleted' || removedLocally) {
    for(const [key,update] of toolUpdates)if(update.room===event.room)toolUpdates.delete(key);for(const key of deliveryStates.keys())if(key.startsWith(event.room+':'))deliveryStates.delete(key);if(collectionRoom===event.room){collectionEntries=[];$('collection-results').replaceChildren();$('collection-dialog').close();}activityEntries=activityEntries.filter(e=>e.room!==event.room);renderActivity();
    const wasSelected=selectedRoom===event.room;introducedProfiles.delete(event.room);attentionRooms.delete(event.room);endTabDrag();conversationLayout=normalizeLayout(conversationLayout);savedLayout=normalizeLayout(savedLayout);
    if(settings?.room_notifications)delete settings.room_notifications[event.room];
    if(inspectedPeer?.room===event.room){inspectedPeer=null;clearPeerCard();}
    if(wasSelected){closePanel('details-dialog',false);closePanel('room-actions-dialog',false);}
    chosenDraftEmojis.delete(event.room);replyDrafts.delete(event.room);for(const [key,message] of messageCache)if(message.room===event.room || key.startsWith(event.room+':'))messageCache.delete(key);if(talkRoom===event.room)releaseTalk();searchGeneration++;$('search-results').replaceChildren();if(notificationRoom===event.room || $('room-notifications-dialog').dataset.parentRoom===event.room){$('room-notifications-dialog').close();notificationRoom=null;}readMarkers.delete(event.room);selectedMentions.delete(event.room);closeMentions(); deletedRooms.add(event.room); rooms = rooms.filter(room => room.id !== event.room); roomSnapshots.delete(event.room); roomMembers.delete(event.room); drafts.delete(event.room); unread.delete(event.room); assets.clear(); assetLoads.clear(); assetGeneration++;
    for (const [key,offer] of liveOffers) if (offer.room === event.room) liveOffers.delete(key);
    if (selectedRoom === event.room) { stopTyping(); historyGeneration++; selectedRoom = null; state = {...state,room:null,peers:[],members:[],voice:false,call:null,voice_started_at:null,speaking:false}; $('message').value = ''; emojiEntries = []; $('custom-grid').replaceChildren(); $('invitation-output').value = ''; for (const id of ['emoji-dialog','emoji-edit-dialog','invite-dialog','confirm-dialog']) $(id).close(); await loadRoom(null); }
    membersKey = ''; roomsKey = ''; render();
    if(removedLocally && wasSelected && allRooms().length)await selectRoom(allRooms()[0].id);
    if(removedLocally){$(`conversation-tab-${selectedRoom}`)?.focus({preventScroll:true});toast('Conversation and history deleted from this device.');}else toast('Room and history deleted by the owner.'); return;
  }
  if ((event.room && deletedRooms.has(event.room)) || (event.snapshot?.room && deletedRooms.has(event.snapshot.room.id))) return;
  if(event.type==='channels_changed'){
    const affected=event.room===selectedRoom && event.deleted===selectedChannel();
    if(event.deleted){if(notificationRoom===event.deleted){$('room-notifications-dialog').close();notificationRoom=null;}const scope=draftKey(event.room,event.deleted);for(const map of [drafts,replyDrafts,chosenDraftEmojis,selectedMentions,readMarkers])map.delete(scope);for(const [key,message]of messageCache)if(message.room===event.room && eventChannel(message)===event.deleted)messageCache.delete(key);if(toolMessage && eventChannel(toolMessage)===event.deleted)clearMessageTools();}
    await window.OchatChannels?.refresh?.();
    if(affected && unlocked){$('message').value='';renderReply();await loadRoom(event.room);}return;
  }
  if (event.type === 'room_state') { const snapshot = event.snapshot; if (!snapshot.room) return; roomSnapshots.set(snapshot.room.id,snapshot); roomMembers.set(snapshot.room.id,snapshot.members || snapshot.peers);render(); if (snapshot.call?.status === 'incoming' && selectedRoom !== snapshot.room.id) await selectRoom(snapshot.room.id); }
  else if (event.type === 'room_disconnected') { const previous = roomSnapshots.get(event.room); if (previous) roomMembers.set(event.room,(previous.members || previous.peers).map(p => ({...p,online:false,voice:false,typing:false}))); roomSnapshots.delete(event.room); roomsKey = ''; render(); }
  else if (event.type === 'state') { const previous = state?.room?.id; state = event.snapshot; if (state.room) { roomSnapshots.set(state.room.id,state); roomMembers.set(state.room.id,state.members || state.peers); } if (state.room?.id && state.room.id !== previous) { historyLoading=true; drafts.set(draftKey(),$('message').value); selectedRoom = state.room.id;  if (!drafts.has(draftKey())) drafts.set(draftKey(),$('message').value); $('message').value = drafts.get(draftKey()) || ''; await refreshRooms(); await loadRoom(selectedRoom); } render(); }
  else if (event.type === 'message') { window.OchatChannels?.message(event);refreshActivity().catch(()=>{});if(!rememberMessage(event))return;const added=inSelectedChannel(event)?addMessage(event,true):true; if (added && event.peer !== state.id && isUnread(event)) { unread.set(event.room,Math.min(999,(unread.get(event.room) || 0)+1));if(!conversationIsOpen(event.room))attentionRooms.add(event.room);roomsKey = ''; renderRooms(); } }
  else if (event.type === 'member_joined') { if (inSelectedChannel(event)) addJoinNotice(event); }
  else if (event.type === 'file_offered') { if (!event.outgoing) liveOffers.set(`${event.peer}:${event.offer.id}`,event); if (inSelectedChannel(event) && isActiveRoom()) addFile(event,true); }
  else if (event.type === 'transfer') { if (inSelectedChannel(event)) transfer(event); }
  else if (event.type === 'file_decision') { if (inSelectedChannel(event)) fileDecision(event); }
  else if (event.type === 'emoji_bank') { if (event.room === selectedRoom) { if(emojiScope==='room'){emojiEntries = event.entries; renderEmojiLibrary();} } }
  else if (event.type === 'emoji_ready') { assets.set(event.hash,event.data_url); document.querySelectorAll('[data-emoji-hash]').forEach(node => { if (node.dataset.emojiHash === event.hash) { const img = el('img','custom-emoji'); img.src = event.data_url; img.alt = `:${node.dataset.emojiName}:`; img.width = img.height = 24; node.replaceChildren(img); } }); document.querySelectorAll('[data-avatar-hash]').forEach(node => { if (node.dataset.avatarHash === event.hash) { const image = el('img','profile-photo'); image.src = event.data_url; image.alt = '';image.draggable=false; node.querySelector('.profile-photo')?.remove(); node.prepend(image); } }); }
  else if (event.type === 'call_ended') { refreshActivity().catch(()=>{});$('call-dialog').close(); const labels = {declined:'Call declined.',busy:'The participant is busy.',timeout:'No answer.',cancelled:'Call cancelled.',ended:'Call ended.',failed:'Call audio failed. Check Audio settings.',disconnected:'Call ended: participant disconnected.'}; toast(labels[event.reason] || 'Call ended.'); }
  else if (event.type === 'notice') toast(event.text);
}
async function boot() { if (!tauri) { toast('Start the native app with cargo run -p ochat.'); return; } await tauri.event.listen('ochat',({payload}) => { handleEvent(payload).catch(toast); }); accountExists = await invoke('account_status'); setAuthMode(accountExists ? 'unlock' : 'create'); applyTheme('system'); show('auth-dialog'); }
boot().catch(toast);


// Studio conversation tools. No content is written to browser storage.
const toolUpdates=new Map(),deliveryStates=new Map(),verificationCache=new Map();
let toolMessage=null,collectionRoom=null,collectionMode='all',collectionEntries=[],collectionKind='shared',verificationTarget=null,diagnosticPoll=null,activityEntries=[],activityBusy=false,activityReview=0;
function toolReference(event){return {peer:event.peer,id:event.id};}
function toolKey(room,reference){return `${room}:${reference.peer}:${reference.id}`;}
function updateSlot(update){return `${toolKey(update.room,update.message)}:${update.change.kind}:${update.change.kind==='reaction'?update.actor+':'+update.change.emoji:''}`;}
function rememberUpdate(update){
  const key=updateSlot(update),old=toolUpdates.get(key);
  if(old?.change.kind==='edit' && old.change.text===null)return;
  if(old && !(update.change.kind==='edit' && update.change.text===null) && (old.revision>update.revision || (old.revision===update.revision && old.actor>=update.actor)))return;
  toolUpdates.set(key,update);
}
function updatesFor(event){return [...toolUpdates.values()].filter(u=>u.room===event.room && u.message.id===event.id && u.message.peer===event.peer);}
function projectedEvent(event){
  let next={...event};for(const update of updatesFor(event)){if(update.change.kind==='edit')next={...next,text:update.change.text || '',emojis:update.change.emojis || [],mentions:[],edited:update.change.text!==null,deleted:update.change.text===null};}return next;
}
function messageRow(reference){return [...$('timeline').querySelectorAll('[data-message-id]')].find(row=>row.dataset.messageId===reference.id && row.dataset.peer===reference.peer);}
function toolButton(label,callback,cls='text-button'){const b=el('button',cls,label);b.type='button';b.onclick=()=>action(b,callback);return b;}
function enhanceMessage(row,event){
  event={...event,room:event.room || selectedRoom};const projected=projectedEvent(event);const body=row.querySelector('.message-body');
  const reply=body.querySelector('.reply-action');if(reply){reply.classList.add('message-reply');}
  const toolbar=el('div','message-toolbar');if(reply)toolbar.append(reply);
  const react=toolButton('React',()=>openReactions(event));react.setAttribute('aria-label',`React to message from ${event.name}`);
  const more=toolButton('•••',()=>openMessageTools(event,{anchor:more}));more.setAttribute('aria-label',`More actions for message from ${event.name}`);toolbar.append(react,toolButton('Thread',()=>window.OchatWorkspace?.openThread(event)),more);body.append(toolbar,el('div','message-reactions'));
  const status=el('div','message-delivery');status.setAttribute('aria-live','polite');body.append(status);
  row._toolEvent=event;if(projected.deleted || projected.edited){const bubble=body.querySelector('.bubble');bubble.replaceChildren();appendTextWithEmojis(bubble,projected.deleted?'Message deleted':projected.text,projected.emojis || [],[]);if(projected.deleted){body.querySelectorAll('.link-preview,.reply-quote').forEach(n=>n.remove());}}renderMessageMetadata(row);
}
function renderMessageMetadata(row){
  const event=projectedEvent(row._toolEvent);row.classList.toggle('deleted-message',!!event.deleted);
  const header=row.querySelector('.message-header');let edited=header.querySelector('.edited-label');if(!edited){edited=el('small','edited-label');header.insertBefore(edited,header.querySelector('time'));}edited.textContent=event.deleted?'Deleted':event.edited?'Edited':'';edited.hidden=!event.deleted && !event.edited;
  row.querySelector('.message-toolbar').hidden=!!event.deleted;
  const list=row.querySelector('.message-reactions');list.replaceChildren();
  const groups=new Map();if(!event.deleted)for(const update of updatesFor(event)){if(update.change.kind==='reaction' && update.change.active){const key=update.change.emoji;const people=groups.get(key)||[];people.push(update.actor);groups.set(key,people);}}
  for(const [emoji,people] of groups){const chip=toolButton('',()=>changeMessage(event,{kind:'reaction',emoji,active:!people.includes(state.id)}),'reaction-chip');chip.classList.toggle('selected',people.includes(state?.id));chip.setAttribute('aria-pressed',String(people.includes(state?.id)));const label=reactionLabel(emoji),names=people.map(peer=>peer===state?.id?'You':peerDetails(peer,event.room,peer.slice(0,8)).name);chip.title=names.join(', ');chip.setAttribute('aria-label',`${label}, ${people.length} reaction${people.length===1?'':'s'}: ${names.join(', ')}`);appendReactionIcon(chip,emoji);chip.append(el('span','',String(people.length)));list.append(chip);}
  let pin=header.querySelector('.message-pin');if(!pin){pin=el('small','message-pin','Pinned');header.insertBefore(pin,header.querySelector('time'));}pin.hidden=event.deleted || !updatesFor(event).some(u=>u.change.kind==='pin'&&u.change.active);if(!pin.hidden)row.classList.remove('continuation');
  renderDelivery(row);
}
function reactionLabel(emoji){return emoji.startsWith('custom:')?`:${emoji.split(':')[2]}:`:emoji;}
function appendReactionIcon(node,emoji){if(emoji.startsWith('custom:')){const [,hash,name]=emoji.split(':');customImage({hash,name},node);}else node.append(el('span','',emoji));}
async function changeMessage(event,change){const room=event.room || selectedRoom;await invoke('update_message',{room,message:toolReference(event),change});}
function applyMessageUpdate(update){
  const row=messageRow(update.message);if(update.room!==selectedRoom || !row)return;
  if(update.change.kind==='edit'){
    const event=projectedEvent(row._toolEvent),body=row.querySelector('.message-body'),bubble=body.querySelector('.bubble');bubble.replaceChildren();if(event.deleted || !window.OchatPersonalization?.image(bubble,event))appendTextWithEmojis(bubble,event.deleted?'Message deleted':event.text,event.deleted?[]:event.emojis,[]);
    body.querySelectorAll('.link-preview,.link-card,.link-preview-card,.link-preview-placeholder,.message-links').forEach(n=>n.remove());
    // Preview implementations may wrap cards; remove every preview resource in this message.
    body.querySelectorAll('[data-preview-url]').forEach(n=>n.remove());
    if(event.deleted)body.querySelector('.reply-quote')?.remove();
    messageCache.set(messageKey(update.room,event),event);
    document.querySelectorAll('.reply-quote').forEach(quote=>{if(quote.dataset.replyId===update.message.id && quote.dataset.replyPeer===update.message.peer && quote.dataset.replyRoom===update.room){const text=quote.querySelector('span');if(text)text.textContent=event.deleted?'Message deleted':messageSummary(event.text).slice(0,160);}});
    const draft=replyDrafts.get(update.room);if(draft?.id===update.message.id && draft.peer===update.message.peer && event.deleted){replyDrafts.delete(update.room);renderReply();}
    if(!event.deleted)appendLinkPreviews(body,event.text);
    row.classList.remove('continuation');
    const following=row.nextElementSibling;if(following?.classList.contains('message'))following.classList.remove('continuation');
  }
  renderMessageMetadata(row);
}
async function refreshMessageTools(room){
  if(!room || !unlocked)return;const generation=sessionGeneration;
  const [updates,deliveries]=await Promise.all([invoke('message_updates',{room}),invoke('message_deliveries',{room})]);
  if(!unlocked || generation!==sessionGeneration || deletedRooms.has(room))return;
  for(const update of updates || [])rememberUpdate(update);
  for(const item of deliveries || [])deliveryStates.set(`${room}:${item.id}`,item);
  if(selectedRoom===room){for(const row of $('timeline').querySelectorAll('[data-message-id]')){for(const update of updatesFor(row._toolEvent))applyMessageUpdate(update);renderMessageMetadata(row);}}
}
function renderDelivery(row){
  const event=row._toolEvent,node=row.querySelector('.message-delivery');node.replaceChildren();if(event.peer!==state?.id || projectedEvent(event).deleted)return;
  const info=deliveryStates.get(`${event.room}:${event.id}`);if(!info)return;
  const wasRead=!!settings?.read_receipts && info.expected?.length>0 && info.expected.every(p=>info.read?.includes(p));const text=wasRead?'Read':info.status==='delivered'?'Delivered':info.status==='failed'?'Not confirmed':info.status==='pending'?(info.received?.length?`Delivered to ${info.received.length}/${info.expected.length} · ${info.expected.length-info.received.length} pending`:'Pending · waiting for delivery'):info.status==='cancelled'?'Cancelled':'Sending…';node.append(el('span','',text));node.dataset.status=info.status;
  node.title=`Confirmed by ${info.received?.length || 0} of ${info.expected?.length || 0} intended recipients. This does not mean read.`;if(info.status==='pending'){const waiting=(info.expected||[]).filter(id=>!info.received?.includes(id)).map(id=>peerDetails(id,event.room).name);if(waiting.length)node.title+=' Awaiting: '+waiting.join(', ')+'. Offline recipients receive it when you are both connected.';}
  if(info.status==='pending')node.append(toolButton('Cancel',async()=>{await invoke('cancel_pending_message',{room:event.room,id:event.id});await refreshDeliveries(event.room);}));
  if(info.status==='failed' || info.status==='cancelled')node.append(toolButton('Retry',async()=>{await invoke('retry_message',{room:event.room,id:event.id});await refreshDeliveries(event.room);}));
}
async function refreshDeliveries(room){const generation=sessionGeneration,items=await invoke('message_deliveries',{room});if(!unlocked || generation!==sessionGeneration)return;for(const item of items || [])deliveryStates.set(`${room}:${item.id}`,item);if(room===selectedRoom)for(const row of $('timeline').querySelectorAll('[data-message-id]'))renderDelivery(row);}
let messageMenuAnchor=null;
function closeMessageMenu(restore=true){
  const menu=$('message-tools-dialog');if(!menu.open)return;
  if(!restore)dialogReturnFocus.delete(menu.id);menu.close();messageMenuAnchor=null;
}
function placeMessageMenu(point,anchor){
  const menu=$('message-tools-dialog'),rect=menu.getBoundingClientRect(),origin=anchor?.getBoundingClientRect();
  const x=point?.x??origin?.right??12,y=point?.y??origin?.bottom??12;
  menu.style.left=Math.max(8,Math.min(innerWidth-(rect.width||224)-8,x))+'px';
  menu.style.top=Math.max(8,Math.min(innerHeight-(rect.height||300)-8,y))+'px';
}
function openMessageTools(event,{point,anchor}={}){
  if(!unlocked || !event || projectedEvent(event).deleted)return;
  closeMessageMenu(false);window.OchatEmojiPopover?.close(false);releaseTalk();
  const menu=$('message-tools-dialog'),room=selectedRoom,scope=draftKey(),generation=sessionGeneration;
  toolMessage=event;messageMenuAnchor=anchor||messageRow(event)?.querySelector('.message-toolbar button:last-child')||document.activeElement;
  dialogReturnFocus.set(menu.id,messageMenuAnchor);
  $('message-tools-context').textContent=`${event.name} · ${new Date(event.timestamp || Date.now()).toLocaleString('en-US')}${roomSnapshots.get(event.room)?.peers.some(p=>p.message_tools===false)?' · Some participants need Ochat 0.5 to see message changes.':''}`;
  const list=$('message-tools-actions');list.replaceChildren();
  const add=(label,callback,danger=false)=>{const b=toolButton(label,async()=>{if(!unlocked || generation!==sessionGeneration || room!==selectedRoom || scope!==draftKey())return;closeMessageMenu(false);await callback();},'message-menu-item'+(danger?' danger':''));b.setAttribute('role','menuitem');list.append(b);return b;};
  add('Reply',()=>beginReply(event));
  add('Add reaction…',()=>openReactions(event));
  add('Open thread',()=>window.OchatWorkspace?.openThread(event));
  add('Copy text',async()=>{await invoke('copy_text',{text:projectedEvent(event).text});toast('Message copied.');});
  const pinned=updatesFor(event).some(u=>u.change.kind==='pin'&&u.change.active);
  const pinButton=add(pinned?'Unpin message':'Pin message',()=>changeMessage(event,{kind:'pin',active:!pinned}));pinButton.disabled=snapshotFor(event.room)?.permissions?.pins===false;pinButton.title=pinButton.disabled?'Your role cannot pin messages':'';
  if(event.peer===state.id){
    add('Edit message',()=>{toolMessage=event;$('edit-message-text').value=projectedEvent(event).text;$('edit-message-form').querySelector('.form-error').textContent='';show('edit-message-dialog');$('edit-message-text').focus();});
    add('Delete message…',()=>confirm('Delete this message?','Remove its content from this conversation. Deletion synchronizes when participants reconnect. Recipients may have kept copies or backups.','Delete message',()=>changeMessage(event,{kind:'edit',text:null,emojis:[]})),true);
  }
  menu.show();placeMessageMenu(point,messageMenuAnchor);list.querySelector('button:not(:disabled)')?.focus({preventScroll:true});
}
$('message-tools-dialog').addEventListener('close',()=>{if(!$('message-tools-dialog').open)messageMenuAnchor=null;});
$('message-tools-dialog').addEventListener('keydown',event=>{
  if(!['ArrowDown','ArrowUp','Home','End'].includes(event.key))return;
  const items=[...$('message-tools-actions').querySelectorAll('button:not(:disabled)')];if(!items.length)return;event.preventDefault();
  const i=items.indexOf(document.activeElement),next=event.key==='Home'?0:event.key==='End'?items.length-1:(i+(event.key==='ArrowUp'?-1:1)+items.length)%items.length;items[next].focus();
});
document.addEventListener('pointerdown',event=>{const menu=$('message-tools-dialog');if(menu.open&&!menu.contains(event.target))closeMessageMenu(false);},true);
document.addEventListener('keydown',event=>{const menu=$('message-tools-dialog');if(!menu.open || [...document.querySelectorAll('dialog[open]')].some(d=>d!==menu&&d.matches?.(':modal')))return;if(event.key==='Escape'){event.preventDefault();event.stopPropagation();closeMessageMenu();}else if(event.key==='Tab')closeMessageMenu(false);},true);
window.addEventListener('resize',()=>closeMessageMenu(false));
$('timeline').addEventListener('wheel',()=>closeMessageMenu(false),{passive:true});
$('edit-message-form').onsubmit=e=>{e.preventDefault();formAction(e.currentTarget,async()=>{const event=toolMessage;if(!event)throw Error('Choose a message first');await changeMessage(event,{kind:'edit',text:$('edit-message-text').value,emojis:[]});$('edit-message-dialog').close();$('edit-message-text').value='';});};
async function openReactions(event){
  toolMessage=event;const unicode=$('reaction-unicode'),custom=$('reaction-custom');unicode.replaceChildren();custom.replaceChildren();
  function button(emoji){const active=updatesFor(event).some(u=>u.actor===state.id&&u.change.kind==='reaction'&&u.change.emoji===emoji&&u.change.active);const b=toolButton('',async()=>{await changeMessage(event,{kind:'reaction',emoji,active:!active});if(!active)window.OchatPersonalization?.rememberEmoji(emoji);$('reaction-dialog').close();},'emoji-choice');b.setAttribute('aria-label',reactionLabel(emoji));appendReactionIcon(b,emoji);return b;}
  for(const emoji of (window.OchatPersonalization?.orderedEmojis(unicodeEmojis) || unicodeEmojis))unicode.append(button(emoji));
  show('reaction-dialog');unicode.querySelector('button')?.focus();const generation=sessionGeneration,entries=await invoke('room_emojis',{room:event.room});if(!unlocked || generation!==sessionGeneration || toolMessage!==event)return;
  for(const entry of entries || [])if(!entry.removed)custom.append(button(`custom:${entry.hash}:${entry.name}`));emptyList(custom,'No room emojis yet.');
}
async function openCollection(kind){
  if(!selectedRoom)return;collectionRoom=selectedRoom;collectionKind=kind;collectionMode='all';collectionEntries=[];$('room-actions-dialog').close();$('collection-query').value='';$('collection-title').textContent=kind==='pins'?'Pinned messages':'Shared files & links';$('collection-tabs').hidden=kind==='pins';$('collection-status').textContent='Loading…';$('collection-results').replaceChildren();show('collection-dialog');const room=collectionRoom,generation=sessionGeneration;
  let loadedEntries;
  if(kind==='pins'){const updates=await invoke('message_updates',{room});const pinned=(updates || []).filter(u=>u.change.kind==='pin'&&u.change.active);loadedEntries=(await Promise.all(pinned.map(u=>invoke('message_by_id',{room,reference:u.message}).then(entry=>entry?{...entry,pinned_by:u.actor}:null)))).filter(e=>e && !e.event.deleted);}
  else loadedEntries=(await invoke('shared_items',{room})) || [];
  if(!unlocked || generation!==sessionGeneration || room!==collectionRoom)return;collectionEntries=loadedEntries;renderCollection();
}
function renderCollection(){
  const list=$('collection-results');list.replaceChildren();const query=$('collection-query').value.toLocaleLowerCase();
  for(const entry of collectionEntries){const event=entry.event,file=event.type==='file_offered',image=/^ochat-image:[a-f0-9]{64}$/.test(event.text||'');if(collectionMode==='files'&&!file || collectionMode==='links'&&(file||image) || collectionMode==='images'&&(!image && (!file || !/\.(png|jpe?g|webp|gif|avif)$/i.test(event.offer.name))))continue;const title=file?event.offer.name:messageSummary(event.text);if(!`${title} ${event.name}`.toLocaleLowerCase().includes(query))continue;
  const item=el('div','collection-item');item.append(el('strong','',file?event.offer.name:event.name),el('small','',`${event.name} · ${new Date(entry.timestamp).toLocaleString('en-US')}`));
  if(file)item.append(el('p','field-hint',`${size(event.offer.size)} · File offer, not an archived download`));else {if(image)window.OchatPersonalization?.image(item,{...event,room:entry.room});else item.append(el('p','collection-excerpt',messageSummary(event.text)));for(const url of messageLinks(event.text))item.append(linkAnchor(typeof url==='string'?url:url.url,typeof url==='string'?url:url.url));}
  if(entry.pinned_by)item.append(el('small','',`Pinned by ${entry.pinned_by===state.id?'you':peerDetails(entry.pinned_by,entry.room,entry.pinned_by.slice(0,8)).name}`));
  item.append(toolButton('View in conversation',async()=>{$('collection-dialog').close();if(file){await selectRoom(entry.room);await loadRoom(entry.room,{id:event.offer.id,peer:event.peer});const row=[...$('timeline').querySelectorAll('[data-file-id]')].find(r=>r.dataset.fileId===event.offer.id);window.OchatTimeline?.pause();row?.scrollIntoView({block:'center'});}else await jumpMessage(entry.room,toolReference(event));}));list.append(item);}
  $('collection-status').textContent=`${list.children.length} item${list.children.length===1?'':'s'}`;emptyList(list,collectionKind==='pins'?'No pinned messages. Use a message’s action menu to pin one.':'No matching saved files or links.');
  document.querySelectorAll('[data-collection]').forEach(b=>b.classList.toggle('selected',b.dataset.collection===collectionMode));
}
$('collection-query').oninput=renderCollection;document.querySelectorAll('[data-collection]').forEach(b=>b.onclick=()=>{collectionMode=b.dataset.collection;renderCollection();});
$('pinned-messages-button').onclick=()=>action($('pinned-messages-button'),()=>openCollection('pins'));$('shared-items-button').onclick=()=>action($('shared-items-button'),()=>openCollection('shared'));
async function refreshActivity(){
  if(!unlocked || activityBusy)return;activityBusy=true;const generation=sessionGeneration;
  try{const items=await invoke('activity_items');if(!unlocked || generation!==sessionGeneration)return;activityEntries=items || [];renderActivity();}finally{activityBusy=false;}
}
function renderActivity(){
  const pending=friends.filter(f=>f.status==='incoming'),unseen=activityEntries.filter(e=>!e.event.activity_seen);$('activity-button').textContent=`Activity${unseen.length+pending.length?` (${Math.min(99,unseen.length+pending.length)})`:''}`;
  const list=$('activity-results');list.replaceChildren();
  for(const friend of pending){const item=el('div','collection-item activity-new');item.append(el('strong','',`${friend.name} sent a friend request`),toolButton('Review request',()=>{$('activity-dialog').close();setNavigationSection('friends');openNavigation($('friends-button'));}));list.append(item);}
  for(const entry of activityEntries){if(deletedRooms.has(entry.room))continue;const event=entry.event,item=el('div',`collection-item${event.activity_seen?'':' activity-new'}`),room=rooms.find(r=>r.id===entry.room)?.name || 'Saved conversation',label=event.type==='missed_call'?'Missed call':event.reply_to?.peer===state?.id?'Reply to your message':'Mentioned you';
  item.append(el('strong','',`${label} · ${event.name}`),el('small','',`${room} · ${new Date(entry.timestamp).toLocaleString('en-US')}`));if(event.text)item.append(el('p','collection-excerpt',messageSummary(event.text)));
  item.append(toolButton('Open',async()=>{$('activity-dialog').close();if(event.type==='missed_call')await selectRoom(entry.room);else await jumpMessage(entry.room,toolReference(event));}));list.append(item);}
  emptyList(list,'You are all caught up.');
}
$('activity-button').onclick=()=>action($('activity-button'),async()=>{await refreshActivity();if(unlocked)show('activity-dialog');});
$('review-activity').onclick=()=>action($('review-activity'),async()=>{await invoke('review_activity');await refreshActivity();});
async function openVerification(){
  const context=conversationContext(selectedRoom),target=inspectedPeer || context.peer;if(!target || target.id===state?.id)return;
  const peer=target.id,generation=sessionGeneration,data=await invoke('peer_verification',{peer});if(!unlocked || generation!==sessionGeneration)return;verificationTarget=data;verificationCache.set(peer,data);$('verification-name').textContent=peerDetails(peer,target.room || selectedRoom,target.name).name;$('verification-fingerprint').textContent=data.fingerprint;updateVerificationPanel();show('verification-dialog');
}
function updateVerificationPanel(){const data=verificationTarget;if(!data)return;$('verification-status').textContent=data.verified?'Verified on this device':data.same_name_verified?'A different verified identity uses this name. Compare this identity separately.':'Not verified';$('confirm-verification').hidden=!!data.verified;$('remove-verification').hidden=!data.verified;if($('peer-identity').textContent===data.peer)$('peer-verification-status').textContent=data.verified?'Verified identity':'Identity not verified';}
$('verify-peer-button').onclick=()=>action($('verify-peer-button'),openVerification);
$('copy-fingerprint').onclick=()=>action($('copy-fingerprint'),()=>invoke('copy_text',{text:verificationTarget?.fingerprint || ''}));
for(const [id,verified] of [['confirm-verification',true],['remove-verification',false]])$(id).onclick=()=>action($(id),async()=>{const data=verificationTarget;if(!data)return;await invoke('verify_identity',{peer:data.peer,fingerprint:data.fingerprint,verified});data.verified=verified;verificationCache.set(data.peer,data);updateVerificationPanel();});
async function refreshDiagnostics(){
  const generation=sessionGeneration,data=await invoke('voice_diagnostics');if(!unlocked || generation!==sessionGeneration || !$('voice-diagnostics-dialog').open)return;
  const audio=data.audio || {},snapshot=data.snapshot,devices=diagnosticDevices;if(!unlocked || generation!==sessionGeneration || !$('voice-diagnostics-dialog').open)return;
  const deviceName=(input,id)=>(devices || []).find(d=>d.input===input && (id?d.id===id:d.is_default))?.name || (id?'Selected device unavailable':'No system default available');
  $('voice-diagnostics-room').textContent=snapshot?.room?.name || 'No active voice session';$('diagnostic-level').value=Math.max(0,Math.min(1,data.level || 0));const facts=$('voice-diagnostics-facts');facts.replaceChildren();
  for(const [name,value] of [['Microphone',deviceName(true,audio.input_device)],['Output',deviceName(false,audio.output_device)],['Transmission',!snapshot?'Stopped':snapshot.muted?'Muted':audio.push_to_talk?(snapshot.speaking?'Push-to-talk · transmitting':'Push-to-talk · hold to transmit'):'Open microphone'],['Microphone volume',`${Math.round((audio.input_volume || 0)*100)}%`],['Listening volume',`${Math.round((audio.output_volume || 0)*100)}%`],['Processing',[audio.noise_suppression?'Noise suppression':null,audio.echo_cancellation?'Echo cancellation':null,audio.automatic_gain?'Automatic gain':null].filter(Boolean).join(' · ') || 'Off']]){const row=el('div','');row.append(el('dt','',name),el('dd','',value));facts.append(row);}
  const peers=(snapshot?.peers || []).filter(p=>p.voice),list=$('voice-diagnostics-peers');list.replaceChildren();for(const peer of peers){const route=peer.route==='direct'?'Direct connection':String(peer.route || '').includes('relay')?'Relayed connection':peer.route || 'Connected';list.append(el('p','',`${peer.name} · ${route} · ${Math.round(peer.latency_ms || 0)} ms${peer.latency_ms>250?' · High latency':''}`));}
  window.OchatWorkspace?.diagnostics(data);
  $('voice-diagnostics-advice').textContent=!snapshot?'Join a voice room or accept a call first.':deviceName(true,audio.input_device).includes('unavailable')||deviceName(true,audio.input_device)==='No system default available'?'Your selected microphone is unavailable. Choose a microphone in Audio settings.':deviceName(false,audio.output_device).includes('unavailable')||deviceName(false,audio.output_device)==='No system default available'?'Your selected output is unavailable. Choose speakers or headphones in Audio settings.':snapshot.muted?'Your microphone is muted. Unmute in the voice dock.':!peers.length?'No other participant has joined voice.':audio.output_volume===0?'Listening volume is zero. Increase it in Audio settings.':audio.input_volume===0?'Microphone volume is zero. Increase it in Audio settings.':audio.push_to_talk?'Hold your push-to-talk button to transmit. Check the local microphone test if others cannot hear you.':'If you cannot hear someone, check the selected output and system volume. Use the local microphone test in Audio settings to check capture.';
}
let diagnosticDevices=[];
async function refreshDiagnosticDevices(){const generation=sessionGeneration,devices=await invoke('audio_devices');if(unlocked&&generation===sessionGeneration&&$('voice-diagnostics-dialog').open)diagnosticDevices=devices || [];}
$('voice-diagnostics-button').onclick=()=>action($('voice-diagnostics-button'),async()=>{show('voice-diagnostics-dialog');await refreshDiagnosticDevices();await refreshDiagnostics();clearInterval(diagnosticPoll);diagnosticPoll=setInterval(()=>refreshDiagnostics().catch(toast),1500);});
$('diagnostics-refresh-devices').onclick=()=>action($('diagnostics-refresh-devices'),async()=>{await refreshDiagnosticDevices();await refreshDiagnostics();});
$('dock-audio-settings').onclick=()=>action($('dock-audio-settings'),async()=>{await openSettings();document.querySelector('[data-settings-tab="audio"]')?.click();});
$('voice-diagnostics-dialog').addEventListener('close',()=>{clearInterval(diagnosticPoll);diagnosticPoll=null;diagnosticDevices=[];$('diagnostic-level').value=0;});
$('diagnostics-settings').onclick=()=>{$('voice-diagnostics-dialog').close();openSettings().then(()=>document.querySelector('[data-settings-tab="audio"]')?.click()).catch(toast);};
function clearMessageTools(){
  clearInterval(diagnosticPoll);diagnosticPoll=null;diagnosticDevices=[];toolUpdates.clear();deliveryStates.clear();verificationCache.clear();activityEntries=[];activityBusy=false;toolMessage=null;collectionEntries=[];collectionRoom=null;verificationTarget=null;
  for(const id of ['message-tools-dialog','edit-message-dialog','reaction-dialog','collection-dialog','activity-dialog','verification-dialog','voice-diagnostics-dialog'])$(id).close();
  for(const id of ['message-tools-actions','message-tools-context','collection-results','activity-results','verification-fingerprint','verification-name','verification-status','voice-diagnostics-facts','voice-diagnostics-peers','voice-diagnostics-room','voice-diagnostics-advice','reaction-custom'])$(id).replaceChildren();
  $('edit-message-text').value='';$('collection-query').value='';$('peer-verification-status').textContent='';$('activity-button').textContent='Activity';$('activity-button').disabled=true;
}
setInterval(()=>{if(!unlocked)return;if([...deliveryStates].some(([key,value])=>key.startsWith(selectedRoom+':')&&value.status==='sending'))refreshDeliveries(selectedRoom).catch(()=>{});},3000);
setInterval(()=>{if(unlocked){refreshActivity().catch(()=>{});if(!$('navigation-friends').hidden)refreshFriends().catch(()=>{});}},15000);

window.OchatPresentation?.connect({
  canInteract:()=>unlocked && !hasBlockingDialog(),
  conversations:()=>{setNavigationSection('rooms');openNavigation();$('navigation-rooms-button').focus({preventScroll:true});},
  search:()=>{if(!$('search-button').disabled)$('search-button').click();},
  settings:()=>$('settings-button').click(),
  composer:()=>{if(!$('message').disabled)$('message').focus();},
  lock:()=>$('lock-button').click(),
  dismiss:dialog=>{if(['navigation-dialog','details-dialog','room-actions-dialog'].includes(dialog.id))closePanel(dialog.id);else dialog.close();},
  messageMenu:(row,event)=>{if(row._toolEvent)openMessageTools(row._toolEvent,{point:event&&(event.clientX||event.clientY)?{x:event.clientX,y:event.clientY}:undefined});}
});

window.OchatDiscord?.init();

window.OchatFeatures?.init({invoke,toast,show,action,context:()=>({unlocked,generation:sessionGeneration,voice:activeVoice(),room:selectedRoom,channel:selectedChannel(),settings,state,roomTitle:id=>conversationContext(id).title}),refreshSettings:next=>{settings=next;},openVolume:peer=>peerDetails(peer,activeVoice()?.room.id,peer)});

window.OchatChannels?.init({invoke,toast,show,confirm,releaseTalk,selectText:selectTextChannel,context:()=>({unlocked,generation:sessionGeneration,room:selectedRoom,tor_enabled:!!settings?.tor_enabled,voice:activeVoice(),state:snapshotFor(selectedRoom)}),refresh:async()=>{const generation=sessionGeneration;const snapshots=await invoke('room_snapshots');if(!unlocked || generation!==sessionGeneration)return;for(const snapshot of snapshots){if(snapshot.room){roomSnapshots.set(snapshot.room.id,snapshot);if(snapshot.room.id===selectedRoom)state=snapshot;}}render();},profile:peer=>showPeer(peer.id,peer.name,selectedRoom),avatar:(node,peer)=>profileAvatar(node,peer.name,peer.avatar_hash,peer.id)});

window.OchatGifPicker?.init({invoke,toast,show,context:()=>({unlocked,generation:sessionGeneration,room:selectedRoom,channel:selectedChannel()})});
window.OchatPersonalization?.init({invoke,toast,show,profileAvatar,insert:insertEmoji,emojis:unicodeEmojis,context:()=>({unlocked,generation:sessionGeneration,state,settings,room:selectedRoom,channel:selectedChannel()}),refresh:async()=>{const generation=sessionGeneration,next=await invoke('snapshot');if(unlocked&&generation===sessionGeneration){state=next;render();}},privateUnread:()=>allRooms().filter(r=>r.capacity===2).reduce((sum,r)=>sum+(unread.get(r.id)||0),0),commonRooms:peer=>allRooms().filter(r=>r.capacity!==2 && (snapshotFor(r.id)?.members || roomMembers.get(r.id) || []).some(m=>m.id===peer)).map(r=>r.name)});

function fillSearchChannels(){const select=$('search-channel');select.replaceChildren();const all=el('option','','All channels');all.value='';select.append(all);for(const channel of channelContextFor()?.channels || []){if(channel.kind!=='text')continue;const option=el('option','','# '+channel.name);option.value=channel.id;select.append(option);}select.disabled=select.options.length<=1;}
function setupAudioPreferences(){
  const panel=document.querySelector('[data-settings-panel="audio"]');const section=el('div','audio-processing-options');
  const label=el('label','','Noise reduction');label.htmlFor='noise-profile';const profile=el('select');profile.id='noise-profile';for(const [value,text]of [['off','Off'],['balanced','Balanced'],['strong','Strong']]){const option=el('option','',text);option.value=value;profile.append(option);}profile.value='balanced';
  const sensitivity=el('label','checkbox-row'),automatic=el('input');automatic.id='automatic-sensitivity';automatic.type='checkbox';automatic.checked=true;sensitivity.append(automatic,el('span','','Automatic voice sensitivity'));
  const thresholdLabel=el('label','','Voice threshold (RMS)'),threshold=el('input');threshold.id='voice-threshold';threshold.type='number';threshold.min='0.0001';threshold.max='0.25';threshold.step='0.0001';threshold.value='0.02';thresholdLabel.htmlFor=threshold.id;
  const senseLabel=el('label','','Manual sensitivity'),sense=el('input'),senseValue=el('output');sense.id='voice-sensitivity';sense.type='range';sense.min='0';sense.max='100';sense.step='1';senseLabel.htmlFor=sense.id;senseValue.id='voice-sensitivity-label';sense.oninput=()=>{threshold.value=(10**(-1-Number(sense.value)*3/100)).toFixed(5);updateVoiceSensitivity();};threshold.oninput=updateVoiceSensitivity;const detection=el('p','field-hint','Waiting for speech');detection.id='microphone-detection';detection.setAttribute('role','status');threshold.disabled=true;automatic.onchange=updateVoiceSensitivity;$('noise-suppression').closest('label').hidden=true;section.append(label,profile,sensitivity,senseLabel,sense,senseValue,thresholdLabel,threshold,el('p','field-hint','Higher sensitivity detects quieter speech. Lower thresholds may also pick up more background noise. Balanced noise reduction is recommended for quiet speech.'),detection);panel.querySelector('meter')?.before(section);$('test-description').textContent='Records five seconds locally, then plays Original followed by Processed audio. Nothing is sent or saved.';
  const search=el('select');search.id='search-channel';search.setAttribute('aria-label','Search channel');$('search-scope').after(search);search.onchange=()=>{if($('search-query').value.trim())runSearch();};
}
function updateVoiceSensitivity(){const threshold=$('voice-threshold'),sense=$('voice-sensitivity');if(!sense)return;const value=Math.max(.0001,Math.min(.25,Number(threshold.value)||.02));sense.value=String(Math.max(0,Math.min(100,(-Math.log10(value)-1)/3*100)));sense.disabled=threshold.disabled=$('automatic-sensitivity').checked;$('voice-sensitivity-label').textContent=sense.disabled?'Automatic · adapts to quiet speech':`${Math.round(Number(sense.value))}% sensitivity · ${Math.round(20*Math.log10(value))} dBFS`;}
setupAudioPreferences();

function setupNotificationTargets(){
  const label=el('label','','Apply to');label.htmlFor='notification-target';const select=el('select');select.id='notification-target';for(const [value,text]of [['room','Entire conversation'],['channel','Current text channel']]){const option=el('option','',text);option.value=value;select.append(option);}
  const title=$('room-notifications-name');title.after(label,select);
  select.onchange=()=>{const parent=$('room-notifications-dialog').dataset.parentRoom,channel=selectedChannel(parent);notificationRoom=select.value==='channel'?channel:parent;const preference=settings.room_notifications?.[notificationRoom] || {};$('room-notifications-name').textContent=conversationContext(parent).title+(select.value==='channel'?' / #'+(channelContextFor(parent)?.channels?.find(c=>c.id===channel)?.name || 'Channel'):'');$('room-notification-scope').value=preference.scope || 'inherit';for(const [id,key]of [['sound','sound'],['preview','previews'],['ring','ringtone']])$('room-notification-'+id).value=preference[key]==null?'inherit':preference[key]?'on':'off';};
}
setupNotificationTargets();

window.OchatProfiles?.init({closed:()=>{inspectedPeer=null;renderPeerCard();},toast,emojis:()=>window.OchatPersonalization.orderedEmojis(unicodeEmojis),remember:value=>window.OchatPersonalization.rememberEmoji(value),avatar:(node,person)=>profileAvatar(node,person.name||'You',person.avatar_hash,person.id)});
window.OchatCommunity?.init();
window.OchatEmoji?.init();

window.OchatPhotoCrop?.init({invoke,toast,context:()=>({unlocked,generation:sessionGeneration})});
