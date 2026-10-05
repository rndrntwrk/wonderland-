use std::sync::Arc;
use wonderland_audio_runtime::{runtime::*,hit::*,mixer::*};
use wonderland_render_core::{AssetKey,EntityRef};
fn owner(id:u32)->EntityRef{EntityRef{object_id:id,generation:1}}
fn make(events:&[&str],body:Vec<u8>)->AudioRuntime{
    let limits=HitLimits::default();let mut bytes=vec![0];bytes.extend(body);let p=Arc::new(HitProgram::new(bytes,&limits).unwrap());
    let group=ResourceGroup{kind:TsoGroup::NewMain,program:p,hsm:Some(events.iter().map(|s|(s.to_string(),1)).collect()),entrypoints:vec![],events:events.iter().map(|s|EventRecord{name:s.to_string(),event_type:1,track_id:7}).collect()};
    let mut c=HitCatalog::default();c.tracks.insert(7,Track{track_id:7,sound_id:8,hitlist_id:None,looped:None});c.samples.insert(8,SampleRef{key:AssetKey([8;32]),group:VolumeGroup::Vox,sample_rate:60,frames:3});
    AudioRuntime::new(HitHost::new(Arc::new(c),limits.clone(),20,9).unwrap(),EventBank::new(vec![group],&limits).unwrap())
}
#[test]
fn global_event_sharing_aggregates_loudest_owner_and_first_tie(){
    let mut r=make(&["beep"],vec![0x60,0,0x0b,8]);let a=r.play("BEEP",Some(owner(1))).unwrap();let b=r.play("beep",Some(owner(2))).unwrap();assert_eq!(a,b);assert_eq!(r.active_count(),1);
    r.submit_volume(a,owner(1),0.5,-0.7,None).unwrap();r.submit_volume(a,owner(2),0.5,0.9,None).unwrap();let intents=r.tick();
    assert!(matches!(intents.as_slice(),[MixerIntent::Start{gain,pan,..}] if *gain==0.5 && *pan == -0.7));
    r.submit_volume(a,owner(2),0.8,0.9,None).unwrap();r.submit_volume(a,owner(1),0.6,-0.1,None).unwrap();let intents=r.tick();assert!(matches!(intents.last(),Some(MixerIntent::SetGainPan{gain,pan,..}) if *gain==0.8 && *pan==0.9));
    r.stop_owner(owner(1));r.tick();assert_eq!(r.active_count(),1);r.stop_owner(owner(2));assert!(r.tick().iter().any(|i|matches!(i,MixerIntent::Release{..})));assert_eq!(r.active_count(),0);
}
#[test]
fn all_threads_tick_before_ordered_queued_starts_and_faults_retire_active_names(){
    let mut r=make(&["a","b"],vec![2,0,0x0b,8]);r.play("b",None).unwrap();r.play("a",None).unwrap();let out=r.tick();let serials:Vec<_>=out.iter().map(|i|match i{MixerIntent::Start{voice,..}=>voice.serial,_=>panic!()}).collect();assert_eq!(serials,vec![1,2]);
    let mut r=make(&["bad"],vec![0x61]);let old=r.play("bad",None).unwrap();r.tick();assert_eq!(r.active_count(),0);assert_eq!(r.take_faults().len(),1);let new=r.play("bad",None).unwrap();assert_ne!(old,new);
}
#[test]
fn interruptible_replacement_keeps_latest_waiter_and_cleans_old_voices(){
    let mut r=make(&["voice"],vec![4,0x31,1,0x60,0,0x0b,8]);let old=r.play("voice",Some(owner(1))).unwrap();r.tick();let first=r.play("voice",Some(owner(1))).unwrap();let latest=r.play("voice",Some(owner(2))).unwrap();assert_ne!(first,latest);
    let out=r.tick();assert!(out.iter().any(|i|matches!(i,MixerIntent::Release{..})));assert!(r.thread(first).is_none());
    for _ in 0..3{r.tick();}assert!(r.thread(old).is_none());assert_eq!(r.thread(latest).unwrap().active_notes(),1);
}
#[test]
fn nightclub_later_tie_wins_suffix_and_interruption_waits_for_boundary(){
    let mut r=make(&["nc_drums_a","nc_bass_b"],vec![4,0x31,1,0x60,0,0x0b,8]);let a=r.play("nc_drums_a",Some(owner(1))).unwrap();let b=r.play("nc_bass_b",Some(owner(2))).unwrap();r.submit_volume(a,owner(1),1.0,0.0,None).unwrap();r.submit_volume(b,owner(2),1.0,0.0,None).unwrap();let out=r.tick();let gains:Vec<_>=out.iter().filter_map(|i|if let MixerIntent::Start{gain,..}=i{Some(*gain)}else{None}).collect();assert_eq!(gains,vec![0.0,1.0]);
    let waiter=r.play("nc_bass_b",Some(owner(2))).unwrap();let out=r.tick();assert!(out.iter().all(|i|!matches!(i,MixerIntent::Start{..})));assert_eq!(r.thread(waiter).unwrap().active_notes(),0);
    for _ in 0..5{r.tick();}assert_eq!(r.thread(waiter).unwrap().active_notes(),1);
}
#[test]
fn reconcile_generation_removes_stale_owner_without_restarting_matching_voice(){
    let mut r=make(&["wind"],vec![0x60,0,0x0b,8]);let id=r.play("wind",Some(owner(1))).unwrap();r.tick();let voice=r.thread(id).unwrap().last_voice().unwrap();r.reconcile_owners(&[owner(1)]).unwrap();assert!(r.tick().iter().all(|i|!matches!(i,MixerIntent::Start{..})));assert_eq!(r.thread(id).unwrap().last_voice(),Some(voice));r.reconcile_owners(&[EntityRef{object_id:1,generation:2}]).unwrap();r.tick();assert_eq!(r.active_count(),0);assert!(!r.complete_voice(voice));
}
#[test]
fn provider_group_first_wins_hsm_and_entrypoint_and_piano_resolution(){
    let limits=HitLimits::default();let p=Arc::new(HitProgram::new(vec![0;64],&limits).unwrap());let base=ResourceGroup{kind:TsoGroup::TsoV2,program:p.clone(),hsm:None,entrypoints:vec![(7,20)],events:vec![EventRecord{name:"beep".into(),event_type:1,track_id:7}]};
    let mut higher=base.clone();higher.kind=TsoGroup::NewMain;higher.hsm=Some(vec![("beep".into(),12),("beep".into(),30),("guid_tkd_beep".into(),99),("playpiano".into(),40)]);higher.events.push(EventRecord{name:"piano_play".into(),event_type:43,track_id:7});
    let h=HitHost::new(Arc::new(HitCatalog::default()),limits.clone(),1,1).unwrap();let bank=EventBank::new(vec![base.clone(),higher],&limits).unwrap();assert!(matches!(bank.resolve("BEEP",&h).unwrap().kind,ResolvedKind::Hit{pc:12,track:99,fallback:7,..}));let piano=bank.resolve("piano_play",&h).unwrap();assert_eq!(piano.name,"playpiano");assert!(matches!(piano.kind,ResolvedKind::Hit{pc:40,..}));let bank=EventBank::new(vec![base],&limits).unwrap();assert!(matches!(bank.resolve("beep",&h).unwrap().kind,ResolvedKind::Hit{pc:20,..}));
}

#[test]
fn nightclub_winner_compares_effective_master_scaled_volume(){
    let mut r=make(&["nc_drums_a","nc_bass_b"],vec![0x60,0,0x0b,8]);let a=r.play("nc_drums_a",Some(owner(1))).unwrap();let b=r.play("nc_bass_b",Some(owner(2))).unwrap();r.tick();
    r.thread_mut(a).unwrap().group=VolumeGroup::Vox;r.thread_mut(b).unwrap().group=VolumeGroup::Fx;
    r.submit_volume(a,owner(1),1.0,0.0,None).unwrap();r.submit_volume(b,owner(2),0.5,0.0,None).unwrap();r.set_master(VolumeGroup::Vox,0.2).unwrap();r.tick();
    assert_eq!(r.thread(a).unwrap().gain,0.0);assert_eq!(r.thread(b).unwrap().gain,0.5);
}
