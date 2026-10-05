use crate::{AudioError,Result,hit::{HitHost,SampleRef},mixer::VolumeGroup};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StationEntry{pub code:&'static str,pub path:&'static str}
pub const STATIONS:[StationEntry;23]=[
    StationEntry{code:"KBEA",path:r"Music/Stations/Beach/"},
    StationEntry{code:"KCLA",path:r"Music/Stations/Classica/"},
    StationEntry{code:"KCOU",path:r"Music/Stations/Country/"},
    StationEntry{code:"KCDA",path:r"Music/Stations/CountryD/"},
    StationEntry{code:"KDIS",path:r"Music/Stations/Disco/"},
    StationEntry{code:"KEZE",path:r"Music/Stations/EZ/"},
    StationEntry{code:"KEZX",path:r"Music/Stations/EZX/"},
    StationEntry{code:"KLAT",path:r"Music/Stations/Latin/"},
    StationEntry{code:"KRAP",path:r"Music/Stations/Rap/"},
    StationEntry{code:"KRAV",path:r"Music/Stations/Rave/"},
    StationEntry{code:"KROC",path:r"Music/Stations/Rock/"},
    StationEntry{code:"KMAP",path:r"Music/Modes/Map/"},
    StationEntry{code:"KSEL",path:r"Music/Modes/Select/"},
    StationEntry{code:"KCRE",path:r"Music/Modes/Create/"},
    StationEntry{code:"KBUY",path:r"Music/Modes/.*buy.*\.mp3"},
    StationEntry{code:"KBUI",path:r"Music/Modes/.*build.*\.mp3"},
    StationEntry{code:"KACT",path:r"sounddata/tvstations/tv_action/"},
    StationEntry{code:"KCOM",path:r"sounddata/tvstations/tv_comedy_cartoon/"},
    StationEntry{code:"KMYS",path:r"sounddata/tvstations/tv_mystery/"},
    StationEntry{code:"KROM",path:r"sounddata/tvstations/tv_romance/"},
    StationEntry{code:"KHOR",path:r"Music/Stations/Horror/"},
    StationEntry{code:"KOLD",path:r"Music/Stations/OldWorld/"},
    StationEntry{code:"KSCI",path:r"Music/Stations/SciFi/"},
];
pub fn catalog()->Vec<StationEntry>{STATIONS.to_vec()}
pub fn code_from_track(id:u32)->Result<String>{let bytes=id.to_le_bytes();if !bytes.iter().all(u8::is_ascii_uppercase){return Err(AudioError::Invalid("station code"));}String::from_utf8(bytes.to_vec()).map_err(|_|AudioError::Invalid("station code"))}
pub fn mode_station(mode:u32)->Result<Option<&'static str>>{match mode{11=>Ok(Some("KSEL")),12=>Ok(Some("KCRE")),13=>Ok(Some("KMAP")),9=>Ok(None),1=>Ok(Some("KBUY")),2=>Ok(Some("KBUI")),5=>Err(AudioError::Unsupported("mode 5 uses direct loadloop patch 0x4f85")),_=>Err(AudioError::Missing("music mode"))}}
/// Source does dirname, then truncates one additional slash component. The
/// supplied trailing separator therefore matters; returned path has no slash.
pub fn commercial_directory(station:&str)->Result<String>{
    if station.is_empty() || station.len()>1024 || station.contains("..") || station.contains(':'){return Err(AudioError::Invalid("station resource path"));}
    let path=station.replace('\\',"/");let dir=path.rsplit_once('/').map(|(d,_)|d).ok_or(AudioError::Invalid("station parent"))?;
    dir.rsplit_once('/').map(|(d,_)|d.to_owned()).ok_or(AudioError::Invalid("commercial parent"))
}
#[derive(Debug)]
pub struct StationPlayer{pub dead:bool,samples:Vec<SampleRef>,position:usize,current:Option<crate::mixer::VoiceId>,current_active:bool,music:bool,group:VolumeGroup,pub gain:f32,pub pan:f32,fade:Option<u32>,pub looped:bool}
impl StationPlayer{
    /// `samples` comes from the authorized provider in source encounter order:
    /// XA commercials first, then discovered station tracks. This runtime does
    /// not walk arbitrary paths or run a user-supplied regex against a disk.
    pub fn new(samples:Vec<SampleRef>,music:bool,group:VolumeGroup,host:&mut HitHost)->Result<Self>{
        if samples.len()>host.limits.hitlist_entries{return Err(AudioError::Limit("station samples"));}
        for s in &samples{if s.frames==0 || s.sample_rate==0{return Err(AudioError::Invalid("station duration"));}}
        let mut shuffled=Vec::with_capacity(samples.len());for sample in samples{let pos=host.rng.below(shuffled.len()+1)?;shuffled.insert(pos,sample);}
        Ok(Self{dead:false,samples:shuffled,position:0,current:None,current_active:false,music,group,gain:1.0,pan:0.0,fade:None,looped:false})
    }
    pub fn tick(&mut self,host:&mut HitHost)->Result<()>{
        if self.dead{return Ok(());}crate::pcm::validate_gain_pan(self.gain,self.pan)?;
        let pan=if self.music{0.0}else{self.pan};let mut gain=self.gain*host.masters[self.group as usize];
        if let Some(left)=&mut self.fade{*left=left.saturating_sub(1);gain=(*left as f32/120.0-0.5).max(0.0)*host.masters[self.group as usize];if *left==0{self.kill(host);return Ok(());}}
        if let Some(voice)=self.current{host.intents.push(crate::mixer::MixerIntent::SetGainPan{voice,gain:gain.clamp(0.0,1.0),pan});}
        if !self.current_active{
            if let Some(voice)=self.current.take(){host.stop_release(voice);}
            if self.samples.is_empty(){self.dead=true;return Ok(());}
            let mut sample=self.samples[self.position].clone();self.position=(self.position+1)%self.samples.len();sample.group=self.group;
            // Source PlayNext uses ordinary instance volume even during fade.
            self.current=Some(host.start(&sample,(self.gain*host.masters[self.group as usize]).clamp(0.0,1.0),pan,self.looped)?);self.current_active=true;
        }Ok(())
    }
    pub fn complete_current(&mut self){self.current_active=false;}
    pub fn complete_voice(&mut self,voice:crate::mixer::VoiceId)->bool{if self.current==Some(voice){let active=self.current_active;self.current_active=false;active}else{false}}
    pub fn current_voice(&self)->Option<crate::mixer::VoiceId>{self.current}
    pub fn fade(&mut self){if self.fade.is_none(){self.fade=Some(180);}}
    pub fn kill(&mut self,host:&mut HitHost){if !self.dead{if let Some(voice)=self.current.take(){host.stop_release(voice);}self.samples.clear();self.current_active=false;self.dead=true;}}
}
#[derive(Debug,Default)]
pub struct MusicQueue{current:Option<StationPlayer>,next:Option<StationPlayer>}
impl MusicQueue{
    pub fn replace(&mut self,mut next:StationPlayer,eager:bool,host:&mut HitHost)->Result<()>{
        if let Some(old)=&mut self.next{old.kill(host);}if let Some(current)=&mut self.current{current.fade();}
        if eager{next.looped=true;next.tick(host)?;}self.next=Some(next);Ok(())
    }
    pub fn tick(&mut self,host:&mut HitHost)->Result<()>{
        if let Some(current)=&mut self.current{current.tick(host)?;}
        if self.current.as_ref().map_or(true,|p|p.dead){if self.next.is_some(){self.current=self.next.take();}}
        Ok(())
    }
    pub fn complete_voice(&mut self,voice:crate::mixer::VoiceId)->bool{self.current.as_mut().map_or(false,|p|p.complete_voice(voice)) || self.next.as_mut().map_or(false,|p|p.complete_voice(voice))}
    pub fn stop(&mut self,host:&mut HitHost){if let Some(current)=&mut self.current{current.kill(host);}if let Some(next)=&mut self.next{next.kill(host);}self.current=None;self.next=None;}
}
