//! Normalized B-provider boundary and source-ordered shared event scheduler.
//! This module does not parse HIT/HSM/EVT/TRK/HLS resources.
use std::{collections::BTreeMap,sync::Arc};
use wonderland_render_core::EntityRef;
use crate::{AudioError,Result,hit::*,mixer::*};
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub enum TsoGroup {NewMain,Relationships,TsoEp5,TsoV2,TsoV3,Turkey}
#[derive(Clone,Debug)]
pub struct EventRecord {pub name:String,pub event_type:u32,pub track_id:u32}
#[derive(Clone,Debug)]
pub struct ResourceGroup {pub kind:TsoGroup,pub program:Arc<HitProgram>,pub hsm:Option<Vec<(String,i32)>>,pub entrypoints:Vec<(u32,u32)>,pub events:Vec<EventRecord>}
#[derive(Clone,Debug)]
pub enum ResolvedKind {Hit {program:Arc<HitProgram>,pc:u32,track:u32,fallback:u32},Simple {track:u32},Station {track:u32},Music {mode:u32}}
#[derive(Clone,Debug)]
pub struct ResolvedEvent {pub name:String,pub kind:ResolvedKind}
#[derive(Clone,Debug)]
pub struct EventBank {pub groups:Vec<ResourceGroup>}
impl EventBank {
    pub fn new(mut groups:Vec<ResourceGroup>,limits:&HitLimits)->Result<Self>{
        if groups.len()>6{return Err(AudioError::Limit("resource groups"));}
        groups.sort_by_key(|g|g.kind);
        let mut count=0usize;
        for (index,g) in groups.iter().enumerate(){
            if index>0 && groups[index-1].kind==g.kind{return Err(AudioError::Invalid("duplicate resource group"));}
            count=count.checked_add(g.events.len()+g.entrypoints.len()+g.hsm.as_ref().map_or(0,Vec::len)).ok_or(AudioError::Limit("event metadata"))?;
            if count>limits.catalog_entries || g.program.bytes.len()>limits.program_bytes{return Err(AudioError::Limit("event metadata"));}
            for event in &g.events{validate_name(&event.name)?;}
            if let Some(constants)=&g.hsm{for (name,_) in constants{validate_name(name)?;}}
            let mut tracks=std::collections::BTreeSet::new();
            for &(track,pc) in &g.entrypoints{if pc as usize>=g.program.bytes.len() || !tracks.insert(track){return Err(AudioError::Invalid("HIT entrypoint metadata"));}}
        }
        Ok(Self{groups})
    }
    pub fn resolve(&self,name:&str,host:&HitHost)->Result<ResolvedEvent>{
        validate_name(name)?;let requested=name.to_ascii_lowercase();
        let (group,event)=self.groups.iter().find_map(|g|g.events.iter().find(|e|e.name.eq_ignore_ascii_case(&requested)).map(|e|(g,e))).ok_or(AudioError::Missing("audio event"))?;
        let effective=if requested=="piano_play"{"playpiano".to_owned()}else{requested};
        let mut track=event.track_id;
        let pc=if let Some(hsm)=&group.hsm{
            if let Some((_,id))=hsm.iter().find(|(name,_)|name.eq_ignore_ascii_case(&format!("guid_tkd_{effective}"))){track=*id as u32;}
            hsm.iter().find(|(name,_)|name.eq_ignore_ascii_case(&effective)).map_or(0,|(_,pc)|*pc as u32)
        }else{group.entrypoints.iter().find(|(id,_)|*id==event.track_id).map_or(0,|(_,pc)|*pc)};
        let kind=match event.event_type {
            30=>ResolvedKind::Station{track:event.track_id},
            36=>ResolvedKind::Music{mode:if event.track_id==0{match effective.as_str(){"bkground_buy1"=>1,"bkground_build"=>2,_=>0}}else{event.track_id}},
            _ if pc!=0=>{if pc as usize>=group.program.bytes.len(){return Err(AudioError::Invalid("event routine PC"));}ResolvedKind::Hit{program:group.program.clone(),pc,track,fallback:event.track_id}},
            _ if track!=0 && host.has_track(track)=>ResolvedKind::Simple{track},
            _=>return Err(AudioError::Missing("event routine or track")),
        };Ok(ResolvedEvent{name:effective,kind})
    }
}
fn validate_name(name:&str)->Result<()>{if name.is_empty() || name.len()>256 || !name.is_ascii() || name.bytes().any(|b|b<32){return Err(AudioError::Invalid("event or symbol name"));}Ok(())}
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub struct ThreadId(pub u64);
#[derive(Debug)]
struct SharedThread {id:ThreadId,name:String,thread:HitThread,owners:Vec<EntityRef>,ever_owned:bool,volume_set:bool,blocker:Option<ThreadId>,waiter:Option<ThreadId>}
#[derive(Debug)]
pub struct AudioRuntime {pub host:HitHost,pub bank:EventBank,sounds:Vec<SharedThread>,active:BTreeMap<String,ThreadId>,next_thread:u64,faults:Vec<(ThreadId,AudioError)>}
impl AudioRuntime {
    pub fn new(host:HitHost,bank:EventBank)->Self{Self{host,bank,sounds:vec![],active:BTreeMap::new(),next_thread:0,faults:vec![]}}
    pub fn play(&mut self,name:&str,owner:Option<EntityRef>)->Result<ThreadId>{
        validate_name(name)?;if owner.map_or(false,|o|o.generation==0){return Err(AudioError::Invalid("owner generation"));}
        let original=name.to_ascii_lowercase();
        if let Some(old)=self.active.get(&original).copied(){if let Some(sound)=self.sounds.iter_mut().find(|s|s.id==old){if !sound.thread.dead && sound.blocker.is_none() && !sound.thread.interruptable(){add_owner(sound,owner,self.host.limits.threads)?;return Ok(old);}}}
        let event=self.bank.resolve(name,&self.host)?;
        if event.name!=original{if let Some(old)=self.active.get(&event.name).copied(){if let Some(sound)=self.sounds.iter_mut().find(|s|s.id==old && !s.thread.dead){add_owner(sound,owner,self.host.limits.threads)?;return Ok(old);}}}
        let old=self.active.get(&event.name).copied();
        let old_index=old.and_then(|id|self.sounds.iter().position(|s|s.id==id));
        let blocker=old_index.and_then(|i|{let s=&self.sounds[i];s.blocker.or(if s.thread.interruptable(){Some(s.id)}else{None})});
        let replacing_waiter=old_index.filter(|&i|self.sounds[i].blocker.is_some());
        if self.sounds.len()>=self.host.limits.threads && replacing_waiter.is_none(){return Err(AudioError::Limit("audio threads"));}
        let mut thread=match event.kind {
            ResolvedKind::Hit{program,pc,track,fallback}=>{let mut t=HitThread::new(program,pc,&self.host.limits)?;if track!=0{t.set_track(track,fallback,&mut self.host)?;}t},
            ResolvedKind::Simple{track}=>HitThread::simple(track,&mut self.host)?,
            ResolvedKind::Station{..}=>return Err(AudioError::Missing("station playlist provider")),
            ResolvedKind::Music{..}=>return Err(AudioError::Missing("music playlist provider")),
        };
        if original=="piano_play"{thread.looped=true;thread.has_set_loop=true;}
        self.next_thread=self.next_thread.checked_add(1).ok_or(AudioError::Limit("thread serial"))?;let id=ThreadId(self.next_thread);
        if let Some(i)=replacing_waiter{self.remove(i);}
        let mut sound=SharedThread{id,name:event.name.clone(),thread,owners:vec![],ever_owned:false,volume_set:false,blocker,waiter:None};add_owner(&mut sound,owner,self.host.limits.threads)?;
        if let Some(blocker)=blocker{if let Some(old)=self.sounds.iter_mut().find(|s|s.id==blocker){old.waiter=Some(id);old.thread.interrupt();if !old.name.starts_with("nc_"){old.thread.kill_vocals(&mut self.host);}}}
        self.active.insert(event.name,id);self.sounds.push(sound);Ok(id)
    }
    pub fn tick(&mut self)->Vec<MixerIntent>{
        // Stable insertion order, with a later equal-volume nightclub event winning.
        let best=self.sounds.iter().filter(|s|s.name.starts_with("nc_")&&!s.thread.dead).enumerate().max_by(|(ia,a),(ib,b)|a.thread.gain.total_cmp(&b.thread.gain).then(ia.cmp(ib))).and_then(|(_,s)|s.name.chars().last());
        if let Some(best)=best{for s in &mut self.sounds{if s.name.starts_with("nc_")&&s.name.chars().last()!=Some(best){let _=s.thread.set_gain_pan(0.0,s.thread.pan,&mut self.host);}}}
        let mut i=0;
        while i<self.sounds.len(){
            let sound=&mut self.sounds[i];
            if sound.blocker.is_some(){if !sound.thread.paused{match sound.thread.tick_number.checked_add(1){Some(n)=>sound.thread.tick_number=n,None=>sound.thread.dispose(&mut self.host)}}}
            else if sound.ever_owned && sound.owners.is_empty(){sound.thread.dispose(&mut self.host);}
            else if let Err(error)=sound.thread.tick(&mut self.host){if self.faults.len()<self.host.limits.threads{self.faults.push((sound.id,error));}}
            sound.volume_set=false;
            if sound.thread.dead{self.remove(i);}else{i+=1;}
        }
        self.host.drain_intents()
    }
    fn remove(&mut self,index:usize){
        let mut old=self.sounds.remove(index);old.thread.dispose(&mut self.host);
        if self.active.get(&old.name)==Some(&old.id){self.active.remove(&old.name);}
        if let Some(waiter)=old.waiter{if let Some(sound)=self.sounds.iter_mut().find(|s|s.id==waiter && s.blocker==Some(old.id)){sound.blocker=None;}}
    }
    pub fn thread(&self,id:ThreadId)->Option<&HitThread>{self.sounds.iter().find(|s|s.id==id).map(|s|&s.thread)}
    pub fn thread_mut(&mut self,id:ThreadId)->Option<&mut HitThread>{self.sounds.iter_mut().find(|s|s.id==id).map(|s|&mut s.thread)}
    pub fn submit_volume(&mut self,id:ThreadId,owner:EntityRef,gain:f32,pan:f32,objects:Option<Vec<i32>>)->Result<()>{
        crate::pcm::validate_gain_pan(gain,pan)?;
        if objects.as_ref().map_or(false,|o|o.len()>29){return Err(AudioError::Limit("owner audio fields"));}
        let sound=self.sounds.iter_mut().find(|s|s.id==id).ok_or(AudioError::Stale)?;
        if !sound.owners.contains(&owner){return Err(AudioError::Stale);}
        if !sound.volume_set || gain>sound.thread.gain{sound.thread.set_gain_pan(gain,pan,&mut self.host)?;if let Some(objects)=objects{sound.thread.objects=objects;}}
        sound.volume_set=true;Ok(())
    }
    pub fn stop_owner(&mut self,owner:EntityRef){for sound in &mut self.sounds{sound.owners.retain(|o|*o!=owner);}}
    pub fn reconcile_owners(&mut self,owners:&[EntityRef])->Result<()>{
        if owners.len()>self.host.limits.catalog_entries || owners.iter().any(|o|o.generation==0){return Err(AudioError::Invalid("owner reconciliation"));}
        let set:std::collections::BTreeSet<_>=owners.iter().copied().collect();if set.len()!=owners.len(){return Err(AudioError::Invalid("duplicate reconciled owner"));}
        for sound in &mut self.sounds{sound.owners.retain(|o|set.contains(o));}Ok(())
    }
    pub fn active_count(&self)->usize{self.sounds.len()}
    pub fn complete_voice(&mut self,voice:VoiceId)->bool{self.sounds.iter_mut().any(|s|s.thread.complete_voice(voice))}
    pub fn take_faults(&mut self)->Vec<(ThreadId,AudioError)>{std::mem::take(&mut self.faults)}
}
fn add_owner(sound:&mut SharedThread,owner:Option<EntityRef>,max:usize)->Result<()>{
    if let Some(owner)=owner{if !sound.owners.contains(&owner){if sound.owners.len()>=max{return Err(AudioError::Limit("event owners"));}sound.owners.push(owner);sound.ever_owned=true;}}Ok(())
}
