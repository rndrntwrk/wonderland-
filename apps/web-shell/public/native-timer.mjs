// Original UITimerPluginEOD controls over the owned private EOD boundary.
// A UI receipt confirms the control request, never simulation completion.
const PLUGIN=0xaa65fe9e;
const dec=v=>typeof v==='string'&&/^[1-9]\d{0,19}$/.test(v)&&BigInt(v)<=18446744073709551615n;
const digest=v=>typeof v==='string'&&/^[0-9a-f]{64}$/.test(v);
const bytes=v=>{if(typeof v!=='string'||v.length>8192||!/^(?:[0-9a-f]{2})*$/.test(v))throw Error('Invalid private bytes');return Uint8Array.from(v.match(/../g)??[],h=>parseInt(h,16));};
const hex=v=>Array.from(v,b=>b.toString(16).padStart(2,'0')).join('');
const hash=async text=>hex(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text))));
const canonical=v=>JSON.stringify(Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])));
function input(event,binary,payload){
  if(typeof binary!=='boolean'||typeof event!=='string')throw Error('Invalid Timer control type');
  const b=bytes(payload);
  if(!((event==='Timer_Set'&&binary&&b.length===2&&b[0]<=99&&b[1]<=59)
    ||(['Timer_State_Change','Timer_IsRunning_Change'].includes(event)&&binary&&b.length===1&&b[0]<=1)
    ||(event==='Timer_Close'&&!binary&&b.length===0)))throw Error('Invalid Timer control');
  return b;
}
async function output(raw){
  if(!raw||raw.plugin_id!==PLUGIN||!dec(raw.source_session_id)||!dec(raw.output_id)||!digest(raw.sha256)
    ||typeof raw.binary!=='boolean')throw Error('Invalid private Timer identity');
  const b=bytes(raw.payload_hex);
  if(!((raw.event==='Timer_Show'&&raw.binary&&b.length===4)
    ||(raw.event==='Timer_Off'&&raw.binary&&b.length===1&&b[0]<=1)
    ||(['eod_enter','eod_leave'].includes(raw.event)&&!raw.binary&&b.length===0)
    ||(raw.event==='Timer_Update'&&!raw.binary)))throw Error('Unsupported private Timer output');
  const proof={source_session_id:raw.source_session_id,output_id:raw.output_id,event:raw.event,binary:raw.binary,payload_hex:raw.payload_hex};
  if(await hash(canonical(proof))!==raw.sha256)throw Error('Private Timer digest mismatch');
  let time=null;
  if(raw.event==='Timer_Update'){
    const text=new TextDecoder('utf-8',{fatal:true}).decode(b);
    if(!/^-?\d+:-?\d+$/.test(text))throw Error('Invalid original Timer update');
    time=text.split(':').map(v=>{const n=Number(v);return Number.isInteger(n)&&n>=0&&n<=255?n:0;});
  }
  return {...raw,bytes:b,time};
}
export function createNativeTimer(request,publish){
  let live=false,dead=false,busy=false,ready=false,opened=false,unknown=false,closing=false,generation=0,controller=null;
  let session=null,next='1',scope=null,pending=null,mode=0,running=false,minutes=0,seconds=0;
  let notice='Waiting for an original object dialog.';const seen=new Map();
  const emit=()=>{if(!dead)publish({visible:live&&(opened||unknown),ready:live&&ready&&opened&&!closing,busy,unknown,pending:!!pending,
    mode,running,minutes,seconds,closing,notice,source_session_id:session});};
  const applyControl=task=>{const m=task.message,b=bytes(m.payload_hex);
    if(m.event==='Timer_Set'){[minutes,seconds]=b;}
    if(m.event==='Timer_IsRunning_Change')running=b[0]===1;
    if(m.event==='Timer_State_Change')mode=b[0];
    if(m.event==='Timer_Close')closing=true;
    pending=null;unknown=false;notice=closing?'Close accepted; waiting for source exit.':'Control accepted by the server.';
  };
  async function accept(value,version,signal){
    if(value?.version!==1||value.state!=='native_eod'||value.type!=='native_eod_outputs'
      ||![value.viewer_id,value.lot_id,value.lot_incarnation].every(dec)
      ||!Array.isArray(value.source_sessions)||value.source_sessions.length>64
      ||!Array.isArray(value.messages)||value.messages.length>4096)throw Error('Invalid private EOD response');
    const identity=[value.viewer_id,value.lot_id,value.lot_incarnation].join(':');
    if(scope!==null&&scope!==identity)throw Error('Private Timer scope changed');
    const sessions=new Map();
    for(const row of value.source_sessions){
      if(!dec(row.source_session_id)||sessions.has(row.source_session_id)||row.plugin_id!==PLUGIN
        ||typeof row.closed!=='boolean'||!dec(row.next_ui_sequence))throw Error('Invalid private Timer session');
      const last=row.last_ui_receipt;
      if((row.next_ui_sequence==='1')!==(last===null)||last&&(!dec(last.sequence)||!digest(last.sha256)
        ||BigInt(last.sequence)+1n!==BigInt(row.next_ui_sequence)))throw Error('Invalid Timer UI receipt cursor');
      sessions.set(row.source_session_id,row);
    }
    const active=[...sessions.values()].filter(s=>!s.closed);
    if(active.length>1)throw Error('Ambiguous private Timer ownership');
    const row=active[0]??sessions.get(session)??null;
    const decoded=[];for(const m of value.messages){const item=await output(m);
      if(!sessions.has(item.source_session_id))throw Error('Private output has no bound session');decoded.push(item);}
    if(dead||!live||version!==generation)return [];
    scope=identity;
    if(row&&row.source_session_id!==session){session=row.source_session_id;opened=false;closing=false;}
    if(row){next=row.next_ui_sequence;
      const accepted=value.accepted_ui;
      if(pending&&(accepted&&canonical(accepted)===canonical(pending.message)
        ||row.last_ui_receipt?.sequence===pending?.message.sequence&&row.last_ui_receipt?.sha256===pending?.sha256)){
        if(pending.message.source_session_id===session)applyControl(pending);
        else {pending=null;unknown=false;notice='Previous Timer control confirmed; current session is unchanged.';}
      }
      if(row.closed){opened=false;closing=false;}
    }else{opened=false;closing=false;}
    const feedback=[];
    for(const m of decoded){
      const key=m.source_session_id+':'+m.output_id;
      if(seen.has(key)&&seen.get(key)!==m.sha256)throw Error('Private output identity reused');
      if(!seen.has(key)){
        if(seen.size>=4096)throw Error('Private Timer presentation capacity');
        if(m.source_session_id===session){
          if(m.event==='Timer_Show'){running=m.bytes[0]===1;mode=m.bytes[1]===1?1:0;minutes=m.bytes[2]<100?m.bytes[2]:0;seconds=m.bytes[3]<60?m.bytes[3]:0;opened=!row.closed;closing=false;}
          if(m.event==='Timer_Off'&&m.bytes[0]===0){running=false;mode=0;}
          if(m.event==='Timer_Update'){[minutes,seconds]=m.time;
            // Original UpdateTimeHandler calls UpdateTime, including Timer_Set.
            if(minutes<=99&&seconds<=59)feedback.push(['Timer_Set',true,hex([minutes,seconds])]);}
          if(m.event==='eod_leave'){opened=false;closing=false;}
        }
        ready=!!row&&!row.closed;emit(); // Apply to the owned UI before acknowledging delivery.
        seen.set(key,m.sha256);
      }
      await request({type:'native_eod_ack',source_session_id:m.source_session_id,output_id:m.output_id,sha256:m.sha256},signal);
      if(dead||!live||version!==generation)return [];
    }
    ready=!!row&&!row.closed;return feedback;
  }
  async function perform(mutation=false){
    if(dead||!live||busy||mutation&&!pending)return false;
    busy=true;const version=generation,own=new AbortController();controller=own;
    const timer=setTimeout(()=>own.abort(),15000);emit();let feedback=[],ok=false;
    try{
      const q=mutation?{type:'native_eod_ui',message:structuredClone(pending.message)}:{type:'native_eod_read'};
      const v=await request(q,own.signal);feedback=await accept(v,version,own.signal);
      if(dead||version!==generation)return false;
      if(mutation&&pending)throw Error('Timer control unconfirmed');
      ok=true;return true;
    }catch{
      if(!dead&&version===generation){ready=false;unknown=!!pending;notice=pending?'Control unconfirmed. Retry the identical request.':'Object dialog unavailable. Reconnect or refresh.';}return false;
    }finally{
      clearTimeout(timer);
      if(!dead&&version===generation){busy=false;controller=null;emit();
        if(ok&&!pending){for(const args of feedback)await api.send(...args);}}
    }
  }
  const api={refresh:()=>perform(),retry:()=>pending?perform(true):Promise.resolve(false),
    async send(event,binary,payload){
      if(dead||!live||!ready||!opened||closing||busy||pending||!session)return false;
      try{input(event,binary,payload);}catch{return false;}
      const task={message:{schema_version:1,source_session_id:session,sequence:next,event,binary,payload_hex:payload},sha256:null};
      pending=task;unknown=false;emit();
      task.sha256=await hash(JSON.stringify(task.message));
      if(dead||pending!==task)return false;return perform(true);
    },
    setLive(value){if(dead||value===live)return;live=value;
      if(!value){generation++;controller?.abort();controller=null;busy=false;ready=false;unknown=!!pending;}emit();},
    dispose(){if(dead)return;dead=true;generation++;controller?.abort();seen.clear();pending=null;session=null;request=null;publish=()=>{};},
  };return api;
}

export function mountNativeTimer(element,request,onShow=()=>{}){
  const doc=element.ownerDocument,node=(tag,cls,text)=>{const n=doc.createElement(tag);if(cls)n.className=cls;if(text)n.textContent=text;return n;};
  const panel=node('section','native-timer-panel chrome');panel.hidden=true;panel.setAttribute('role','dialog');panel.setAttribute('aria-label','Timer');
  const title=node('h2','','Timer'),clock=node('output','native-timer-clock');clock.setAttribute('aria-label','Timer time');
  const body=node('div','native-timer-controls'),modes=node('div','native-timer-row');
  const countdown=node('button','chrome','Countdown'),stopwatch=node('button','chrome','Stopwatch');modes.append(countdown,stopwatch);
  const fields=node('fieldset'),legend=node('legend','','Set time'),minLabel=node('label','','Minutes'),secLabel=node('label','','Seconds');
  const min=node('input'),sec=node('input');for(const input of [min,sec]){input.type='number';input.min='0';input.step='1';input.inputMode='numeric';}
  min.max='99';sec.max='59';minLabel.append(min);secLabel.append(sec);
  const set=node('button','chrome','Set time');fields.append(legend,minLabel,secLabel,set);
  const actions=node('div','native-timer-row'),play=node('button','chrome primary','Start timer'),pause=node('button','chrome','Pause timer');actions.append(play,pause);
  const close=node('button','chrome','Close timer');body.append(modes,fields,actions,close);
  const notice=node('p','native-timer-notice');notice.setAttribute('role','status');
  const retry=node('button','chrome primary','Retry same control'),refresh=node('button','chrome','Refresh timer');
  for(const b of [countdown,stopwatch,set,play,pause,close,retry,refresh])b.type='button';
  panel.append(title,clock,body,notice,retry,refresh);element.replaceChildren(panel);
  let disposed=false,live=false,pollEpoch=0,timer=null,wasVisible=false,dirty=false,latest;
  const validTime=()=>/^\d{1,2}$/.test(min.value)&&/^\d{1,2}$/.test(sec.value)&&Number(min.value)<=99&&Number(sec.value)<=59;
  const controls=()=>{if(!latest)return;const blocked=!latest.ready||latest.busy||latest.pending;
    fields.disabled=blocked||latest.running;set.disabled=fields.disabled||!validTime();
    play.disabled=blocked||latest.running||dirty&&!validTime();pause.disabled=blocked||!latest.running;
    countdown.disabled=blocked||latest.running||latest.mode===0;stopwatch.disabled=blocked||latest.running||latest.mode===1;
    close.disabled=blocked;retry.hidden=!latest.unknown;retry.disabled=!live||latest.busy;refresh.disabled=!live||latest.busy;
  };
  const client=createNativeTimer(request,v=>{
    if(disposed)return;latest=v;panel.hidden=!v.visible;panel.dataset.sourceSession=v.source_session_id??'';
    body.hidden=!v.ready;clock.hidden=!v.ready||v.running;
    if(v.visible&&v.ready&&!wasVisible){dirty=false;onShow();}wasVisible=v.visible&&v.ready;
    clock.textContent=String(v.minutes).padStart(2,'0')+':'+String(v.seconds).padStart(2,'0');
    if(!dirty){min.value=String(v.minutes);sec.value=String(v.seconds);}
    countdown.setAttribute('aria-pressed',String(v.mode===0));stopwatch.setAttribute('aria-pressed',String(v.mode===1));
    notice.textContent=v.notice;controls();
  });
  for(const input of [min,sec])input.addEventListener('input',()=>{dirty=true;controls();});
  async function setTime(){if(!validTime())return false;const ok=await client.send('Timer_Set',true,hex([Number(min.value),Number(sec.value)]));if(ok)dirty=false;return ok;}
  set.addEventListener('click',()=>void setTime());
  play.addEventListener('click',async()=>{if(dirty&&!await setTime())return;await client.send('Timer_IsRunning_Change',true,'01');});
  pause.addEventListener('click',()=>void client.send('Timer_IsRunning_Change',true,'00'));
  countdown.addEventListener('click',()=>void client.send('Timer_State_Change',true,'00'));
  stopwatch.addEventListener('click',()=>void client.send('Timer_State_Change',true,'01'));
  close.addEventListener('click',()=>void client.send('Timer_Close',false,''));
  retry.addEventListener('click',()=>void client.retry());refresh.addEventListener('click',()=>void client.refresh());
  async function poll(epoch){if(disposed||!live||epoch!==pollEpoch)return;await client.refresh();
    if(!disposed&&live&&epoch===pollEpoch)timer=setTimeout(()=>poll(epoch),1000);}
  return {setLive(value){if(disposed||value===live)return;live=value;pollEpoch++;clearTimeout(timer);client.setLive(value);if(value)void poll(pollEpoch);},
    dispose(){disposed=true;live=false;pollEpoch++;clearTimeout(timer);client.dispose();element.replaceChildren();}};
}
