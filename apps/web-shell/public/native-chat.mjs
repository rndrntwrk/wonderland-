// Public native lot chat. No simulation receipts, VM events or private channels.
const decimal=v=>typeof v==='string'&&/^(0|[1-9]\d{0,19})$/.test(v)&&BigInt(v)<=18446744073709551615n;
const textValid=v=>typeof v==='string'&&v.trim().length>0&&v.length<=200
  &&!/[\u0000-\u0008\u000b-\u001f\u007f]/.test(v)&&v.isWellFormed();
function message(v){
  if(!v||!decimal(v.id)||v.id==='0'||!decimal(v.sender_uid)||v.sender_uid==='0'
    ||!decimal(v.sender_sequence)||v.sender_sequence==='0'||typeof v.sender_name!=='string'
    ||!v.sender_name||v.sender_name.length>100||v.channel_id!==0||v.private!==false
    ||!Number.isSafeInteger(v.created_at_ms)||v.created_at_ms<0||!textValid(v.text))throw Error('Invalid lot chat message');
  return {id:v.id,sender_uid:v.sender_uid,sender_name:v.sender_name,sender_sequence:v.sender_sequence,
    channel_id:0,private:false,text:v.text,created_at_ms:v.created_at_ms};
}
export function createNativeLotChat(request,publish){
  let dead=false,live=false,busy=false,ready=false,unknown=false,generation=0,controller=null;
  let scope=null,sequence='0',messages=[],pending=null,notice='Connect to the lot to chat.',delivered=0;
  const emit=()=>{if(!dead)publish({ready:live&&ready,busy,pending:!!pending,unknown,messages:structuredClone(messages),notice,delivered});};
  function accept(value){
    if(value?.version!==1||value.state!=='lot_chat'||!decimal(value.viewer_sequence)
      ||![value.viewer_id,value.lot_id,value.lot_incarnation].every(v=>decimal(v)&&v!=='0')
      ||!Array.isArray(value.messages)||value.messages.length>100)throw Error('Invalid lot chat response');
    const identity=[value.viewer_id,value.lot_id,value.lot_incarnation].join(':');
    if(scope!==null&&scope!==identity)throw Error('Lot chat scope changed');
    if(BigInt(value.viewer_sequence)<BigInt(sequence))throw Error('Lot chat receipt regressed');
    const rows=value.messages.map(message);
    if(rows.some((v,i)=>i>0&&BigInt(v.id)<=BigInt(rows[i-1].id)))throw Error('Invalid lot chat order');
    const own=value.viewer_receipt===null?null:message(value.viewer_receipt);
    if((value.viewer_sequence==='0')!==(own===null)||own&&(own.sender_uid!==value.viewer_id||own.sender_sequence!==value.viewer_sequence))
      throw Error('Invalid lot chat owner receipt');
    const ack=value.receipt===null?null:message(value.receipt);
    if(ack&&(!pending||ack.sender_uid!==value.viewer_id||ack.sender_sequence!==pending.sequence||ack.text!==pending.text))
      throw Error('Uncorrelated lot chat acknowledgement');
    scope=identity;sequence=value.viewer_sequence;messages=rows;ready=true;
    if(pending&&own?.sender_sequence===pending.sequence&&own.text===pending.text){pending=null;unknown=false;delivered++;}
    notice=pending?'Previous send is unconfirmed. Retry its exact message.':'Messages are shared with current members of this lot.';
  }
  async function perform(action){
    if(dead||!live||busy)return false;
    const version=generation;busy=true;const ownController=new AbortController();controller=ownController;
    const timeout=setTimeout(()=>ownController.abort(),15000);emit();
    try{
      const value=await request(action,action==='send'?structuredClone(pending):{},ownController.signal);
      if(dead||!live||version!==generation)return false;
      accept(value);return true;
    }catch{
      if(!dead&&version===generation){ready=false;messages=[];unknown=!!pending;
        notice=pending?'Send not confirmed. Reconnect or retry the same message.':'Chat is unavailable. Refresh after reconnecting.';}
      return false;
    }finally{clearTimeout(timeout);if(!dead&&version===generation){busy=false;controller=null;emit();}}
  }
  return {
    refresh:()=>perform('read'),
    async send(text){
      if(dead||!live||!ready||busy||pending||!textValid(text))return false;
      if(BigInt(sequence)===18446744073709551615n)return false;
      pending={sequence:String(BigInt(sequence)+1n),text};unknown=false;return perform('send');
    },
    retry:()=>pending?perform('send'):Promise.resolve(false),
    setLive(value){if(dead||live===value)return;live=value;ready=false;
      if(!value){generation++;controller?.abort();controller=null;busy=false;messages=[];unknown=!!pending;notice='Reconnect to the lot to continue chatting.';}emit();},
    dispose(){if(dead)return;generation++;controller?.abort();messages=[];pending=null;dead=true;publish=()=>{};request=null;},
  };
}

export function mountNativeLotChat(element,request){
  const doc=element.ownerDocument,node=(tag,cls,text)=>{const n=doc.createElement(tag);if(cls)n.className=cls;if(text)n.textContent=text;return n;};
  const panel=node('section','native-chat-panel chrome');panel.setAttribute('aria-label','Lot chat');
  const heading=node('h2','','Lot chat'),log=node('div','connected-chat-log');log.setAttribute('role','log');log.setAttribute('aria-label','Lot messages');log.setAttribute('aria-live','polite');
  const form=node('form','connected-compose connected-chat-compose'),label=node('label','','Say something to the lot');
  const input=node('textarea');input.id='native-chat-message';input.maxLength=200;input.rows=2;input.placeholder='Message the lot…';label.htmlFor=input.id;
  const send=node('button','chrome primary','Send');send.type='submit';
  const notice=node('p','native-chat-notice');notice.setAttribute('role','status');
  const refresh=node('button','chrome','Refresh chat');refresh.type='button';
  const retry=node('button','chrome primary','Retry same message');retry.type='button';
  form.append(label,input,send);panel.append(heading,log,form,notice,refresh,retry);element.replaceChildren(panel);
  let disposed=false,live=false,timer=null,pollEpoch=0,lastDelivered=0,lastRows='',atBottom=true;
  log.addEventListener('scroll',()=>{atBottom=log.scrollHeight-log.clientHeight-log.scrollTop<=2;});
  const client=createNativeLotChat(request,state=>{
    if(disposed)return;
    input.disabled=!state.ready||state.pending||state.busy;send.disabled=input.disabled||!textValid(input.value);
    refresh.disabled=!live||state.busy;retry.hidden=!state.unknown;retry.disabled=!live||state.busy;
    notice.textContent=state.notice;
    if(state.delivered!==lastDelivered){lastDelivered=state.delivered;input.value='';send.disabled=true;}
    const key=JSON.stringify(state.messages);
    if(key!==lastRows){lastRows=key;const top=log.scrollTop;log.replaceChildren();
      for(const m of state.messages){const item=node('div','connected-message'),who=node('small','',m.sender_name),body=node('p','',m.text);
        item.dataset.chatId=m.id;item.append(who,body);log.append(item);}
      if(!state.messages.length)log.append(node('p','connected-empty','No messages yet.'));
      log.scrollTop=atBottom?log.scrollHeight:top;
    }
  });
  async function poll(epoch){if(disposed||!live||epoch!==pollEpoch)return;await client.refresh();
    if(!disposed&&live&&epoch===pollEpoch)timer=setTimeout(()=>poll(epoch),1000);}
  form.addEventListener('submit',event=>{event.preventDefault();void client.send(input.value);});
  input.addEventListener('input',()=>{send.disabled=input.disabled||!textValid(input.value);});
  refresh.addEventListener('click',()=>void client.refresh());retry.addEventListener('click',()=>void client.retry());
  return {setLive(value){if(disposed||value===live)return;live=value;pollEpoch++;clearTimeout(timer);client.setLive(value);if(value)void poll(pollEpoch);},
    dispose(){disposed=true;live=false;pollEpoch++;clearTimeout(timer);client.dispose();element.replaceChildren();}};
}
