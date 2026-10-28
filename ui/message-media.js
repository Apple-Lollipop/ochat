/* Image references remain scoped to their conversation and account generation. */
(() => {
  let api,epoch=0;
  const pending=new Map();
  const imageReference=event=>!event?.deleted&&/^ochat-image:([a-f0-9]{64})$/.exec(event?.text||'');
  function thumbnail(container,event){
    const match=imageReference(event);if(!api||!match)return false;
    const context=api.context(),token=epoch;if(!context.unlocked)return false;
    const image=document.createElement('img');image.className='reply-image-preview';image.alt='Image from '+(event.name||'this message');image.decoding='async';container.append(image);
    const key=[context.generation,event.room||context.room,event.channel||'',event.peer,match[1]].join(':');let request=pending.get(key);if(!request){request=api.invoke('message_image',{room:event.room||context.room,channel:event.channel??null,peer:event.peer,hash:match[1]}).finally(()=>{if(pending.get(key)===request)pending.delete(key);});pending.set(key,request);}
    request.then(data=>{
      const current=api.context();if(!current.unlocked||current.generation!==context.generation||token!==epoch||!image.isConnected)return;
      if(typeof data==='string'&&/^data:image\/(png|jpeg|gif|webp);base64,/.test(data))image.src=data;
    }).catch(()=>{if(token===epoch&&image.isConnected)image.alt='Image unavailable';});return true;
  }
  function renderThread(container,event,scope={}){return window.OchatPersonalization?.image(container,{...event,room:event.room||scope.room,channel:event.channel??scope.channel??null})||false;}
  window.OchatMessageMedia=Object.freeze({init(callbacks){api=callbacks;},thumbnail,renderThread,reset(){epoch++;pending.clear();}});
})();
