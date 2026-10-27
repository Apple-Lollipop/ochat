/* Presentation for friends, privacy and shared controls. Network state is supplied by the app. */
(() => {
  const $=id=>document.getElementById(id);
  const node=(tag,className,text)=>{const n=document.createElement(tag);n.className=className||'';if(text!==undefined)n.textContent=text;return n;};
  let filter='friends',chosen=false,counts={accepted:0,incoming:0,outgoing:0},privacy={enabled:false,ready:false};
  function onion(){const icon=node('span','onion-glyph');icon.setAttribute('aria-hidden','true');return icon;}
  function badge(parent,person){
    let mark=[...parent.children].find(n=>n.classList.contains('peer-tor'));
    if(!person?.online || person.tor_enabled!==true){mark?.remove();return;}
    if(!mark){mark=node('span','peer-tor');mark.append(onion());mark.setAttribute('role','img');parent.append(mark);}
    const label=person.route==='self'?'Tor enabled on this device':'Tor enabled · reported by this peer';
    mark.title=label;mark.setAttribute('aria-label',label);
  }
  function filterFriends(){
    const query=$('friends-search')?.value.trim().toLocaleLowerCase()||'';let visible=0;
    for(const [id,kind]of [['accepted-friend-list','friends'],['received-friend-list','requests'],['sent-friend-list','requests']]){
      const list=$(id);if(!list)continue;let matches=0;
      for(const row of list.querySelectorAll('.friend-row')){row.hidden=!!query&&!row.dataset.search?.includes(query);if(!row.hidden)matches++;}
      list.parentElement.hidden=kind!==filter||matches===0;if(kind===filter)visible+=matches;
    }
    $('friends-empty').hidden=visible!==0;
    $('friends-empty').textContent=query?'No matching people.':filter==='requests'?'No pending requests. New invitations will appear here.':'Add a friend with their contact ID to start a private chat.';
    for(const b of document.querySelectorAll('[data-friends-filter]'))b.setAttribute('aria-pressed',String(b.dataset.friendsFilter===filter));
  }
  function friends(next){
    counts=next;if(!$('friends-search'))return;
    if(!chosen&&(counts.accepted||counts.incoming||counts.outgoing)){filter=counts.accepted?'friends':'requests';chosen=true;}
    $('friends-all-count').textContent=String(counts.accepted);$('friends-requests-count').textContent=String(counts.incoming+counts.outgoing);
    for(const row of $('friend-list').querySelectorAll('.friend-row')){
      const controls=row.querySelector('.friend-controls'),remove=controls?.querySelector('.danger'),identity=row.querySelector('.friend-identity');
      if(remove&&!row.querySelector('.friend-more')){
        const menu=node('details','friend-more'),summary=node('summary','','•••');summary.setAttribute('aria-label','More options for '+row.querySelector('strong').textContent);
        const content=node('div','friend-more-content');if(identity)content.append(identity);content.append(remove);menu.append(summary,content);controls.append(menu);
      }
    }
    filterFriends();
  }
  function setupFriends(){
    const toolbar=node('div','friends-toolbar'),tabs=node('div','friends-filters');tabs.setAttribute('role','group');tabs.setAttribute('aria-label','Friends view');
    for(const [value,text,id]of [['friends','Your friends','friends-all-count'],['requests','Requests','friends-requests-count']]){
      const b=node('button','secondary',text),count=node('span','friends-count','0');count.id=id;b.type='button';b.dataset.friendsFilter=value;b.append(count);b.onclick=()=>{filter=value;chosen=true;filterFriends();};tabs.append(b);
    }
    const search=node('input');search.type='search';search.id='friends-search';search.placeholder='Find a friend';search.setAttribute('aria-label','Find a friend by name or identity');search.oninput=filterFriends;
    toolbar.append(tabs,search);$('friend-list').before(toolbar);const empty=node('p','friends-empty');empty.id='friends-empty';empty.setAttribute('role','status');$('friend-list').after(empty);
    const hint=node('p','friends-intro','Add someone with their contact ID, or copy yours to share it. Requests stay here until accepted.');$('navigation-friends').prepend(hint);
    document.addEventListener('click',event=>{for(const menu of document.querySelectorAll('.friend-more[open]'))if(!menu.contains(event.target))menu.open=false;});
    document.addEventListener('keydown',event=>{if(event.key==='Escape'){const menu=document.querySelector('.friend-more[open]');if(menu){event.preventDefault();menu.open=false;menu.querySelector('summary').focus();}}});
    friends(counts);
  }
  function setupSearch(){
    const scope=$('search-scope'),channel=$('search-channel'),filters=node('div','search-filters');
    const scopeLabel=document.querySelector('label[for="search-scope"]');scope.before(filters);scopeLabel.remove();
    for(const [select,text]of [[scope,'Conversation'],[channel,'Text channel']]){const field=node('div','search-filter'),label=node('label','',text);label.htmlFor=select.id;field.append(label,select);filters.append(field);}
  }
  function privacyState(state){
    privacy=state;const card=$('tor-status-card');if(!card)return;
    card.dataset.state=!state.enabled?'off':state.ready?'active':'connecting';
    $('tor-status-title').textContent=!state.enabled?'Tor is off':state.ready?'Tor is active':'Connecting to Tor';
    $('tor-pending-change').hidden=$('tor-enabled').checked===state.enabled;
  }
  function setupPrivacy(){
    const field=document.querySelector('.privacy-settings'),toggle=$('tor-enabled'),route=$('tor-route-status');if(!field)return;field.setAttribute('aria-label','Tor protection');
    const hero=node('div','tor-hero'),icon=node('span','tor-hero-icon');icon.append(onion());const intro=node('div');intro.append(node('h3','','Tor protection'),node('p','','A private route for Ochat'));hero.append(icon,intro);
    const toggleRow=node('label','tor-toggle-row'),toggleCopy=node('span');toggleCopy.append(node('strong','','Use Tor'),node('small','','Route peer connections and in-app requests through Tor.'));toggleRow.append(toggleCopy,toggle);
    const status=node('div','tor-status-card');status.id='tor-status-card';const title=node('strong');title.id='tor-status-title';status.append(node('span','tor-status-dot'),title,route);
    const path=node('div','tor-path');path.setAttribute('aria-label','This device, Tor network, then the destination');for(const name of ['Your device','Tor network','Destination']){const step=node('span','tor-path-step',name);path.append(step);}
    const features=node('ul','tor-features');for(const [title,detail]of [['Built into Ochat','No separate installation.'],['No direct fallback','If Tor is unavailable, protected requests wait or fail.'],['Calls may be slower','Audio, video and screen sharing use the same private route.']]){const item=node('li');item.append(node('strong','',title),node('span','',detail));features.append(item);}
    const pending=node('p','tor-pending-change','This change applies when you save. Ochat will lock; unlock it to reconnect.');pending.id='tor-pending-change';pending.setAttribute('role','status');
    field.replaceChildren(hero,toggleRow,pending,status,path,features,node('p','field-hint','Your Ochat identity stays the same. Links opened in your browser use the browser’s connection. Connected peers can see that you report Tor enabled.'));
    toggle.addEventListener('change',()=>privacyState(privacy));privacyState(privacy);
  }
  function init(){
    setupFriends();setupSearch();setupPrivacy();
    const credit=node('p','field-hint emoji-credit','Emoji artwork: Twemoji by Twitter and contributors · CC-BY 4.0. Included locally for offline use.');$('emoji-dialog').append(credit);
    const account=document.querySelector('[data-settings-panel="account"]');account.append(credit.cloneNode(true));
  }
  function reset(){filter='friends';chosen=false;counts={accepted:0,incoming:0,outgoing:0};if($('friends-search')){$('friends-search').value='';friends(counts);}privacyState({enabled:false,ready:false});for(const mark of document.querySelectorAll('.peer-tor'))mark.remove();}
  window.OchatCommunity={init,friends,badge,privacyState,reset};
})();
