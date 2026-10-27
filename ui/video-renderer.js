/* Bounded decoding and recovery shared by the conversation and detached viewer. */
(() => {
  function create(container, options={}) {
    const canvas=document.createElement('canvas'),image=document.createElement('img'),status=document.createElement('div');
    canvas.className='video-canvas';image.className='video-image';image.alt='Live shared video';status.className='video-status';container.append(canvas,image,status);
    let decoder=null,session=null,lastSequence=null,needsKey=true,disposed=false,profile=null,lastRequest=-Infinity;
    function closeDecoder(){const previous=decoder;decoder=null;needsKey=true;try{previous?.close();}catch{}}
    function clear(message='Waiting for video…'){closeDecoder();session=null;lastSequence=null;image.removeAttribute('src');canvas.width=canvas.height=1;canvas.hidden=image.hidden=true;status.hidden=false;status.textContent=message;}
    function recover(event,message='Resuming video…',failed=false){
      // Keep a healthy decoder alive while it starts or drains its queue.
      // A fresh keyframe can reset prediction without cancelling pending output.
      if(failed)closeDecoder();else needsKey=true;status.textContent=message;status.hidden=false;
      if(performance.now()-lastRequest>=400){lastRequest=performance.now();options.requestKeyframe?.(event);}
    }
    function packet(event){
      if(disposed)return;
      if(event.type==='visual_frame'){
        if(!/^data:image\/jpeg;base64,[A-Za-z0-9+/=]+$/.test(event.data_url)||event.data_url.length>131100)return;
        if(session!==event.session){clear();session=event.session;}
        image.src=event.data_url;image.hidden=false;canvas.hidden=true;status.hidden=true;return;
      }
      const control=event.control,p=control?.profile;
      if(!p||!['jpeg','vp8'].includes(p.codec)||p.width<1||p.height<1||p.width>1920||p.height>1080||typeof event.data!=='string'||event.data.length>699052)return;
      if(session!==control.session){clear();session=control.session;profile=p;}
      if(p.codec==='jpeg'){image.src='data:image/jpeg;base64,'+event.data;image.hidden=false;canvas.hidden=true;status.hidden=true;return;}
      if(!window.VideoDecoder||!window.EncodedVideoChunk){status.textContent='VP8 decoding is unavailable. Ask the sender to use Compatibility mode.';return;}
      // Independent streams may finish out of order. Never move backwards.
      if(lastSequence!==null&&event.sequence<=lastSequence)return;
      if(lastSequence!==null&&event.sequence!==lastSequence+1&&!event.key)recover(event);
      lastSequence=event.sequence;
      if(needsKey&&!event.key){if(!decoder)recover(event);return;}
      try{
        if(!decoder){
          const identity=session;
          const instance=new VideoDecoder({output(frame){try{if(!disposed&&session===identity&&decoder===instance){canvas.width=frame.displayWidth;canvas.height=frame.displayHeight;canvas.getContext('2d').drawImage(frame,0,0);canvas.hidden=false;image.hidden=true;status.hidden=true;}}finally{frame.close();}},error(){if(decoder===instance)recover(event,'Resuming video…',true);}});
          decoder=instance;decoder.configure({codec:'vp8',codedWidth:profile.width,codedHeight:profile.height,optimizeForLatency:true});
        }
        if(decoder.decodeQueueSize>8){recover(event);return;}
        const binary=atob(event.data),bytes=Uint8Array.from(binary,c=>c.charCodeAt(0));
        decoder.decode(new EncodedVideoChunk({type:event.key?'key':'delta',timestamp:event.timestamp,data:bytes}));needsKey=false;
      }catch{recover(event,'Resuming video…',true);}
    }
    clear();return {packet,clear,dispose(){disposed=true;clear('Sharing ended.');container.replaceChildren();}};
  }
  window.OchatVideoRenderer=Object.freeze({create});
})();
