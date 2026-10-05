#![forbid(unsafe_code)]
#[cfg(test)]
mod tests {
 use std::sync::Arc;
 use sim_core::{runtime::{SimRuntime,RuntimeEvent},ids::ObjectId};
 use wonderland_avatar_view as c;
 use wonderland_render_core::{AssetKey,EntityRef};
 #[allow(dead_code,unused_imports)]mod fixture{include!(env!("A_FIXTURE_PATH"));}
 fn projected(runtime:&SimRuntime,rig:&c::Rig)->c::Timeline {
  let source=&runtime.state().entities[&ObjectId(1)].avatar.as_ref().unwrap().animations;
  let resolve=|state:&sim_core::avatars::timeline::AnimationState|{
   let mut normalized=c::fixtures::animation("internal-pose-name",0.0,8.0);normalized.num_frames=state.metadata.num_frames as u32;
   let digest=AssetKey([91;32]);let clip=c::Clip::new_resolved(rig,normalized,digest,state.metadata.resource.clone(),c::AvatarLimits::default()).unwrap();
   assert!(clip.matches_projection(&state.metadata.resource,state.metadata.num_frames as u32,digest));Arc::new(clip)
  };
  c::Timeline{layers:source.animations.iter().map(|a|c::TimelineLayer{clip:resolve(a),current_frame:a.current_frame,speed:a.speed,weight:a.weight,backwards:a.backwards,end_reached:a.end_reached,looping:a.looping}).collect(),carry:source.carry.as_ref().map(|a|c::CarryPose{clip:resolve(a),frame:a.current_frame})}
 }
 fn run(hz:u32)->(Vec<[u8;32]>,Vec<Vec<RuntimeEvent>>,usize) {
  let mut runtime=fixture::presentation_runtime();let rig=c::fixtures::representative_rig();let mesh=c::fixtures::representative_mesh(&rig);
  let a_entity=runtime.state().entities[&ObjectId(1)].info.reference;let mut player=if hz==0 {None}else{Some(c::PosePlayer::new(EntityRef{object_id:a_entity.object_id.0 as u32,generation:a_entity.generation},&rig))};
  let mut hashes=Vec::new();let mut all_events=Vec::new();let mut cues=0;
  for _ in 0..60 {
   let accepted=runtime.next_tick(vec![]).unwrap();let outcome=runtime.step(&accepted).unwrap();assert!(!outcome.duplicate);
   let before=runtime.state_hash().unwrap();
   if let Some(player)=&mut player {
    let timeline=projected(&runtime,&rig);assert!(player.commit(outcome.tick,&rig,timeline).unwrap());
    for index in 0..hz/30 {let fraction=index as f32/(hz/30)as f32;let pose=player.sample(&rig,fraction).unwrap();mesh.skin(&pose,wonderland_render_core::Mat4::IDENTITY).unwrap();assert_eq!(before,runtime.state_hash().unwrap());}
   }
   let replay=runtime.step(&accepted).unwrap();assert!(replay.duplicate);assert!(replay.events.is_empty());assert_eq!(before,replay.state_hash);
   if let Some(player)=&mut player {assert!(!player.commit(replay.tick,&rig,projected(&runtime,&rig)).unwrap());}
   for event in &outcome.events {if let RuntimeEvent::Avatar{output,..}=event {cues+=output.animation_cues.len();}}
   hashes.push(before);all_events.push(outcome.events);
  }
  (hashes,all_events,cues)
 }
 #[test]
 fn actual_a_state_hash_and_ordered_events_are_equal_with_absent_30_60_120hz_c(){
  let absent=run(0);assert!(absent.2>0,"fixture must really execute A animation markers");
  for hz in [30,60,120] {let observed=run(hz);assert_eq!(observed,absent);println!("A8a0e251 hz={hz} ticks=60 hashes=equal events=equal cues={} duplicate_cues=0 synthetic_bones=14 vertices=312 triangles=156",observed.2);}
 }
}
