//! The named FreeSO-4c6b3e8 baseline HIT profile. Source stubs are explicit;
//! malformed programs fault and clean up rather than keeping disposed events.
use std::{collections::BTreeMap, sync::Arc};
use wonderland_render_core::AssetKey;
use crate::{AudioError, Result};
use crate::mixer::{MixerIntent, VoiceId, VolumeGroup};

#[derive(Clone, Debug)]
pub struct HitLimits { pub instructions_per_tick:usize, pub call_depth:usize, pub threads:usize, pub notes_per_thread:usize, pub queued_plays:usize, pub program_bytes:usize, pub catalog_entries:usize, pub hitlist_entries:usize }
impl Default for HitLimits { fn default()->Self { Self { instructions_per_tick:4096,call_depth:64,threads:128,notes_per_thread:256,queued_plays:512,program_bytes:4*1024*1024,catalog_entries:65536,hitlist_entries:65536 } } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpcodeStatus { Operative, LiteralNoOp, ConsumingStub, InertDuck, BrokenWait }
#[derive(Clone, Copy, Debug)]
pub struct OpcodeInfo {pub opcode:u8,pub name:&'static str,pub operands:Option<u8>,pub status:OpcodeStatus}
pub const OPCODES:[OpcodeInfo;97]=[
    OpcodeInfo { opcode:0x00, name:"NOP", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x01, name:"Note", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x02, name:"NoteOn", operands:Some(1), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x03, name:"NoteOff", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x04, name:"LoadB", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x05, name:"LoadL", operands:Some(5), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x06, name:"Set", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x07, name:"Call", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x08, name:"Return", operands:Some(0), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x09, name:"Wait", operands:Some(1), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x0a, name:"CallEntryPoint", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x0b, name:"WaitSamp", operands:Some(0), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x0c, name:"End", operands:Some(0), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x0d, name:"Jump", operands:None, status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x0e, name:"Test", operands:Some(1), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x0f, name:"NOP", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x10, name:"Add", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x11, name:"Sub", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x12, name:"Div", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x13, name:"Mul", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x14, name:"Cmp", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x15, name:"Less", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x16, name:"Greater", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x17, name:"Not", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x18, name:"Rand", operands:Some(3), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x19, name:"Abs", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x1a, name:"Limit", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x1b, name:"Error", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x1c, name:"Assert", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x1d, name:"AddToGroup", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x1e, name:"RemoveFromGroup", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x1f, name:"GetVar", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x20, name:"Loop", operands:Some(0), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x21, name:"SetLoop", operands:Some(0), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x22, name:"Callback", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x23, name:"SmartAdd", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x24, name:"SmartRemove", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x25, name:"SmartRemoveAll", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x26, name:"SmartSetCrit", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x27, name:"SmartChoose", operands:Some(1), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x28, name:"And", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x29, name:"NAnd", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x2a, name:"Or", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x2b, name:"NOr", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x2c, name:"XOr", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x2d, name:"Max", operands:Some(5), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x2e, name:"Min", operands:Some(5), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x2f, name:"Inc", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x30, name:"Dec", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x31, name:"PrintReg", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x32, name:"PlayTrack", operands:Some(1), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x33, name:"KillTrack", operands:Some(1), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x34, name:"Push", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x35, name:"PushMask", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x36, name:"PushVars", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x37, name:"CallMask", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x38, name:"CallPush", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x39, name:"Pop", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x3a, name:"Test1", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x3b, name:"Test2", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x3c, name:"Test3", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x3d, name:"Test4", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x3e, name:"IfEqual", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x3f, name:"IfNotEqual", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x40, name:"IfGreater", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x41, name:"IfLess", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x42, name:"IfGreatOrEq", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x43, name:"IfLessOrEq", operands:Some(4), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x44, name:"SmartSetList", operands:Some(1), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x45, name:"SeqGroupKill", operands:Some(1), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x46, name:"SeqGroupWait", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x47, name:"SeqGroupReturn", operands:Some(1), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x48, name:"GetSrcDataField", operands:Some(3), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x49, name:"SeqGroupTrackID", operands:Some(2), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x4a, name:"SetLL", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x4b, name:"SetLT", operands:Some(2), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x4c, name:"SetTL", operands:Some(2), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x4d, name:"WaitEqual", operands:Some(2), status:OpcodeStatus::BrokenWait },
    OpcodeInfo { opcode:0x4e, name:"WaitNotEqual", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x4f, name:"WaitGreater", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x50, name:"WaitLess", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x51, name:"WaitGreatOrEq", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x52, name:"WaitLessOrEq", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x53, name:"Duck", operands:Some(0), status:OpcodeStatus::InertDuck },
    OpcodeInfo { opcode:0x54, name:"Unduck", operands:Some(0), status:OpcodeStatus::InertDuck },
    OpcodeInfo { opcode:0x55, name:"TestX", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x56, name:"SetLG", operands:Some(5), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x57, name:"SetGL", operands:Some(5), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x58, name:"Throw", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x59, name:"SetSrcDataField", operands:Some(3), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x5a, name:"StopTrack", operands:Some(1), status:OpcodeStatus::ConsumingStub },
    OpcodeInfo { opcode:0x5b, name:"SetChanReg", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x5c, name:"PlayNote", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x5d, name:"StopNote", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x5e, name:"KillNote", operands:Some(0), status:OpcodeStatus::LiteralNoOp },
    OpcodeInfo { opcode:0x5f, name:"SmartIndex", operands:Some(2), status:OpcodeStatus::Operative },
    OpcodeInfo { opcode:0x60, name:"NoteOnLoop", operands:Some(1), status:OpcodeStatus::Operative },
];
pub fn opcode_census()->Vec<OpcodeInfo> { OPCODES.to_vec() }
#[derive(Clone, Debug, Default)]
pub enum PcMode { #[default] Tso, Ts1(BTreeMap<u32,u32>) }
#[derive(Clone, Debug)]
pub struct HitProgram {pub bytes:Vec<u8>,pub mode:PcMode}
impl HitProgram {
    /// `bytes` is the complete HIT resource; entrypoint PCs remain absolute.
    pub fn new(bytes:Vec<u8>,limits:&HitLimits)->Result<Self>{
        if bytes.is_empty() {return Err(AudioError::Invalid("empty HIT program"));}
        if bytes.len()>limits.program_bytes || bytes.len()>u32::MAX as usize {return Err(AudioError::Limit("HIT program bytes"));}
        Ok(Self{bytes,mode:PcMode::Tso})
    }
    fn translate(&self,id:u32)->u32 {match &self.mode {PcMode::Tso=>id,PcMode::Ts1(map)=>*map.get(&id).unwrap_or(&0)}}
}
#[derive(Clone, Debug)]
pub struct SampleRef {pub key:AssetKey,pub group:VolumeGroup,pub sample_rate:u32,pub frames:u64}
#[derive(Clone, Debug)]
pub struct Track {pub track_id:u32,pub sound_id:u32,pub hitlist_id:Option<u32>,pub looped:Option<bool>}
#[derive(Clone, Debug,Default)]
pub struct HitCatalog {pub tracks:BTreeMap<u32,Track>,pub backups:BTreeMap<u32,u32>,pub hitlists:BTreeMap<u32,Vec<u32>>,pub samples:BTreeMap<u32,SampleRef>}
#[derive(Clone, Debug)]
pub struct AudioRng { state:u64 }
impl AudioRng {
    pub const PROFILE:&'static str="splitmix64-rejection-v1";
    pub fn new(seed:u64)->Self {Self{state:seed}}
    pub fn next_u64(&mut self)->u64 {
        self.state=self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut x=self.state;x=(x^(x>>30)).wrapping_mul(0xbf58476d1ce4e5b9);x=(x^(x>>27)).wrapping_mul(0x94d049bb133111eb);x^(x>>31)
    }
    pub fn below(&mut self,upper:usize)->Result<usize> {
        if upper==0 {return Err(AudioError::Invalid("empty random range"));}
        let bound=upper as u64;let threshold=bound.wrapping_neg()%bound;
        for _ in 0..64 {let x=self.next_u64();if x>=threshold{return Ok((x%bound) as usize);}}
        Err(AudioError::Limit("random rejection budget"))
    }
}
#[derive(Debug)]
pub struct HitHost {
    pub globals:[i32;36],pub limits:HitLimits,pub catalog:Arc<HitCatalog>,pub masters:[f32;4],
    pub rng:AudioRng,generation:u64,next_voice:u64,pending:Vec<MixerIntent>,pub(crate) intents:Vec<MixerIntent>,
    // Preserve source cached-track first random choice without mutating imported IR.
    selected_sound:BTreeMap<u32,u32>,
}
impl HitHost {
    pub fn new(catalog:Arc<HitCatalog>,limits:HitLimits,seed:u64,generation:u64)->Result<Self>{
        if limits.instructions_per_tick==0 || limits.call_depth==0 || limits.threads==0 || limits.notes_per_thread==0 || limits.queued_plays==0 || generation==0 {return Err(AudioError::Invalid("HIT limits or generation"));}
        if catalog.tracks.len()+catalog.backups.len()+catalog.hitlists.len()+catalog.samples.len()>limits.catalog_entries {return Err(AudioError::Limit("catalog entries"));}
        for list in catalog.hitlists.values() {if list.len()>limits.hitlist_entries {return Err(AudioError::Limit("hitlist entries"));}}
        for sample in catalog.samples.values() {if sample.sample_rate==0 || sample.frames==0 {return Err(AudioError::Invalid("sample duration"));}}
        Ok(Self{globals:[0;36],limits,catalog,masters:[1.0;4],rng:AudioRng::new(seed),generation,next_voice:0,pending:vec![],intents:vec![],selected_sound:BTreeMap::new()})
    }
    /// Drain once after all source-ordered threads tick. Controls/cleanup precede
    /// queued starts. A stopped queued voice is removed before it can start.
    pub fn drain_intents(&mut self)->Vec<MixerIntent>{self.intents.append(&mut self.pending);std::mem::take(&mut self.intents)}
    pub(crate) fn start(&mut self,sample:&SampleRef,gain:f32,pan:f32,looped:bool)->Result<VoiceId>{
        if self.pending.len()>=self.limits.queued_plays {return Err(AudioError::Limit("queued plays"));}
        self.next_voice=self.next_voice.checked_add(1).ok_or(AudioError::Limit("voice serial"))?;
        let voice=VoiceId{generation:self.generation,serial:self.next_voice};
        self.pending.push(MixerIntent::Start{voice,sample:sample.key,group:sample.group,gain,pan,looped,seek_frame:0});Ok(voice)
    }
    pub(crate) fn stop_release(&mut self,voice:VoiceId){
        self.pending.retain(|i| !matches!(i,MixerIntent::Start{voice:v,..} if *v==voice));
        self.intents.push(MixerIntent::Stop{voice});self.intents.push(MixerIntent::Release{voice});
    }
    pub fn has_track(&self,id:u32)->bool{self.track(id,0).is_some()}
    fn track(&self,id:u32,fallback:u32)->Option<(u32,Track)>{
        for primary in [Some(id),if fallback!=0{Some(fallback)}else{None}].into_iter().flatten(){if let Some(t)=self.catalog.tracks.get(&primary){return Some((primary,t.clone()));}}
        for backup in [Some(id),if fallback!=0{Some(fallback)}else{None}].into_iter().flatten(){if let Some(primary)=self.catalog.backups.get(&backup){if let Some(t)=self.catalog.tracks.get(primary){return Some((*primary,t.clone()));}}}None
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadStep {Halted,ExecutionEnded,Dead}
#[derive(Debug)]
struct Note {voice:VoiceId,active:bool,released:bool,start_tick:u64,end_tick:Option<u64>,duration_ticks:f64}
#[derive(Debug)]
pub struct HitThread {
    pub pc:u32,pub wait_remain:i32,pub thread_dead:bool,pub dead:bool,pub looped:bool,pub has_set_loop:bool,pub loop_pointer:i64,pub zero:bool,pub sign:bool,pub patch:Option<u32>,pub objects:Vec<i32>,pub paused:bool,
    program:Arc<HitProgram>,registers:[i32;16],locals:[i32;54],stack:Vec<u32>,hitlist:Option<u32>,notes:Vec<Note>,
    pub tick_number:u64,pub gain:f32,pub pan:f32,pub group:VolumeGroup,simple_pending:bool,simple:bool,
}
impl HitThread {
    pub fn new(program:Arc<HitProgram>,pc:u32,limits:&HitLimits)->Result<Self>{
        if program.bytes.is_empty() || program.bytes.len()>limits.program_bytes || pc as usize>=program.bytes.len(){return Err(AudioError::Invalid("HIT entrypoint"));}
        let mut registers=[0;16];registers[1]=12;
        Ok(Self{pc,wait_remain:-1,thread_dead:false,dead:false,looped:false,has_set_loop:false,loop_pointer:i64::from(pc),zero:false,sign:false,patch:None,objects:vec![0;29],paused:false,program,registers,locals:[0;54],stack:vec![],hitlist:None,notes:vec![],tick_number:0,gain:1.0,pan:0.0,group:VolumeGroup::Fx,simple_pending:false,simple:false})
    }
    pub fn simple(track:u32,host:&mut HitHost)->Result<Self>{
        let mut thread=Self::new(Arc::new(HitProgram::new(vec![8],&host.limits)?),0,&host.limits)?;
        thread.set_track(track,0,host)?;thread.simple=true;thread.simple_pending=true;Ok(thread)
    }
    pub fn tick(&mut self,host:&mut HitHost)->Result<ThreadStep>{
        if self.dead{return Ok(ThreadStep::Dead);}
        let result=self.tick_inner(host);
        if result.is_err(){self.dispose(host);}
        result
    }
    fn tick_inner(&mut self,host:&mut HitHost)->Result<ThreadStep>{
        if self.paused{return Ok(ThreadStep::Halted);}
        self.tick_number=self.tick_number.checked_add(1).ok_or(AudioError::Limit("thread ticks"))?;
        // Deliberate hardening: interruption expiry is independent of VolumeSet.
        for note in &mut self.notes {if note.active && note.end_tick.map_or(false,|end|self.tick_number>end){note.active=false;if !note.released {host.stop_release(note.voice);note.released=true;}}}
        if self.simple {
            if self.simple_pending{self.note_on(host,false)?;self.simple_pending=false;}
            if self.last_active(){return Ok(ThreadStep::Halted);}self.dispose(host);return Ok(ThreadStep::Dead);
        }
        if self.thread_dead{if self.last_active(){return Ok(ThreadStep::Halted);}self.dispose(host);return Ok(ThreadStep::Dead);}
        for _ in 0..host.limits.instructions_per_tick {
            let opcode=self.byte()?;
            let info=OPCODES.get(usize::from(opcode)).ok_or(AudioError::Unsupported("unknown HIT opcode"))?;
            match info.status {
                OpcodeStatus::LiteralNoOp|OpcodeStatus::InertDuck=>continue,
                OpcodeStatus::ConsumingStub=>{for _ in 0..info.operands.unwrap_or(0){self.byte()?;}continue;}
                OpcodeStatus::BrokenWait=>{let dest=self.byte()?;self.byte()?;self.read_var(i32::from(dest),host)?;continue;}
                OpcodeStatus::Operative=>{}
            }
            match opcode {
                0x02|0x60=>{let dest=self.byte()?;let value=self.note_on(host,opcode==0x60)?;self.write_var(i32::from(dest),value,host)?;if opcode==0x60{return Ok(ThreadStep::Halted);}}
                0x04|0x05=>{let dest=self.byte()?;let value=if opcode==4{i32::from(self.byte()? as i8)}else{self.int()?};self.store_flags(dest,value,host)?;}
                0x06|0x4a=>{let dest=self.byte()?;let src=self.byte()?;let value=self.read_var(i32::from(src),host)?;self.store_flags(dest,value,host)?;}
                0x07=>{let target=self.uint()?;if self.stack.len()>=host.limits.call_depth{return Err(AudioError::Limit("call depth"));}self.stack.push(self.pc);self.goto(self.program.translate(target))?;}
                0x08=>{self.thread_dead=true;return Ok(ThreadStep::ExecutionEnded);}
                0x09=>{let src=self.byte()?;if self.wait_remain==-1{self.wait_remain=self.read_var(i32::from(src),host)?;}self.wait_remain=self.wait_remain.wrapping_sub(16);if self.wait_remain>0{self.pc-=2;return Ok(ThreadStep::Halted);}self.wait_remain=-1;}
                0x0b=>{if self.last_active(){self.pc-=1;}return Ok(ThreadStep::Halted);}
                0x0c=>{if let Some(pc)=self.stack.pop(){self.goto(pc)?;}else if self.looped&&self.has_set_loop{self.goto_loop()?;}else{self.thread_dead=true;return Ok(ThreadStep::ExecutionEnded);}}
                0x0d=>{let b=self.byte()?;if b>15{self.pc-=1;let target=self.uint()?;self.goto(self.program.translate(target))?;}else{let target=self.read_var(i32::from(b),host)? as u32;self.goto(self.program.translate(target))?;if self.byte()?==0{let next=self.pc.checked_add(2).ok_or(AudioError::Fault("jump PC overflow"))?;self.goto(next)?;}else{self.pc-=1;}}}
                0x0e=>{let src=self.byte()?;self.flags(self.read_var(i32::from(src),host)?);}
                0x10..=0x14=>{let dest=self.byte()?;let src=self.byte()?;let a=self.read_var(i32::from(dest),host)?;let b=self.read_var(i32::from(src),host)?;let value=match opcode{0x10=>a.wrapping_add(b),0x11|0x14=>a.wrapping_sub(b),0x12=>a.checked_div(b).ok_or(AudioError::Fault("division"))?,0x13=>a.wrapping_mul(b),_=>unreachable!()};if opcode!=0x14{self.write_var(i32::from(dest),value,host)?;}self.flags(value);}
                0x18=>{let dest=self.byte()?;let low=self.byte()?;let high=self.byte()?;if high<low{return Err(AudioError::Invalid("random bounds"));}let value=host.rng.below(usize::from(high-low)+1)? as i32+i32::from(low);self.store_flags(dest,value,host)?;}
                0x20=>self.goto_loop()?,
                0x21=>{self.loop_pointer=i64::from(self.pc);self.has_set_loop=true;}
                0x27=>{let dest=self.byte()?;let value=self.choose(host)? as i32;self.write_var(i32::from(dest),value,host)?;}
                0x2d|0x2e=>{let dest=self.byte()?;let value=self.int()?;let old=self.read_var(i32::from(dest),host)?;self.store_flags(dest,if opcode==0x2d{old.max(value)}else{old.min(value)},host)?;}
                0x3e..=0x43=>{let target=self.uint()?;let take=match opcode{0x3e=>self.zero,0x3f=>!self.zero,0x40=>!self.sign&&!self.zero,0x41=>self.sign,0x42=>!self.sign,0x43=>self.sign||self.zero,_=>unreachable!()};if take{self.goto(self.program.translate(target))?;}}
                0x44=>{let src=self.byte()?;let id=self.read_var(i32::from(src),host)? as u32;self.hitlist=if host.catalog.hitlists.contains_key(&id){Some(id)}else{None};}
                0x45=>{if self.byte()?==0{self.kill_vocals(host);}}
                0x48=>{let dest=self.byte()?;let src=self.byte()?;let field=self.byte()?;self.read_var(i32::from(src),host)?;let address=10010i32.checked_add(self.read_var(i32::from(field),host)?).ok_or(AudioError::Fault("object field overflow"))?;let value=self.read_var(address,host)?;self.store_flags(dest,value,host)?;}
                0x56|0x57=>{let local=self.byte()?;let global=self.int()?;let index=usize::try_from(global).ok().filter(|&i|i<36).ok_or(AudioError::Fault("global index"))?;if opcode==0x56{host.globals[index]=i32::from(local);}else{self.write_var(i32::from(local),host.globals[index],host)?;}}
                0x5f=>{let dest=self.byte()?;let src=self.byte()?;let index=usize::try_from(self.read_var(i32::from(src),host)?).map_err(|_|AudioError::Fault("hitlist index"))?;let id=self.list_entry(index,host)?;self.set_track(id,0,host)?;let returned=self.list_entry(index,host)?;self.write_var(i32::from(dest),returned as i32,host)?;}
                _=>return Err(AudioError::Unsupported("unhandled operative HIT opcode")),
            }
        }Err(AudioError::Limit("instructions per tick"))
    }
    fn byte(&mut self)->Result<u8>{let byte=*self.program.bytes.get(self.pc as usize).ok_or(AudioError::Truncated(u64::from(self.pc)))?;self.pc=self.pc.checked_add(1).ok_or(AudioError::Fault("PC overflow"))?;Ok(byte)}
    fn uint(&mut self)->Result<u32>{Ok(u32::from_le_bytes([self.byte()?,self.byte()?,self.byte()?,self.byte()?]))}
    fn int(&mut self)->Result<i32>{Ok(self.uint()? as i32)}
    fn goto(&mut self,pc:u32)->Result<()>{if pc as usize>=self.program.bytes.len(){return Err(AudioError::Fault("HIT PC"));}self.pc=pc;Ok(())}
    fn goto_loop(&mut self)->Result<()>{self.goto(u32::try_from(self.loop_pointer).map_err(|_|AudioError::Fault("loop pointer"))?)}
    fn flags(&mut self,value:i32){self.zero=value==0;self.sign=value<0;}
    fn store_flags(&mut self,dest:u8,value:i32,host:&mut HitHost)->Result<()>{self.write_var(i32::from(dest),value,host)?;self.flags(value);Ok(())}
    pub fn read_var(&self,address:i32,host:&HitHost)->Result<i32>{
        match address {0..=15=>Ok(self.registers[address as usize]),16..=69=>Ok(self.locals[(address-16) as usize]),100..=135=>Ok(host.globals[(address-100) as usize]),10010..=10038=>self.objects.get((address-10010) as usize).copied().ok_or(AudioError::Fault("object field")),x if x<0=>Err(AudioError::Fault("negative variable")),_=>Ok(0)}
    }
    pub fn write_var(&mut self,address:i32,value:i32,host:&mut HitHost)->Result<()>{
        match address {0..=15=>self.registers[address as usize]=value,16..=69=>{if address==0x12||address==0x32{self.patch=Some(value as u32);}self.locals[(address-16) as usize]=value;},100..=135=>host.globals[(address-100) as usize]=value,10010..=10038=>*self.objects.get_mut((address-10010) as usize).ok_or(AudioError::Fault("object field"))?=value,x if x<0=>return Err(AudioError::Fault("negative variable")),_=>{}}Ok(())
    }
    pub fn set_track(&mut self,id:u32,fallback:u32,host:&mut HitHost)->Result<()>{
        if let Some((primary,track))=host.track(id,fallback){
            if let Some(list)=track.hitlist_id.filter(|&id|id!=0){self.hitlist=if host.catalog.hitlists.contains_key(&list){Some(list)}else{None};}
            let sound=if track.sound_id==0 && track.hitlist_id.unwrap_or(0)!=0 {
                if let Some(sound)=host.selected_sound.get(&primary){*sound}else{let sound=self.choose(host)?;host.selected_sound.insert(primary,sound);sound}
            }else{track.sound_id};self.patch=Some(sound);
            if let Some(looped)=track.looped{self.looped=looped;self.has_set_loop=looped;}
        }else{self.patch=Some(id);}Ok(())
    }
    fn choose(&mut self,host:&mut HitHost)->Result<u32>{
        if let Some(id)=self.hitlist{let list=host.catalog.hitlists.get(&id).ok_or(AudioError::Missing("hitlist"))?;let index=host.rng.below(list.len())?;Ok(list[index])}else{Ok(0)}
    }
    fn list_entry(&self,index:usize,host:&HitHost)->Result<u32>{self.hitlist.and_then(|id|host.catalog.hitlists.get(&id)).and_then(|list|list.get(index)).copied().ok_or(AudioError::Fault("hitlist index"))}
    fn note_on(&mut self,host:&mut HitHost,looped:bool)->Result<i32>{
        let sample=match self.patch.and_then(|p|host.catalog.samples.get(&p)){Some(sample)=>sample.clone(),None=>return Ok(-1)};
        if self.notes.len()>=host.limits.notes_per_thread{return Err(AudioError::Limit("notes per thread"));}
        self.group=sample.group;let gain=(self.gain*host.masters[self.group as usize]).clamp(0.0,1.0);
        let voice=host.start(&sample,gain,self.pan,looped)?;
        self.notes.push(Note{voice,active:true,released:false,start_tick:self.tick_number,end_tick:None,duration_ticks:sample.frames as f64/f64::from(sample.sample_rate)*60.0});Ok((self.notes.len()-1) as i32)
    }
    fn last_active(&self)->bool{self.notes.last().map_or(false,|n|n.active)}
    pub fn complete_voice(&mut self,id:VoiceId)->bool{if let Some(n)=self.notes.iter_mut().find(|n|n.voice==id){let was=n.active;n.active=false;was}else{false}}
    pub fn last_voice(&self)->Option<VoiceId>{self.notes.last().map(|n|n.voice)}
    pub fn active_notes(&self)->usize{self.notes.iter().filter(|n|n.active).count()}
    pub fn kill_vocals(&mut self,host:&mut HitHost){for n in &mut self.notes{n.active=false;if !n.released{host.stop_release(n.voice);n.released=true;}}}
    pub fn dispose(&mut self,host:&mut HitHost){if !self.dead{self.kill_vocals(host);self.dead=true;}}
    pub fn interruptable(&self)->bool{self.locals[0x21]>0 || self.locals[0x27]>0}
    pub fn interrupt(&mut self){for n in &mut self.notes{if n.end_tick.is_none(){let periods=(self.tick_number.saturating_sub(n.start_tick) as f64/n.duration_ticks).ceil();n.end_tick=Some(n.start_tick.saturating_add((periods*n.duration_ticks) as u64));}}}
    pub fn set_gain_pan(&mut self,gain:f32,pan:f32,host:&mut HitHost)->Result<()>{
        crate::pcm::validate_gain_pan(gain,pan)?;self.gain=gain;self.pan=pan;
        for n in &self.notes{if n.active{host.intents.push(MixerIntent::SetGainPan{voice:n.voice,gain:(gain*host.masters[self.group as usize]).clamp(0.0,1.0),pan});}}
        Ok(())
    }
    pub fn pause(&mut self,host:&mut HitHost){if !self.paused{self.paused=true;for n in &self.notes{if n.active{host.intents.push(MixerIntent::Pause{voice:n.voice});}}}}
    pub fn resume(&mut self,host:&mut HitHost){if self.paused{self.paused=false;for n in &self.notes{if n.active{host.intents.push(MixerIntent::Resume{voice:n.voice});}}}}
}
