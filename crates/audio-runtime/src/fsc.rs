use std::{collections::BTreeMap,sync::Arc};
use crate::{AudioError,Result,hit::{HitHost,SampleRef}};
#[derive(Clone,Debug)]
pub struct Fsc {pub version:String,pub header:[u16;14],pub notes:Vec<FscNote>,pub random_jump_points:Vec<usize>,pub skipped_first:String}
#[derive(Clone,Debug)]
pub struct FscNote {pub volume:u16,pub random:bool,pub lr_pan:u16,pub fb_pan:u16,pub random2:bool,pub fade_in:u16,pub fade_out:u16,pub delay:u16,pub random3:bool,pub loop_count:u16,pub loop2:bool,pub quant:u16,pub probability:u16,pub pitch_left:i16,pub pitch_right:i16,pub fast:bool,pub group_id:u16,pub stereo:bool,pub filename:String}
impl Fsc {
    /// Named baseline profile: preserve the source parser's first candidate row
    /// skip, and ignore rows with a field count other than twenty.
    pub fn parse(bytes:&[u8],max_bytes:usize,max_notes:usize)->Result<Self>{
        if bytes.len()>max_bytes{return Err(AudioError::Limit("FSC bytes"));}if !bytes.is_ascii(){return Err(AudioError::Invalid("FSC ASCII"));}
        let text=std::str::from_utf8(bytes).map_err(|_|AudioError::Invalid("FSC text"))?;let mut lines=text.lines();
        let version=lines.next().ok_or(AudioError::Truncated(0))?.to_owned();if version.len()>256{return Err(AudioError::Limit("FSC version"));}
        let head=lines.find(|l|!l.starts_with('#')).ok_or(AudioError::Missing("FSC header"))?;let fields:Vec<_>=head.split('\t').collect();if fields.len()<15{return Err(AudioError::Invalid("FSC header fields"));}
        let mut header=[0;14];for i in 0..14{header[i]=if (i==10||i==11)&&fields[i+1].starts_with('-'){0}else{num(fields[i+1])?};}
        if header[6]==0{return Err(AudioError::Invalid("FSC tempo"));}
        let skipped_first=lines.find(|l|!l.starts_with('#')&&!l.starts_with("cells")).ok_or(AudioError::Missing("FSC first row"))?.to_owned();
        let mut notes=vec![];let mut random_jump_points=vec![];
        for line in lines{
            let values:Vec<_>=line.split('\t').collect();if values.len()!=20{continue;}
            if notes.len()>=max_notes{return Err(AudioError::Limit("FSC notes"));}
            let filename=values[19];if filename.len()>256 || filename.contains(['/', '\\']) || filename==".." || filename.contains(':'){return Err(AudioError::Invalid("FSC authorized sample name"));}
            let note=FscNote{volume:num(values[1])?,random:values[2]!="0",lr_pan:num(values[3])?,fb_pan:num(values[4])?,random2:values[5]!="0",fade_in:num(values[6])?,fade_out:num(values[7])?,delay:num(values[8])?,random3:values[9]!="0",loop_count:num(values[10])?,loop2:values[11]!="0",quant:num(values[12])?,probability:num(values[13])?,pitch_left:num(values[14])?,pitch_right:num(values[15])?,fast:values[16]!="0",group_id:num(values[17])?,stereo:values[18]!="0",filename:filename.to_owned()};
            if note.random{random_jump_points.push(notes.len());}notes.push(note);
        }
        if notes.is_empty(){return Err(AudioError::Invalid("empty FSC sequence"));}
        Ok(Self{version,header,notes,random_jump_points,skipped_first})
    }
}
fn num<T:std::str::FromStr>(text:&str)->Result<T>{text.parse().map_err(|_|AudioError::Invalid("FSC numeric field"))}
#[derive(Debug)]
pub struct FscPlayer{pub position:usize,pub loop_count:i16,pub time:f32,fsc:Arc<Fsc>,volume:f32,voices:Vec<crate::mixer::VoiceId>,stopped:bool}
impl FscPlayer{
    pub fn new(fsc:Arc<Fsc>,host:&mut HitHost)->Result<Self>{
        if fsc.notes.is_empty() || fsc.header[6]==0 || fsc.notes.len()>host.limits.hitlist_entries || fsc.random_jump_points.iter().any(|&i|i>=fsc.notes.len()){return Err(AudioError::Invalid("FSC metadata"));}
        let mut player=Self{position:0,loop_count:-1,time:0.0,fsc,volume:1.0,voices:vec![],stopped:false};player.restart(host)?;Ok(player)
    }
    fn restart(&mut self,host:&mut HitHost)->Result<()>{self.position=if self.fsc.random_jump_points.is_empty(){0}else{self.fsc.random_jump_points[host.rng.below(self.fsc.random_jump_points.len())?]};Ok(())}
    pub fn tick(&mut self,seconds:f32,host:&mut HitHost,samples:&BTreeMap<String,SampleRef>)->Result<()>{
        if !seconds.is_finite()||!(0.0..=1.0).contains(&seconds){return Err(AudioError::Invalid("FSC presentation step"));}if self.stopped{return Ok(());}
        self.time+=seconds;let beat=60.0/f32::from(self.fsc.header[6]);let mut steps=0;
        while self.time>beat{
            if steps>=host.limits.instructions_per_tick{return Err(AudioError::Limit("FSC beats per tick"));}steps+=1;self.time-=beat;
            if self.loop_count!=-1{self.loop_count=self.loop_count.wrapping_sub(1);continue;}
            let mut note=self.fsc.notes.get(self.position).ok_or(AudioError::Invalid("FSC position"))?.clone();self.position+=1;
            if note.random || self.position>=self.fsc.notes.len(){self.restart(host)?;note=self.fsc.notes[self.position].clone();}
            if note.filename=="NONE"{continue;}
            self.loop_count=note.loop_count.wrapping_sub(1) as i16;
            let play=note.probability==0 || host.rng.below(16)?<usize::from(note.probability);
            if !play{continue;}
            if self.voices.len()>=host.limits.notes_per_thread{return Err(AudioError::Limit("FSC active voices"));}
            let gain=(f32::from(note.volume)/1024.0)*(f32::from(self.fsc.header[0])/1024.0)*self.volume*host.masters[3];let pan=f32::from(note.lr_pan)/512.0-1.0;crate::pcm::validate_gain_pan(gain,pan)?;
            let mut sample=samples.get(&note.filename).ok_or(AudioError::Missing("FSC authorized sample"))?.clone();sample.group=crate::mixer::VolumeGroup::Ambience;
            self.voices.push(host.start(&sample,gain,pan,false)?);
        }Ok(())
    }
    pub fn complete_voice(&mut self,voice:crate::mixer::VoiceId,host:&mut HitHost)->bool{if let Some(index)=self.voices.iter().position(|v|*v==voice){self.voices.remove(index);host.stop_release(voice);true}else{false}}
    pub fn stop(&mut self,host:&mut HitHost){if !self.stopped{for voice in self.voices.drain(..){host.stop_release(voice);}self.stopped=true;}}
    /// Baseline behavior: affects newly created notes only; existing FSC notes
    /// retain their creation gain. Stop explicitly releases voices unlike source.
    pub fn set_volume(&mut self,volume:f32)->Result<()>{crate::pcm::validate_gain_pan(volume,0.0)?;self.volume=volume;Ok(())}
}
