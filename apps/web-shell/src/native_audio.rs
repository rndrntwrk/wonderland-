//! Presentation-only native sound consumer. Receipts and historical checkpoints
//! never synthesize audio, and audio completion never changes the simulation.
use std::{collections::BTreeMap, sync::Arc};
use wonderland_audio_content::{pack::AudioPack, session::AcceptedAudioSession};
use wonderland_audio_content::{
    pack::guid_scope,
    position::{CameraAudio, source_gain_pan},
};
use wonderland_audio_runtime::{
    cue::{AudioCue, CueAction, CueAdmission, CueId},
    mixer::MixerIntent,
    projection::{SoundRequest, project_request},
};
use wonderland_game_runtime::{
    RuntimeEvent,
    sim_core::{avatars::events::AnimationCue, vm::ExternalKind},
};
use wonderland_game_runtime::{RuntimeProjection, TickOutcome};
use wonderland_render_core::EntityRef;
use wonderland_world_view::{ViewportControls, WorldDocument};
pub struct NativeAudio {
    pub session: AcceptedAudioSession,
    pack: Arc<AudioPack>,
    lot: u64,
    timeline: u64,
    last_tick: Option<u64>,
    owners: BTreeMap<(EntityRef, String), (bool, bool)>,
    pub notice: Option<String>,
}
impl NativeAudio {
    pub fn new(
        pack: Arc<AudioPack>,
        lot: u64,
        timeline: u64,
        generation: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            session: pack.session(lot, timeline, generation)?,
            pack,
            lot,
            timeline,
            last_tick: None,
            owners: BTreeMap::new(),
            notice: None,
        })
    }
    pub fn checkpoint(&mut self, tick: u64) -> Vec<MixerIntent> {
        let stops = self.stop();
        let watermark = self.last_tick.map_or(tick, |old| old.max(tick));
        self.last_tick = Some(watermark);
        let _ = self.session.system.ledger.retire_through(watermark);
        stops
    }
    pub fn accept(
        &mut self,
        outcomes: &[TickOutcome],
        projection: &RuntimeProjection,
        audible: bool,
    ) -> Result<(), String> {
        if projection.lot_id != self.lot
            || projection.epoch != self.timeline
            || outcomes.len() > 256
            || outcomes
                .iter()
                .any(|o| o.tick > projection.tick || o.events.len() > 65536)
            || outcomes.windows(2).any(|p| p[0].tick > p[1].tick)
        {
            return Err("Audio update does not match the accepted timeline".into());
        }
        let live = projection
            .entities
            .iter()
            .filter_map(|e| owner(e.reference))
            .collect::<std::collections::BTreeSet<_>>();
        self.session
            .reconcile_owners(&live.iter().copied().collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
        self.owners.retain(|(id, _), _| live.contains(id));
        for outcome in outcomes {
            if outcome.duplicate || self.last_tick.is_some_and(|t| outcome.tick <= t) {
                continue;
            }
            if audible {
                for (ordinal, event) in outcome.events.iter().enumerate() {
                    let id = CueId {
                        lot_id: self.lot,
                        timeline: self.timeline,
                        tick: outcome.tick,
                        event_ordinal: ordinal as u32,
                        nested_ordinal: 0,
                        owner: None,
                    };
                    match event {
                        RuntimeEvent::Avatar { entity, output } => {
                            let Some(entity) = owner(*entity).filter(|e| live.contains(e)) else {
                                continue;
                            };
                            if output.animation_cues.len() > 4096 {
                                self.notice =
                                    Some("Animation sound property limit exceeded".into());
                                continue;
                            }
                            for (nested, cue) in output.animation_cues.iter().enumerate() {
                                if let AnimationCue::Sound(event) = cue {
                                    let mut id = id.clone();
                                    id.owner = Some(entity);
                                    id.nested_ordinal = nested as u32;
                                    self.cue(
                                        AudioCue {
                                            id,
                                            action: CueAction::Play {
                                                event: event.clone(),
                                                looped: false,
                                            },
                                        },
                                        false,
                                        false,
                                    );
                                }
                            }
                        }
                        RuntimeEvent::Presentation(request)
                            if request.kind == ExternalKind::Sound && !request.is_check =>
                        {
                            let Some(caller) = owner(request.context.caller) else {
                                continue;
                            };
                            let request = SoundRequest {
                                opcode: request.opcode,
                                operand: request.operand,
                                caller,
                                stack: request.context.stack_object_ref.and_then(owner),
                                scope: guid_scope(request.context.code_owner),
                            };
                            match project_request(id, &request, &self.pack.fwav) {
                                Ok(Some(sound))
                                    if sound.cue.id.owner.is_some_and(|e| live.contains(&e)) =>
                                {
                                    self.cue(sound.cue, sound.no_pan, sound.no_zoom)
                                }
                                Ok(None) => {
                                    self.notice = Some(
                                        "An accepted sound has no matching imported FWAV resource"
                                            .into(),
                                    )
                                }
                                Ok(Some(_)) => {}
                                Err(e) => {
                                    self.notice = Some(format!("Sound event unavailable: {e}"))
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Missing resources, locked devices and content changes never backlog cues.
            self.last_tick = Some(outcome.tick);
            self.session
                .system
                .ledger
                .retire_through(outcome.tick)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn cue(&mut self, cue: AudioCue, no_pan: bool, no_zoom: bool) {
        if let CueAction::Play { event, .. } = &cue.action
            && let Some(owner) = cue.id.owner
            && !self.owners.contains_key(&(owner, event.clone()))
            && self.owners.len() >= 512
        {
            self.notice = Some("Positional sound owner limit reached".into());
            return;
        }
        match self.session.accept(&cue) {
            Ok(CueAdmission::New) => match cue.action {
                CueAction::Play { event, .. } => {
                    if let Some(owner) = cue.id.owner {
                        if self.owners.len() < 512 {
                            self.owners.insert((owner, event), (no_pan, no_zoom));
                        } else {
                            self.notice = Some("Positional sound owner limit reached".into());
                        }
                    }
                }
                CueAction::StopOwner => self
                    .owners
                    .retain(|(owner, _), _| Some(*owner) != cue.id.owner),
            },
            Ok(_) => {}
            Err(e) => self.notice = Some(format!("Accepted sound unavailable: {e}")),
        }
    }
    /// Reuse the displayed orbit camera and original gain/pan equations. This never
    /// reads a device clock into simulation state, nor guesses a source object ID.
    pub fn spatial(&mut self, world: &WorldDocument, controls: ViewportControls, aspect: f32) {
        let Ok(camera) = wonderland_world_view::orbit_camera(world, controls, aspect) else {
            return;
        };
        let Ok(pose) = camera.pose() else {
            return;
        };
        self.owners.retain(|(owner, event), (no_pan, no_zoom)| {
            let Some(object) = world.objects.iter().find(|o| o.entity == Some(*owner)) else {
                return false;
            };
            let p = object.position_tiles;
            let calculation = CameraAudio::ThreeD {
                visual_position: [p.x, p.z, p.y],
                target: [pose.target.x, pose.target.y, pose.target.z],
                position: [pose.position.x, pose.position.y, pose.position.z],
                zoom_3d: camera.zoom,
                precise_zoom: 1.,
            };
            let Ok((gain, pan)) = source_gain_pan(
                calculation,
                object.level as i8,
                controls.visible_level as i8,
                *no_pan,
                *no_zoom,
            ) else {
                return false;
            };
            self.session
                .system
                .submit_volume(event, *owner, gain, pan, None)
                .is_ok()
        });
    }
    pub fn stop(&mut self) -> Vec<MixerIntent> {
        self.owners.clear();
        self.session.stop_all()
    }
}

fn owner(entity: wonderland_game_runtime::EntityRef) -> Option<EntityRef> {
    entity.is_valid().then_some(EntityRef {
        object_id: entity.object_id.0 as u32,
        generation: entity.generation,
    })
}

/// One explicit manifest in the selected local cohort. Directory selections
/// resolve paths relative to that manifest, never to the network or filesystem.
pub fn selected_audio_pack(files: &[(String, Vec<u8>)]) -> Result<Option<Arc<AudioPack>>, String> {
    let manifests = files
        .iter()
        .filter(|(name, _)| name.rsplit('/').next() == Some("wonderland-audio.json"))
        .collect::<Vec<_>>();
    if manifests.is_empty() {
        return Ok(None);
    }
    if manifests.len() != 1 {
        return Err("Select exactly one wonderland-audio.json manifest".into());
    }
    let (name, bytes) = manifests[0];
    if bytes.len() > 128 * 1024 {
        return Err("Audio manifest exceeds 128 KiB".into());
    }
    let spec =
        serde_json::from_slice(bytes).map_err(|e| format!("Audio manifest is invalid: {e}"))?;
    let prefix = name.rsplit_once('/').map_or("", |(parent, _)| parent);
    AudioPack::load_in_folder(spec, files, prefix).map(|p| Some(Arc::new(p)))
}
