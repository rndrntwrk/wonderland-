//! Private isolated genuine SourceGoto probe; no account or browser acceptance.
use std::{collections::BTreeSet, fs, path::Path};
use serde_json::{json, Value};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{GameRuntime, content_bridge::cooked::*, sim_core};
use sim_core::{
 ids::{ObjectId, PersistentId}, interactions::{FinishResult, PrincipalKey, QueueEventKind},
 runtime::{goto_adapter::*, *}, route_motion::{MotionAnimation, MotionPhase, RouteMotionRecord},
 vm::VmMode, world::{Facing, LotModel, TilePos},
};
fn load(root: &Path, expected: &Digest)->Result<GameRuntime,Box<dyn std::error::Error>> {
 let limits=CookedLoadLimits::default();let binding=fs::read(root.join("binding.json"))?;let manifest=fs::read(root.join("manifest.json"))?;
 let prepared=PreparedRelease::from_binding(&binding,expected,&manifest,&limits)?;
 let bytes=prepared.required_packs().iter().map(|p|fs::read(root.join(format!("{}.wlp",p.digest.as_str())))).collect::<Result<Vec<_>,_>>()?;
 let packs=prepared.required_packs().iter().zip(&bytes).map(|(p,bytes)|PackBytes{digest:&p.digest,bytes}).collect::<Vec<_>>();
 Ok(GameRuntime::from_cooked(prepared.load(&packs,&limits)?,LotModel::new(8,8,2).map_err(|e|format!("{e:?}"))?,RuntimeConfig::new(VmMode::Tso,7,1,123),RuntimeRole::Authority)?)
}
fn run_case(root:&Path,binding:&Digest,interaction:u16,guid:u32)->Result<Value,Box<dyn std::error::Error>> {
 let mut r=load(root,binding)?;
 r.advance(vec![AcceptedCommand::Spawn(SpawnSpec{guid,position:TilePos::new(1,1,1).center(),facing:Facing::NORTH,persistent_id:PersistentId(1),avatar:true})])?;
 let actor=r.sim().state().entities[&ObjectId(1)].info.reference;
 r.advance(vec![AcceptedCommand::SetInteractionAuthority{actor,access:Some(interaction_adapter::InteractionAccess{principal:PrincipalKey(9),allow_hidden:false})}])?;
 let before=r.sim().state().clone();let offers=r.sim().terrain_offers(PrincipalKey(9),actor,TilePos::new(3,3,1).center())?;
 assert_eq!(r.sim().state(),&before);assert_eq!(offers,r.sim().terrain_offers(PrincipalKey(9),actor,TilePos::new(3,3,1).center())?);assert_eq!(r.sim().state(),&before);
 let selected=offers.offers.iter().find(|o|o.interaction.tta_index==u32::from(interaction)).ok_or("requested source interaction absent")?;
 let selected_label=selected.label.clone();
 let action=SourceGotoIntent{principal:PrincipalKey(9),actor:offers.actor,world_revision:offers.world_revision,queue_revision:offers.queue_revision,command_sequence:1,x:56,y:56,level:1,interaction,param0:selected.param0};
 let mut replica=SimRuntime::from_state(r.sim().state().clone(),r.sim().content().clone(),RuntimeRole::Replica)?;
 let mut restored=SimRuntime::from_state(r.sim().state().clone(),r.sim().content().clone(),RuntimeRole::Replica)?;
 let mut selected_action=None;let mut route_success=false;let mut selected_success=false;
 let mut active_ticks=0;let mut walk_ticks=0;let mut turn_ticks=0;let mut fractional_ticks=0;let mut turn_resources=BTreeSet::new();let mut walk_resources=BTreeSet::new();let mut phases=BTreeSet::new();let mut trace=vec![];
 for i in 0..1000 {
  let input=r.sim().next_tick(if i==0 {vec![AcceptedCommand::SourceGoto(action)]}else{vec![]})?;
  let a=r.apply_accepted(&input)?;let b=replica.step(&input)?;let c=restored.step(&input)?;
  assert_eq!(a,b);assert_eq!(a,c);assert_eq!(r.sim().state(),replica.state());assert_eq!(r.sim().state(),restored.state());
  assert!(r.sim().pending_effects()?.is_empty(),"external effect in source route");
  assert!(!a.events.iter().any(|e|matches!(e,RuntimeEvent::ThreadFault{..}|RuntimeEvent::RouteAnimationUnavailable{..})),"fault or animation fallback: {:?}",a.events);
  let state=r.sim().state();let queue=&state.interaction_queues[&actor.object_id];
  if i==0 {selected_action=Some(queue.entries().iter().find(|entry|entry.invocation.definition.key.tta_index==u32::from(interaction)&&entry.invocation.target.slot!=actor.object_id.0 as u32).ok_or("source action was not enqueued")?.id);}
  for event in &a.events {
   match event {
    RuntimeEvent::RouteFinished{entity,success,code,..} if *entity==actor => {assert!(*success&&*code==sim_core::world::RouteFailCode::Success);route_success=true;},
    RuntimeEvent::Interaction(event) if event.action==selected_action => match event.kind {
     QueueEventKind::Finished{result}=>{assert_eq!(result,FinishResult::Succeeded);selected_success=true;},
     QueueEventKind::StartRejected|QueueEventKind::RuntimeFailed{..}=>panic!("source action failed: {event:?}"),
     _=>{}
    },_=>{}
   }
  }
  let entity=&state.entities[&actor.object_id];let layers=&entity.avatar.as_ref().unwrap().animations.animations;
  let mut motion_json=Value::Null;
  for record in state.source_routing.routes.values().filter(|v|v.actor()==actor) {
   match record {
    RouteMotionRecord::Unavailable{reason,..}=>panic!("source motion unavailable: {reason}"),
    RouteMotionRecord::Active(m)=>{
     active_ticks+=1;phases.insert(format!("{:?}",m.phase));assert!(m.visual.validate());
     for layer in layers {assert_eq!(r.sim().content().animation_by_resource(&layer.metadata.resource),Some(&layer.metadata));}
     if m.animation==MotionAnimation::Walk {
      walk_ticks+=1;assert_eq!(layers.len(),3);let velocity=f32::from(m.blend_velocity)/8.;let weights=[1.-velocity,velocity*0.66,velocity*0.33];
      for (n,layer) in layers.iter().enumerate(){assert_eq!(layer.metadata.resource,m.resources.names[n]);assert_eq!(layer.weight,weights[n]);assert_eq!(layer.speed,if n==0 {1.}else{1.5});walk_resources.insert(layer.metadata.resource.clone());}
      let full=if interaction==4 {21}else{20};let half=if interaction==4 {21}else{25};
      for (n,index) in [3,full,half].into_iter().enumerate(){assert_eq!(m.resources.names[n],format!("{}.anim",r.sim().content().string(0,150,index).ok_or("missing source STR")?));}
      if m.visual.visual_position.iter().take(2).zip([m.visual.position.x,m.visual.position.y]).any(|(p,i)|(*p*16.-i as f32).abs()>0.01){fractional_ticks+=1;}
     }
     if let MotionAnimation::Turn(index)=m.animation {
      turn_ticks+=1;assert_eq!(layers.len(),1);assert_eq!(layers[0].metadata.resource,m.resources.names[3+usize::from(index)]);assert_eq!(layers[0].metadata.num_frames,m.resources.turn_frames[usize::from(index)]);assert_eq!(layers[0].speed,if interaction==4 {2.}else{1.});turn_resources.insert(layers[0].metadata.resource.clone());
     }
     motion_json=json!({"phase":format!("{:?}",m.phase),"animation":format!("{:?}",m.animation),"velocity":m.velocity,"blend_velocity":m.blend_velocity,"line":m.line,"visual":m.visual});
    },_=>{}
   }
  }
  trace.push(json!({"tick":a.tick,"position":entity.info.position,"motion":motion_json,"layers":layers.iter().map(|l|json!({"resource":l.metadata.resource,"num_frames":l.metadata.num_frames,"frame":l.current_frame,"speed":l.speed,"weight":l.weight,"loop":l.looping,"ended":l.end_reached})).collect::<Vec<_>>() }));
  let snap=r.snapshot()?;restored.restore(&snap)?;assert_eq!(restored.state(),r.sim().state());
  if selected_success {
   assert!(route_success&&active_ticks>0&&walk_ticks>0&&turn_ticks>0&&fractional_ticks>0);
   assert!(phases.contains(&format!("{:?}",MotionPhase::BeginTurn)));
   assert_eq!((entity.info.position.x,entity.info.position.y,entity.info.position.level),(56,56,1));
   let pose=&state.source_routing.avatars[&actor];assert_eq!(pose.visual_position,[3.5,3.5,0.]);
   let standing=format!("{}.anim",r.sim().content().string(0,150,3).unwrap());assert_eq!(layers.len(),1);assert_eq!(layers[0].metadata.resource,standing);
   return Ok(json!({"runtime_descriptor":r.sim().content().descriptor()?,"interaction":interaction,"source_label":selected_label,"complete_tick":a.tick,"active_ticks":active_ticks,"walk_ticks":walk_ticks,"turn_ticks":turn_ticks,"fractional_position_ticks":fractional_ticks,"phases":phases,"walk_resources":walk_resources,"turn_resources":turn_resources,"source_action_succeeded":true,"route_succeeded":true,"replica_and_snapshot_equal_each_tick":true,"final_standing":standing,"trace":trace}));
  }
 }
 Err("source interaction did not complete within bounded probe".into())
}
fn main()->Result<(),Box<dyn std::error::Error>> {
 let args=std::env::args().skip(1).collect::<Vec<_>>();if args.len()!=4{return Err("release directory, expected binding SHA, original avatar GUID decimal, output JSON".into())}
 let binding=Digest::try_from(args[1].clone())?;let guid=args[2].parse()?;
 let walk=run_case(Path::new(&args[0]),&binding,2,guid)?;let run=run_case(Path::new(&args[0]),&binding,4,guid)?;
 assert!(run["complete_tick"].as_u64().unwrap()<walk["complete_tick"].as_u64().unwrap());
 let report=json!({"schema":"wonderland.private-source-motion-probe.v1","binding_sha256":binding,"simulation_schema":sim_core::state::SIMULATION_SCHEMA,"isolated_genuine_source_behaviors":true,"browser_rendered":false,"owned_accounts_verified":false,"walk":walk,"run":run});
 fs::write(&args[3],serde_json::to_vec_pretty(&report)?)?;
 println!("source Walk completed tick{}; Run tick{}; original turns, per-tick layers/continuous positions, replica and snapshot equality verified",report["walk"]["complete_tick"],report["run"]["complete_tick"]);Ok(())
}
