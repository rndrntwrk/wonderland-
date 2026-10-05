use std::sync::Arc;
use wonderland_audio_runtime::{system::*,cue::*,hit::*,runtime::*,mixer::*,projection::*};
use wonderland_render_core::{AssetKey,EntityRef};
fn entity()->EntityRef{EntityRef{object_id:1,generation:2}}
fn cue(tick:u64,ordinal:u32,name:&str)->AudioCue{AudioCue{id:CueId{lot_id:1,timeline:1,tick,event_ordinal:ordinal,nested_ordinal:0,owner:Some(entity())},action:CueAction::Play{event:name.into(),looped:false}}}
fn sample(id:u8)->SampleRef{SampleRef{key:AssetKey([id;32]),sample_rate:60,frames:60,group:VolumeGroup::Fx}}
fn system_with_content(content_override:Option<AudioContent>)->AudioSystem{
    let limits=HitLimits::default();let mut cat=HitCatalog::default();cat.samples.insert(8,sample(2));cat.samples.insert(0x4f85,sample(5));cat.tracks.insert(7,Track{track_id:7,sound_id:8,hitlist_id:None,looped:None});
    let program=Arc::new(HitProgram::new(vec![0,2,0,0x0b,8],&limits).unwrap());let bank=EventBank::new(vec![ResourceGroup{kind:TsoGroup::NewMain,program,hsm:Some(vec![("beep".into(),1)]),entrypoints:vec![],events:vec![EventRecord{name:"beep".into(),event_type:1,track_id:7},EventRecord{name:"radio".into(),event_type:30,track_id:u32::from_le_bytes(*b"KBEA")},EventRecord{name:"load".into(),event_type:36,track_id:5}]}],&limits).unwrap();
    let runtime=AudioRuntime::new(HitHost::new(Arc::new(cat),limits,1,1).unwrap(),bank);let mut content=AudioContent::default();content.stations.insert("KBEA".into(),StationPlaylist{samples:vec![sample(1)],music:true});content.ambience_loops.insert(6,sample(6));content.ambience_loops.insert(7,sample(7));
    let note="cell\t1024\t0\t512\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\tmissing.xa";
    let fsc=format!("FSC1\nheader\t1024\t0\t0\t0\t1\t1\t3600\t4\t0\t0\t0\t0\t0\t0\n{note}\n{note}\n");
    content.fsc.insert(0,Arc::new(wonderland_audio_runtime::fsc::Fsc::parse(fsc.as_bytes(),4096,8).unwrap()));content.fsc_samples.insert(0,Default::default());
    AudioSystem::new(runtime,CueLedger::new(1,1,32).unwrap(),content_override.unwrap_or(content)).unwrap()
}
fn system()->AudioSystem{system_with_content(None)}
#[test]
fn causal_cues_route_station_and_hit_in_shared_source_insertion_order(){
    let mut s=system();let radio=cue(1,0,"radio");let beep=cue(1,1,"beep");assert_eq!(s.accept(&radio).unwrap(),CueAdmission::New);s.accept(&beep).unwrap();assert_eq!(s.accept(&radio).unwrap(),CueAdmission::Duplicate);let out=s.tick().unwrap();let samples:Vec<_>=out.iter().filter_map(|i|if let MixerIntent::Start{sample,..}=i{Some(sample.0[0])}else{None}).collect();assert_eq!(samples,vec![1,2]);
    let stop=AudioCue{id:CueId{tick:2,event_ordinal:0,..radio.id},action:CueAction::StopOwner};s.accept(&stop).unwrap();let out=s.tick().unwrap();assert_eq!(out.iter().filter(|i|matches!(i,MixerIntent::Release{..})).count(),2);
}
#[test]
fn direct_loadloop_and_ambience_replacement_produce_owned_loop_intents(){
    let mut s=system();s.accept(&cue(1,0,"load")).unwrap();let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Start{sample:AssetKey([5,..]),looped:true,group:VolumeGroup::Music,..})));
    s.set_ambience(6,true).unwrap();s.tick().unwrap();s.set_ambience(7,true).unwrap();let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Release{..})));assert!(out.iter().any(|i|matches!(i,MixerIntent::Start{sample:AssetKey([7,..]),looped:true,group:VolumeGroup::Ambience,..})));assert_eq!(s.ambience.bits,1<<7);let stopped=s.stop_all();assert_eq!(stopped.iter().filter(|i|matches!(i,MixerIntent::Release{..})).count(),2);assert!(s.stop_all().is_empty());
}
#[test]
fn muted_group_keeps_logical_cue_but_zeroes_playback_gain(){let mut s=system();s.set_master(VolumeGroup::Fx,0.0).unwrap();s.accept(&cue(1,0,"beep")).unwrap();let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Start{gain,..} if *gain==0.0)));}
struct Provider;impl FwavProvider for Provider{fn scoped_event(&self,_:AssetKey,id:u16)->Option<String>{if id==3{Some("scoped".into())}else{None}}fn global_event(&self,_:u16)->Option<String>{Some("global".into())}}
#[test]
fn a_sound_projection_preserves_owner_flag_quirks_and_scoped_fallback(){
    let id=cue(1,4,"x").id;let mut req=SoundRequest{opcode:23,operand:[3,0,0,1,15,99,0,0],caller:entity(),stack:Some(EntityRef{object_id:2,generation:1}),scope:AssetKey([0;32])};let p=project_request(id.clone(),&req,&Provider).unwrap().unwrap();assert_eq!(p.cue.id.owner,req.stack);assert!(p.no_pan&&p.no_zoom);assert_eq!(p.source_sample_rate,256);assert_eq!(p.source_volume,99);assert!(matches!(p.cue.action,CueAction::Play{event,looped:true} if event=="scoped"));
    req.operand[0]=4;assert!(matches!(project_request(id.clone(),&req,&Provider).unwrap().unwrap().cue.action,CueAction::Play{event,..} if event=="global"));req.opcode=48;req.operand[0]=3;assert_eq!(project_request(id.clone(),&req,&Provider).unwrap().unwrap().cue.id.owner,Some(req.caller));req.operand[0]=1;assert_eq!(project_request(id,&req,&Provider).unwrap().unwrap().cue.id.owner,req.stack);
}
#[test]
fn avatar_audio_keeps_nested_source_ordinal_and_does_not_reinterpret_dress(){
    let out=project_avatar(cue(1,6,"x").id,&[AvatarCueView::Other,AvatarCueView::Other,AvatarCueView::Other,AvatarCueView::Sound("step".into()),AvatarCueView::Dress("hat".into()),AvatarCueView::Undress("old".into()),AvatarCueView::Sound("swoosh".into())]).unwrap();assert_eq!(out.len(),2);assert_eq!(out.iter().map(|c|c.id.nested_ordinal).collect::<Vec<_>>(),vec![3,6]);
}
#[test]
fn a_missing_fsc_sample_cannot_discard_other_players_ordered_intents(){
    let mut s=system();s.set_ambience(0,true).unwrap();s.tick().unwrap();s.accept(&cue(2,0,"beep")).unwrap();let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Start{sample:AssetKey([2,..]),..})));assert_eq!(s.take_faults().len(),1);assert!(s.tick().is_ok());
}
#[test]
fn fsc_players_emit_after_hit_starts_in_activation_order(){
    let mut content=AudioContent::default();let note="cell\t1024\t0\t512\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\tsound.xa";let source=format!("FSC1\nheader\t1024\t0\t0\t0\t1\t1\t3600\t4\t0\t0\t0\t0\t0\t0\n{note}\n{note}\n");
    for bit in [0u8,20]{content.fsc.insert(bit,Arc::new(wonderland_audio_runtime::fsc::Fsc::parse(source.as_bytes(),4096,8).unwrap()));content.fsc_samples.insert(bit,std::collections::BTreeMap::from([("sound.xa".into(),sample(bit))]));}
    let mut s=system_with_content(Some(content));s.set_ambience(20,true).unwrap();s.set_ambience(0,true).unwrap();s.tick().unwrap();s.accept(&cue(2,0,"beep")).unwrap();let out=s.tick().unwrap();let ids:Vec<_>=out.iter().filter_map(|i|if let MixerIntent::Start{sample,..}=i{Some(sample.0[0])}else{None}).collect();assert_eq!(ids,vec![2,20,0]);
}
#[test]
fn mixed_player_budget_counts_hit_and_station_but_allows_existing_owner_sharing(){
    let mut s=system();s.runtime.host.limits.threads=1;s.accept(&cue(1,0,"radio")).unwrap();
    assert!(s.accept(&cue(1,1,"beep")).is_err());s.accept(&cue(1,2,"radio")).unwrap();
    let mut shared=cue(1,3,"radio");shared.id.owner=Some(EntityRef{object_id:2,generation:1});assert!(s.accept(&shared).is_err());
    let out=s.tick().unwrap();assert_eq!(out.iter().filter(|i|matches!(i,MixerIntent::Start{..})).count(),1);
}
#[test]
fn station_owner_volume_uses_first_tie_then_resets_each_tick_and_checks_generation(){
    let mut content=AudioContent::default();content.stations.insert("KBEA".into(),StationPlaylist{samples:vec![sample(1)],music:false});
    let mut s=system_with_content(Some(content));let a=entity();let b=EntityRef{object_id:2,generation:1};
    s.accept(&cue(1,0,"radio")).unwrap();let mut shared=cue(1,1,"radio");shared.id.owner=Some(b);s.accept(&shared).unwrap();
    s.submit_volume("radio",a,0.6,0.75,None).unwrap();s.submit_volume("RADIO",b,0.6,-0.75,None).unwrap();
    let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Start{gain,pan,..} if *gain==0.6&&*pan==0.75)));
    s.submit_volume("radio",b,0.2,-0.5,None).unwrap();s.set_master(VolumeGroup::Fx,0.5).unwrap();
    let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::SetGainPan{gain,pan,..} if *gain==0.1&&*pan == -0.5)));
    assert!(s.submit_volume("radio",EntityRef{generation:3,..a},1.0,0.0,None).is_err());
    s.reconcile_owners(&[EntityRef{generation:3,..a}]).unwrap();let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Release{..})));
}
#[test]
fn latest_pending_music_replacement_reuses_its_capacity_and_cancels_old_start(){
    let mut s=system();s.runtime.host.limits.threads=1;s.runtime.bank.groups[0].events.push(EventRecord{name:"load2".into(),event_type:36,track_id:5});
    s.accept(&cue(1,0,"load")).unwrap();s.accept(&cue(1,1,"load2")).unwrap();let out=s.tick().unwrap();
    let starts:Vec<_>=out.iter().filter_map(|i|if let MixerIntent::Start{voice,..}=i{Some(*voice)}else{None}).collect();assert_eq!(starts.len(),1);assert_eq!(starts[0].serial,2);
    assert!(!s.complete_voice(VoiceId{generation:1,serial:1}));assert_eq!(s.stop_all().iter().filter(|i|matches!(i,MixerIntent::Release{..})).count(),1);
}
#[test]
fn failed_ambience_loop_feedback_releases_and_explicit_reapplication_retries(){
    let mut s=system();s.set_ambience(6,true).unwrap();let out=s.tick().unwrap();let voice=out.iter().find_map(|i|if let MixerIntent::Start{voice,..}=i{Some(*voice)}else{None}).unwrap();
    assert!(s.complete_voice(voice));assert!(!s.complete_voice(voice));s.set_ambience(6,true).unwrap();let out=s.tick().unwrap();assert!(out.iter().any(|i|matches!(i,MixerIntent::Start{voice:new,..} if *new!=voice)));
}

#[test]
fn repeat_owner_request_preserves_program_set_loop_while_a_new_owner_applies_flags(){
    let mut s=system();s.runtime.bank.groups[0].program=Arc::new(HitProgram::new(vec![0,0x21,0x60,0,0x0b,8],&s.runtime.host.limits).unwrap());
    s.accept(&cue(1,0,"beep")).unwrap();s.tick().unwrap();let id=s.runtime.event_thread("beep").unwrap();assert!(s.runtime.thread(id).unwrap().has_set_loop);
    s.accept(&cue(2,0,"beep")).unwrap();assert!(s.runtime.thread(id).unwrap().has_set_loop);
    let mut other=cue(2,1,"beep");other.id.owner=Some(EntityRef{object_id:2,generation:1});s.accept(&other).unwrap();assert!(!s.runtime.thread(id).unwrap().has_set_loop);
}
