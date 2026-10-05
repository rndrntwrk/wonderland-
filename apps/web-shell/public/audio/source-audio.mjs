/** Browser source-file binding. Presentation only; never creates gameplay cues. */
import { BrowserAudio } from './browser-audio.mjs';
const GROUPS=['Music','Fx','Vox','Ambience'];
const hex=bytes=>Array.from(bytes,v=>v.toString(16).padStart(2,'0')).join('');
function exactId(value){if(typeof value==='number'&&(!Number.isSafeInteger(value)||value<1))throw Error('invalid audio identity');if(!['string','number','bigint'].includes(typeof value)||typeof value==='string'&&!/^[1-9][0-9]*$/.test(value))throw Error('invalid audio identity');const n=BigInt(value);if(n<1n||n>(1n<<64n)-1n)throw Error('invalid audio identity');return n;}
const voiceKey=v=>`${exactId(v.generation)}:${exactId(v.serial)}`;
export function waveResource(encoded,{maxPcmBytes=64*1024*1024,maxEncodedBytes=4*1024*1024}={}) {
 if(!(encoded instanceof ArrayBuffer)||encoded.byteLength<12)throw Error('invalid RIFF WAVE');
 const v=new DataView(encoded),text=(at,n)=>String.fromCharCode(...new Uint8Array(encoded,at,n));
 if(text(0,4)!=='RIFF'||text(8,4)!=='WAVE'||v.getUint32(4,true)+8!==encoded.byteLength)throw Error('invalid RIFF WAVE size');
 let format=null,data=null;
 for(let at=12;at+8<=encoded.byteLength;){const kind=text(at,4),size=v.getUint32(at+4,true),end=at+8+size;if(end>encoded.byteLength)throw Error('truncated RIFF chunk');
  if(kind==='fmt '){if(format||size<16)throw Error('invalid RIFF format');format={tag:v.getUint16(at+8,true),channels:v.getUint16(at+10,true),sampleRate:v.getUint32(at+12,true),rate:v.getUint32(at+16,true),align:v.getUint16(at+20,true),bits:v.getUint16(at+22,true)};}
  if(kind==='data'){if(data!==null)throw Error('duplicate RIFF data');data=size;}at=end+(size%2);
 }
 if(!format||data===null||format.tag!==1||![1,2].includes(format.channels)||![8,16,24,32].includes(format.bits)||format.sampleRate<1||format.sampleRate>384000||format.align!==format.channels*format.bits/8||format.rate!==format.sampleRate*format.align||data===0||data%format.align)throw Error('invalid PCM WAVE fields');
 const frames=data/format.align,decodedBytes=frames*format.channels*4;
 return {encoded,format:'wav',sampleRate:format.sampleRate,channels:format.channels,frames,decodedBytes,streaming:decodedBytes>maxPcmBytes||encoded.byteLength>maxEncodedBytes};
}
export class SourceAudioPlayer {
 constructor({backend,contextFactory,mediaFactory=()=>new Audio(),urlApi=globalThis.URL}={}) {
  this.resources=new Map();this.voices=new Map();this.streams=new Map();this.completed=[];this.volumes=Object.fromEntries(GROUPS.map(g=>[g,1]));this.muted=false;this.disposed=false;this.serial=0n;this.generation=1n;this.mediaFactory=mediaFactory;this.urlApi=urlApi;this.state='locked';this.notices=[];this.lastGeneration=null;this.lastSerial=0n;
  this.backend=backend??new BrowserAudio({contextFactory,loadSample:async(key,{signal})=>{if(signal.aborted)throw Error('audio load cancelled');const r=this.resources.get(Array.isArray(key)?hex(key):key);if(!r)throw Error('Source sample is unavailable. Load the original audio resource.');if(r.streaming)throw Error('streamed sample cannot be decoded as an effect');return r;}});
 }
 _live(){if(this.disposed)throw Error('source audio player disposed');}
 _gain(v){return v.gain*(this.muted?0:this.volumes[v.group]);}
 applyAll(items){this._live();const out=[];for(const item of items){const [kind]=Object.keys(item),data=item[kind],id=voiceKey(data.voice);
  if(kind==='Start'){const generation=exactId(data.voice.generation),serial=exactId(data.voice.serial);
   if(this.lastGeneration!==null&&(generation<this.lastGeneration||generation===this.lastGeneration&&serial<=this.lastSerial))throw Error('stale or replayed audio identity');
   if(!GROUPS.includes(data.group)||!Number.isFinite(data.gain)||data.gain<0||data.gain>1||!Number.isFinite(data.pan)||data.pan<-1||data.pan>1||typeof data.looped!=='boolean')throw Error('invalid source voice parameters');
   if(this.voices.size+this.completed.length>=128)throw Error('source voice budget exceeded');
   if(this.lastGeneration!==null&&generation>this.lastGeneration){this.backend.reset();for(const stream of [...this.streams.keys()])this._removeStream(stream);this.voices.clear();this.completed=[];}
   this.lastGeneration=generation;this.lastSerial=serial;const v={...data,voice:{generation:generation.toString(),serial:serial.toString()}};this.voices.set(id,v);const r=this.resources.get(Array.isArray(v.sample)?hex(v.sample):v.sample);
   if(r?.streaming){if(v.group!=='Music')throw Error('Long source samples support Music streaming only');this._stream(v,r);continue;}
   out.push({Start:{...v,gain:this._gain(v)}});
  } else {if(!['SetGainPan','Pause','Resume','Stop','Release'].includes(kind))throw Error('unknown source mixer intent');const v=this.voices.get(id),s=this.streams.get(id);
   if(kind==='SetGainPan'){if(!Number.isFinite(data.gain)||data.gain<0||data.gain>1||!Number.isFinite(data.pan)||data.pan<-1||data.pan>1)throw Error('invalid source gain or pan');if(v){v.gain=data.gain;v.pan=data.pan;}}
   if(s){if(kind==='SetGainPan')s.media.volume=this._gain(v);else if(kind==='Pause'){s.paused=true;s.media.pause();}else if(kind==='Resume'){s.paused=false;if(this.state==='running')this._playStream(s);}else if(kind==='Stop'||kind==='Release')this._removeStream(id);}
   else out.push(kind==='SetGainPan'&&v?{SetGainPan:{...data,gain:this._gain(v)}}:item);
   if(kind==='Stop'||kind==='Release')this.voices.delete(id);
  }
 }this.backend.applyAll(out);}
 _stream(v,r){const id=voiceKey(v.voice);if(this.streams.has(id))throw Error('replayed stream identity');const media=this.mediaFactory(),url=this.urlApi.createObjectURL(r.blob);media.src=url;media.preload='metadata';media.loop=v.looped;media.volume=this._gain(v);const s={media,url,voice:v.voice,paused:false};this.streams.set(id,s);
  media.addEventListener('loadedmetadata',()=>{if(this.streams.get(id)===s&&BigInt(v.seek_frame??0)>0n&&r.sampleRate)media.currentTime=Number(BigInt(v.seek_frame))/r.sampleRate;},{once:true});
  media.addEventListener('ended',()=>{if(this.streams.get(id)===s){this.completed.push(v.voice);this._removeStream(id);this.voices.delete(id);}});
  media.addEventListener('error',()=>{if(this.streams.get(id)===s){this.notices.push('The browser could not decode the selected source audio.');this.completed.push(v.voice);this._removeStream(id);this.voices.delete(id);}});
  if(this.state==='running')this._playStream(s);
 }
 _playStream(s){Promise.resolve(s.media.play()).catch(e=>{if(!this.disposed&&[...this.streams.values()].includes(s)){this.state='interrupted';this.notices.push(String(e.message??e));}});}
 _removeStream(id){const s=this.streams.get(id);if(!s)return;s.media.pause();s.media.removeAttribute('src');s.media.load?.();this.urlApi.revokeObjectURL(s.url);this.streams.delete(id);}
 _refresh(){const intents=[];for(const [id,v] of this.voices){const s=this.streams.get(id);if(s)s.media.volume=this._gain(v);else intents.push({SetGainPan:{voice:v.voice,gain:this._gain(v),pan:v.pan}});}this.backend.applyAll(intents);}
 setVolume(group,gain){this._live();if(!GROUPS.includes(group)||!Number.isFinite(gain)||gain<0||gain>1)throw Error('invalid audio volume');this.volumes[group]=gain;this._refresh();}
 setMuted(value){this._live();this.muted=!!value;this._refresh();}
 async unlockFromGesture(){this._live();await this.backend.unlockFromGesture();this._live();this.state='running';for(const s of this.streams.values())if(!s.paused)this._playStream(s);}
 async suspend(){this._live();for(const s of this.streams.values())s.media.pause();await this.backend.suspend();this.state='suspended';}
 takeFinished(){const result=[...this.backend.takeFinished(),...this.completed.splice(0)];for(const v of result)this.voices.delete(voiceKey(v));return result;}
 snapshot(){const b=this.backend.snapshot();return {...b,state:this.disposed?'disposed':this.state==='running'&&b.state?b.state:this.state,streamingVoices:this.streams.size,loadedSamples:this.resources.size,muted:this.muted,volumes:{...this.volumes},lastNotice:this.notices.at(-1)??null};}
 registerResource(key,resource){this._live();if(!/^[a-f0-9]{64}$/.test(key))throw Error('invalid source content key');this.resources.set(key,resource);}
 async loadFiles(files){this._live();if(files.length>512||files.reduce((n,f)=>n+f.size,0)>128*1024*1024)throw Error('Source selection exceeds this browser memory budget.');const loaded=[];
  for(const file of files){if(!/\.(mp3|wav)$/i.test(file.name))throw Error('Select original MP3 or PCM WAVE files. XA and UTK need the source decoder.');const encoded=await file.arrayBuffer();this._live();const key=hex(new Uint8Array(await crypto.subtle.digest('SHA-256',encoded)));this._live();const isMp3=/\.mp3$/i.test(file.name);const resource=isMp3?{streaming:true,format:'mp3',blob:file}:waveResource(encoded);if(resource.streaming)resource.blob=file;resource.name=file.webkitRelativePath||file.name;this.registerResource(key,resource);loaded.push({key,name:resource.name,streaming:resource.streaming});}
  return loaded;
 }
 audition(key,{looped=false}={}){this._live();this.takeFinished();if(!this.resources.has(key))throw Error('Source audio not loaded');this.stopAudition();const voice={generation:this.generation.toString(),serial:(++this.serial).toString()};this.auditionVoice=voice;this.applyAll([{Start:{voice,sample:key,group:'Music',gain:1,pan:0,looped,seek_frame:'0'}}]);}
 stopAudition(){if(this.auditionVoice){this.applyAll([{Stop:{voice:this.auditionVoice}}]);this.auditionVoice=null;}}
 async dispose(){if(this.disposed)return;this.disposed=true;for(const id of [...this.streams.keys()])this._removeStream(id);this.voices.clear();this.resources.clear();this.completed=[];await this.backend.dispose();this.state='disposed';}
}
let shared=null,accepted=null;
export function acceptedAudioHost(){return accepted??=new SourceAudioPlayer();}
export function sourceAudioHost(){return shared??=new SourceAudioPlayer();}
export function openSourceAudioControls(){
 const player=sourceAudioHost(),runtime=acceptedAudioHost();let dialog=document.getElementById('source-audio-dialog');if(dialog){dialog.showModal();return;}
 dialog=document.createElement('dialog');dialog.id='source-audio-dialog';dialog.className='source-audio-dialog';dialog.setAttribute('aria-labelledby','source-audio-title');
 dialog.innerHTML=`<form method="dialog"><button class="audio-close" aria-label="Close sound settings">×</button></form><h2 id="source-audio-title">Sound</h2><p class="audio-note">Load your original music files. Effects, voices and ambience play when their source resources are available.</p><div class="audio-actions"><button type="button" data-enable>Enable sound</button><button type="button" data-pause>Pause sound</button><label><input type="checkbox" data-mute> Mute</label></div><div class="audio-volumes">${GROUPS.map(g=>`<label>${g==='Fx'?'Effects':g==='Vox'?'Voices':g}<input type="range" min="0" max="100" value="100" data-group="${g}" aria-label="${g} volume"><output>100%</output></label>`).join('')}</div><label class="audio-file-label">Original music files<input type="file" multiple accept=".mp3,.wav" data-files></label><label>Selected music<select data-playlist aria-label="Selected music"><option value="">Choose a loaded file</option></select></label><div class="audio-actions"><button type="button" data-play>Play selected music</button><button type="button" data-stop>Stop music</button><label><input type="checkbox" data-loop> Repeat</label></div><p data-status role="status">Sound is waiting for activation.</p>`;
 const status=dialog.querySelector('[data-status]');let busy=false;
 const run=async fn=>{if(busy)return;busy=true;try{await fn();const s=player.snapshot();status.textContent=s.lastNotice||s.lastError||`Sound ${s.state}. ${s.loadedSamples} source file${s.loadedSamples===1?'':'s'} loaded.`;}catch(e){status.textContent=String(e.message??e);}finally{busy=false;}};
 dialog.querySelector('[data-enable]').onclick=()=>run(()=>Promise.all([player.unlockFromGesture(),runtime.unlockFromGesture()]));dialog.querySelector('[data-pause]').onclick=()=>run(()=>Promise.all([player.suspend(),runtime.suspend()]));
 const mute=dialog.querySelector('[data-mute]');mute.checked=player.muted;mute.onchange=()=>{player.setMuted(mute.checked);runtime.setMuted(mute.checked);};
 for(const slider of dialog.querySelectorAll('[data-group]')){slider.value=player.volumes[slider.dataset.group]*100;slider.nextElementSibling.value=`${slider.value}%`;slider.oninput=()=>{player.setVolume(slider.dataset.group,Number(slider.value)/100);runtime.setVolume(slider.dataset.group,Number(slider.value)/100);slider.nextElementSibling.value=`${slider.value}%`;};}
 const list=dialog.querySelector('[data-playlist]');dialog.querySelector('[data-files]').onchange=e=>run(async()=>{const loaded=await player.loadFiles(Array.from(e.target.files));for(const f of loaded)runtime.registerResource(f.key,player.resources.get(f.key));for(const f of loaded){const option=document.createElement('option');option.value=f.key;option.textContent=f.name;list.append(option);}if(loaded.length)list.value=loaded[0].key;e.target.value='';});
 dialog.querySelector('[data-play]').onclick=()=>run(async()=>{await player.unlockFromGesture();player.audition(list.value,{looped:dialog.querySelector('[data-loop]').checked});});dialog.querySelector('[data-stop]').onclick=()=>run(()=>player.stopAudition());
 document.body.append(dialog);dialog.showModal();
}
export async function disposeSourceAudio(){const old=shared,oldAccepted=accepted;shared=null;accepted=null;document.getElementById('source-audio-dialog')?.remove();await Promise.all([old?.dispose(),oldAccepted?.dispose()]);}
