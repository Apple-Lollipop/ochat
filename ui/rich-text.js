/* Safe message formatting. Source stays Unicode; HTML is never interpreted. */
(() => {
  const make = (tag, cls) => {const node = document.createElement(tag); if(cls) node.className=cls; return node;};
  function inline(parent, text, plain, depth=0) {
    if(depth>8){plain(parent,text);return;}
    const pattern=/(`[^`\n]+`|\*\*[^\n]+?\*\*|\|\|[^\n]+?\|\|)/g;
    let position=0;
    for(const match of text.matchAll(pattern)){
      plain(parent,text.slice(position,match.index));const value=match[0];
      if(value.startsWith('`')){const code=make('code','message-inline-code');code.textContent=value.slice(1,-1);parent.append(code);}
      else if(value.startsWith('**')){const strong=make('strong');inline(strong,value.slice(2,-2),plain,depth+1);parent.append(strong);}
      else {const spoiler=make('span','message-spoiler');const toggle=make('button','spoiler-toggle');toggle.type='button';toggle.textContent='Reveal spoiler';toggle.setAttribute('aria-expanded','false');const content=make('span','spoiler-content');content.hidden=true;inline(content,value.slice(2,-2),plain,depth+1);toggle.onclick=()=>{const open=toggle.getAttribute('aria-expanded')!=='true';toggle.setAttribute('aria-expanded',String(open));toggle.textContent=open?'Hide spoiler':'Reveal spoiler';content.hidden=!open;};spoiler.append(toggle,content);parent.append(spoiler);}
      position=match.index+value.length;
    }
    plain(parent,text.slice(position));
  }
  function render(parent,text,plain=(target,value)=>target.append(document.createTextNode(value))) {
    text=String(text??'');const lines=text.split('\n');let list=null,listType=null;
    for(let index=0;index<lines.length;index++){
      const line=lines[index];
      if(/^```[\w+-]*\s*$/.test(line)){
        list=null;listType=null;const language=line.slice(3).trim(),pre=make('pre','message-code-block'),code=make('code');if(language)code.setAttribute('aria-label',language+' code');const content=[];while(++index<lines.length&&!/^```\s*$/.test(lines[index]))content.push(lines[index]);code.textContent=content.join('\n');pre.append(code);parent.append(pre);continue;
      }
      const bullet=/^\s{0,3}([-*]|\d+[.)])\s+(.+)$/.exec(line);
      if(bullet){const type=/^\d/.test(bullet[1])?'ol':'ul';if(!list||listType!==type){list=make(type,'message-list');listType=type;parent.append(list);}const item=make('li');inline(item,bullet[2],plain);list.append(item);continue;}
      list=null;listType=null;inline(parent,line,plain);if(index<lines.length-1)parent.append(make('br'));
    }
  }
  function init(){
    const input=document.getElementById('message');if(!input||document.getElementById('rich-text-toolbar'))return;
    const toolbar=make('div','rich-text-toolbar');toolbar.id='rich-text-toolbar';toolbar.setAttribute('role','group');toolbar.setAttribute('aria-label','Message formatting');
    function insert(kind){if(input.disabled||toolbar.hidden)return;const start=input.selectionStart,end=input.selectionEnd,selection=input.value.slice(start,end);let value,offset=0,length;
      if(kind==='bold'){value='**'+(selection||'bold text')+'**';offset=2;length=(selection||'bold text').length;}
      if(kind==='spoiler'){value='||'+(selection||'spoiler')+'||';offset=2;length=(selection||'spoiler').length;}
      if(kind==='list'){value=(start>0&&input.value[start-1]!=='\n'?'\n':'')+(selection||'List item').split('\n').map(line=>'- '+line).join('\n');offset=value.startsWith('\n')?3:2;length=(selection||'List item').length;}
      if(kind==='code'){value=(start>0&&input.value[start-1]!=='\n'?'\n':'')+'```\n'+(selection||'code')+'\n```'+(end<input.value.length&&input.value[end]!=='\n'?'\n':'');offset=value.indexOf('\n',value.startsWith('\n')?1:0)+1;length=(selection||'code').length;}
      input.setRangeText(value,start,end,'end');input.focus();input.setSelectionRange(start+offset,start+offset+length);input.dispatchEvent(new Event('input',{bubbles:true}));
    }
    for(const [kind,label,title]of [['bold','B','Bold (Ctrl+B)'],['list','List','Bullet list'],['code','</>','Code block'],['spoiler','Spoiler','Hide text behind a spoiler']]){const button=make('button');button.type='button';button.textContent=label;button.title=title;button.setAttribute('aria-label',title);button.onpointerdown=event=>event.preventDefault();button.onclick=()=>insert(kind);toolbar.append(button);}
    const composer=document.getElementById('composer');composer?.before(toolbar);const sync=()=>{toolbar.hidden=input.disabled||!!composer?.hidden;for(const button of toolbar.querySelectorAll('button'))button.disabled=input.disabled;};sync();new MutationObserver(sync).observe(input,{attributes:true,attributeFilter:['disabled']});if(composer)new MutationObserver(sync).observe(composer,{attributes:true,attributeFilter:['hidden']});input.addEventListener('keydown',event=>{if((event.ctrlKey||event.metaKey)&&!event.shiftKey&&!event.altKey&&event.key.toLowerCase()==='b'){event.preventDefault();insert('bold');}});
  }
  const previewText=text=>String(text??'').replace(/```[\s\S]*?(?:```|$)/g,'').replace(/`[^`\n]+`/g,'').replace(/\|\|[^\n]+?\|\|/g,'');
  window.OchatRichText=Object.freeze({render,init,previewText});
})();
