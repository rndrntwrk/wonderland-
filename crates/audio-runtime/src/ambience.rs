use crate::{AudioError,Result};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmbienceEntry {pub bit:u8,pub guid:u32,pub category:u8,pub name:&'static str,pub file:&'static str}
pub const AMBIENCE:[AmbienceEntry;39]=[
    AmbienceEntry{bit:0,guid:0x3dd887a6,category:0,name:"AnimalsSongBirds",file:"daybirds"},
    AmbienceEntry{bit:1,guid:0x3dd887aa,category:1,name:"MechanicalExplosions",file:"explosions"},
    AmbienceEntry{bit:2,guid:0x7dd887ad,category:0,name:"AnimalsFarm",file:"farmanimals"},
    AmbienceEntry{bit:3,guid:0x9dd887af,category:1,name:"MechanicalGunshot",file:"gunshots"},
    AmbienceEntry{bit:4,guid:0xddd887b3,category:1,name:"MechanicalPlanes",file:"planes"},
    AmbienceEntry{bit:5,guid:0xfdd887b5,category:2,name:"WeatherLightingThunder",file:"thunder"},
    AmbienceEntry{bit:6,guid:0x9e0bc19a,category:4,name:"LoopBrook",file:"loops/brook_lp.xa"},
    AmbienceEntry{bit:7,guid:0xfe0bc1a1,category:4,name:"LoopCrowd",file:"loops/crowd_lp.xa"},
    AmbienceEntry{bit:8,guid:0x1e0bc1a3,category:4,name:"LoopHeartbeat",file:"loops/heartbeat_lp.xa"},
    AmbienceEntry{bit:9,guid:0x5e0bc1a4,category:4,name:"LoopIndoor",file:"loops/indoor_lp.xa"},
    AmbienceEntry{bit:10,guid:0x5e0bc1a6,category:4,name:"LoopInsects",file:"loops/insect_lp.xa"},
    AmbienceEntry{bit:11,guid:0xbe0bc1a9,category:4,name:"LoopOcean",file:"loops/ocean_lp.xa"},
    AmbienceEntry{bit:12,guid:0x1e0bc1ab,category:4,name:"LoopOutdoor",file:"loops/outdoor_lp.xa"},
    AmbienceEntry{bit:13,guid:0xde0bc1ad,category:4,name:"LoopRain",file:"loops/rain_lp.xa"},
    AmbienceEntry{bit:14,guid:0x3e0bc2af,category:4,name:"LoopTechno",file:"loops/scifi_lp.xa"},
    AmbienceEntry{bit:15,guid:0x1e0bc2b2,category:4,name:"LoopStorm",file:"loops/storm_lp.xa"},
    AmbienceEntry{bit:16,guid:0x3e0bc2b4,category:4,name:"LoopTraffic",file:"loops/traffic_lp.xa"},
    AmbienceEntry{bit:17,guid:0x1e0bc2b5,category:4,name:"LoopWind",file:"loops/wind_lp.xa"},
    AmbienceEntry{bit:18,guid:0x1e128187,category:2,name:"WeatherBreeze",file:"breeze"},
    AmbienceEntry{bit:19,guid:0xfe128189,category:1,name:"MechanicalConstruction",file:"construction"},
    AmbienceEntry{bit:20,guid:0x5e12818c,category:0,name:"AnimalsDog",file:"dog"},
    AmbienceEntry{bit:21,guid:0xbe12818d,category:1,name:"MechanicalDriveBy",file:"driveby"},
    AmbienceEntry{bit:22,guid:0xde12818f,category:2,name:"WeatherHowlingWind",file:"howlingwind"},
    AmbienceEntry{bit:23,guid:0x1e128190,category:1,name:"MechanicalIndustrial",file:"indust"},
    AmbienceEntry{bit:24,guid:0x3e128192,category:0,name:"AnimalsInsects",file:"insect"},
    AmbienceEntry{bit:25,guid:0xbe128196,category:0,name:"AnimalsJungle",file:"jungle"},
    AmbienceEntry{bit:26,guid:0xde128198,category:3,name:"PeopleOffice",file:"office"},
    AmbienceEntry{bit:27,guid:0x3e12819a,category:3,name:"PeopleRestaurant",file:"restaurant"},
    AmbienceEntry{bit:28,guid:0xbe12819c,category:1,name:"MechanicalSciBleeps",file:"scibleeps"},
    AmbienceEntry{bit:29,guid:0x1e1281ac,category:1,name:"MechanicalSirens",file:"siren"},
    AmbienceEntry{bit:30,guid:0x1e1281ad,category:0,name:"AnimalsWolf",file:"wolf"},
    AmbienceEntry{bit:31,guid:0xbe19bb2d,category:0,name:"AnimalsSeaBirds",file:"seabirds"},
    AmbienceEntry{bit:32,guid:0xde19bb31,category:2,name:"WeatherRainDrops",file:"raindrops"},
    AmbienceEntry{bit:33,guid:0xbe1a033e,category:3,name:"PeopleMagic",file:"magic"},
    AmbienceEntry{bit:34,guid:0xa9b9652a,category:1,name:"MechanicalSmallMachines",file:"smallmachines"},
    AmbienceEntry{bit:35,guid:0xa9b96536,category:3,name:"PeopleScreams",file:"screams"},
    AmbienceEntry{bit:36,guid:0xa9b96539,category:0,name:"AnimalsNightBirds",file:"nightbirds"},
    AmbienceEntry{bit:37,guid:0xa9b9653c,category:3,name:"PeopleGym",file:"gym"},
    AmbienceEntry{bit:38,guid:0xa9b9653e,category:3,name:"PeopleGhost",file:"ghost"},
];
pub fn catalog()->Vec<AmbienceEntry>{AMBIENCE.to_vec()}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmbienceChange{Start(u8),Stop(u8)}
#[derive(Debug,Default)]
pub struct AmbienceSelection {pub bits:u64}
impl AmbienceSelection{
    pub fn set(&mut self,id:u8,enabled:bool)->Result<Vec<AmbienceChange>>{
        let entry=AMBIENCE.get(usize::from(id)).ok_or(AudioError::Invalid("ambience ID"))?;
        let bit=1u64<<id;let active=self.bits&bit!=0;if active==enabled{return Ok(vec![]);}
        let mut changes=vec![];
        if enabled{
            if entry.category==4{for e in AMBIENCE.iter().filter(|e|e.category==4){if self.bits&(1u64<<e.bit)!=0{self.bits&=!(1u64<<e.bit);changes.push(AmbienceChange::Stop(e.bit));}}}
            self.bits|=bit;changes.push(AmbienceChange::Start(id));
        }else{self.bits&=!bit;changes.push(AmbienceChange::Stop(id));}
        Ok(changes)
    }
    pub fn from_guid(guid:u32)->Result<u8>{AMBIENCE.iter().find(|e|e.guid==guid).map(|e|e.bit).ok_or(AudioError::Missing("ambience GUID"))}
}
pub fn dj_pattern(category:u8,digits:[u8;3])->Result<(u8,u8)>{
    if category>3 || digits.iter().any(|&v|v>3){return Err(AudioError::Invalid("DJ pattern"));}
    let category=match category{0=>1,1=>0,n=>n};Ok((category+10,digits[0]*16+digits[1]*4+digits[2]))
}
