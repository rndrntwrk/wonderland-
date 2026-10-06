//! Source-ordered audio policy over normalized, authorized content.
use crate::{
    ambience::{AmbienceChange, AmbienceSelection, AMBIENCE},
    cue::{AudioCue, CueAction, CueAdmission, CueLedger},
    fsc::{Fsc, FscPlayer},
    hit::SampleRef,
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    runtime::{AudioRuntime, ResolvedKind, ThreadId},
    station::{code_from_track, mode_station, StationPlayer, STATIONS},
    AudioError, Result,
};
use std::{collections::BTreeMap, sync::Arc};
use wonderland_render_core::EntityRef;

#[derive(Clone, Debug)]
pub struct StationPlaylist {
    /// Source encounter order: commercials, then station tracks.
    pub samples: Vec<SampleRef>,
    /// Legacy MP3 player behavior: ignores pan, even in the FX group.
    pub music: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AudioContent {
    pub stations: BTreeMap<String, StationPlaylist>,
    pub ambience_loops: BTreeMap<u8, SampleRef>,
    pub fsc: BTreeMap<u8, Arc<Fsc>>,
    pub fsc_samples: BTreeMap<u8, BTreeMap<String, SampleRef>>,
}

struct OwnedStation {
    id: u64,
    name: String,
    player: StationPlayer,
    owners: Vec<EntityRef>,
    ever_owned: bool,
    volume_set: bool,
}
enum Route {
    Hit(ThreadId),
    Station(OwnedStation),
}
enum AmbientPlayer {
    Loop(VoiceId),
    Fsc(FscPlayer),
}

/// Tick at a fixed 60 Hz audio cadence, independently of rendering and A's
/// simulation clock. Loading/device callbacks never acknowledge simulation work.
pub struct AudioSystem {
    pub runtime: AudioRuntime,
    pub ledger: CueLedger,
    pub ambience: AmbienceSelection,
    content: AudioContent,
    routes: Vec<Route>,
    next_id: u64,
    music_current: Option<u64>,
    music_next: Option<OwnedStation>,
    ambient: BTreeMap<u8, AmbientPlayer>,
    ambient_order: Vec<u8>,
    faults: Vec<(String, AudioError)>,
}

impl AudioSystem {
    pub fn new(runtime: AudioRuntime, ledger: CueLedger, content: AudioContent) -> Result<Self> {
        if content.stations.len() > 23
            || content.ambience_loops.len() > 12
            || content.fsc.len() > 27
            || content.fsc_samples.len() > 27
        {
            return Err(AudioError::Limit("audio content families"));
        }
        let mut entries = content.ambience_loops.len();
        for (code, playlist) in &content.stations {
            if !STATIONS.iter().any(|s| s.code == code)
                || playlist.samples.len() > runtime.host.limits.hitlist_entries
            {
                return Err(AudioError::Invalid("station provider"));
            }
            entries = entries
                .checked_add(playlist.samples.len())
                .ok_or(AudioError::Limit("audio content entries"))?;
            for sample in &playlist.samples {
                validate_sample(sample)?;
            }
        }
        for (&bit, sample) in &content.ambience_loops {
            if AMBIENCE
                .get(usize::from(bit))
                .map_or(true, |e| e.category != 4)
            {
                return Err(AudioError::Invalid("ambience loop provider"));
            }
            validate_sample(sample)?;
        }
        for (&bit, fsc) in &content.fsc {
            if AMBIENCE
                .get(usize::from(bit))
                .map_or(true, |e| e.category == 4)
            {
                return Err(AudioError::Invalid("FSC provider"));
            }
            entries = entries
                .checked_add(fsc.notes.len())
                .ok_or(AudioError::Limit("audio content entries"))?;
        }
        for (&bit, samples) in &content.fsc_samples {
            if !content.fsc.contains_key(&bit) {
                return Err(AudioError::Invalid("orphan FSC sample manifest"));
            }
            entries = entries
                .checked_add(samples.len())
                .ok_or(AudioError::Limit("audio content entries"))?;
            for (name, sample) in samples {
                if name.is_empty()
                    || name.len() > 256
                    || !name.is_ascii()
                    || name.contains(['/', '\\', ':'])
                    || name == ".."
                {
                    return Err(AudioError::Invalid("FSC sample name"));
                }
                validate_sample(sample)?;
            }
        }
        if entries > runtime.host.limits.catalog_entries {
            return Err(AudioError::Limit("audio content entries"));
        }
        Ok(Self {
            runtime,
            ledger,
            ambience: AmbienceSelection::default(),
            content,
            routes: vec![],
            next_id: 0,
            music_current: None,
            music_next: None,
            ambient: BTreeMap::new(),
            ambient_order: vec![],
            faults: vec![],
        })
    }

    pub fn accept(&mut self, cue: &AudioCue) -> Result<CueAdmission> {
        let admission = self.ledger.check(&cue.id)?;
        if admission != CueAdmission::New {
            return Ok(admission);
        }
        match &cue.action {
            CueAction::StopOwner => {
                if let Some(owner) = cue.id.owner {
                    self.runtime.stop_owner(owner);
                    for route in &mut self.routes {
                        if let Route::Station(s) = route {
                            s.owners.retain(|o| *o != owner);
                        }
                    }
                    if let Some(s) = &mut self.music_next {
                        s.owners.retain(|o| *o != owner);
                    }
                }
            }
            CueAction::Play { event, looped } => {
                let rng = self.runtime.host.rng.clone();
                if let Err(error) = self.play(event, *looped, cue.id.owner) {
                    self.runtime.host.rng = rng;
                    return Err(error);
                }
            }
        }
        // The exclusive borrow prevents any ledger change between preflight
        // and this commit. Rejected resources/capacity consume no causal entry.
        self.ledger.admit(&cue.id)
    }

    fn play(&mut self, name: &str, looped: bool, owner: Option<EntityRef>) -> Result<()> {
        let resolved = self.runtime.bank.resolve(name, &self.runtime.host)?;
        match resolved.kind {
            ResolvedKind::Hit { .. } | ResolvedKind::Simple { .. } => {
                // VMPlaySound skips an owner's existing noninterruptible entry.
                // In particular, this must not erase SetLoop executed meanwhile.
                if owner.map_or(false, |o| {
                    self.runtime.is_owned_shared_event(&resolved.name, o)
                }) {
                    return Ok(());
                }
                let specials = self
                    .routes
                    .iter()
                    .filter(|r| matches!(r, Route::Station(_)))
                    .count()
                    + usize::from(self.music_next.is_some());
                let capacity = self.runtime.host.limits.threads.saturating_sub(specials);
                let id = self.runtime.play_with_limit(name, owner, capacity)?;
                if let Some(t) = self.runtime.thread_mut(id) {
                    let piano = name.eq_ignore_ascii_case("piano_play");
                    if !t.loop_defined || piano {
                        t.looped = looped || piano;
                        t.has_set_loop = piano;
                    }
                }
                // Interruptible replacement can retire an old waiter before the
                // next tick. Remove its route now so cue bursts remain bounded.
                let runtime = &self.runtime;
                self.routes.retain(|r| match r {
                    Route::Hit(id) => runtime.thread(*id).is_some(),
                    Route::Station(_) => true,
                });
                if !self
                    .routes
                    .iter()
                    .any(|r| matches!(r, Route::Hit(found) if *found == id))
                {
                    self.routes.push(Route::Hit(id));
                }
            }
            ResolvedKind::Station { track } => {
                if self.share_station(&resolved.name, owner)? {
                    return Ok(());
                }
                self.check_capacity(false)?;
                self.next_id
                    .checked_add(1)
                    .ok_or(AudioError::Limit("station serial"))?;
                let code = code_from_track(track)?;
                let playlist = self
                    .content
                    .stations
                    .get(&code)
                    .ok_or(AudioError::Missing("authorized station playlist"))?
                    .clone();
                let player = StationPlayer::new(
                    playlist.samples,
                    playlist.music,
                    VolumeGroup::Fx,
                    &mut self.runtime.host,
                )?;
                let station = self.owned_station(resolved.name, player, owner)?;
                self.routes.push(Route::Station(station));
            }
            ResolvedKind::Music { mode } => {
                if self.share_station(&resolved.name, owner)? {
                    return Ok(());
                }
                self.check_capacity(self.music_next.is_some())?;
                self.next_id
                    .checked_add(1)
                    .ok_or(AudioError::Limit("station serial"))?;
                let mut player = if mode == 5 {
                    let sample = self
                        .runtime
                        .host
                        .catalog
                        .samples
                        .get(&0x4f85)
                        .ok_or(AudioError::Missing("loadloop patch 0x4f85"))?
                        .clone();
                    let mut p = StationPlayer::new(
                        vec![sample],
                        false,
                        VolumeGroup::Music,
                        &mut self.runtime.host,
                    )?;
                    p.looped = true;
                    p
                } else {
                    let playlist = if let Some(code) = mode_station(mode)? {
                        self.content
                            .stations
                            .get(code)
                            .ok_or(AudioError::Missing("authorized music playlist"))?
                            .clone()
                    } else {
                        StationPlaylist {
                            samples: vec![],
                            music: true,
                        }
                    };
                    StationPlayer::new(
                        playlist.samples,
                        playlist.music,
                        VolumeGroup::Music,
                        &mut self.runtime.host,
                    )?
                };
                // Complete fallible eager admission before retiring/fading any
                // existing owner. The old pending start may exchange its queue
                // slot; an unrelated queued voice still produces a clean error.
                if mode == 5 {
                    let replacing = self
                        .music_next
                        .as_ref()
                        .and_then(|s| s.player.current_voice());
                    player.tick_replacing_pending(&mut self.runtime.host, replacing)?;
                }
                // Serial capacity was checked before constructing the player;
                // no preceding operation changes this counter.
                let next = self.owned_station(resolved.name, player, owner)?;
                if let Some(mut old) = self.music_next.take() {
                    old.player.kill(&mut self.runtime.host);
                }
                if let Some(id) = self.music_current {
                    for r in &mut self.routes {
                        if let Route::Station(s) = r {
                            if s.id == id {
                                s.player.fade();
                            }
                        }
                    }
                }
                self.music_next = Some(next);
            }
        }
        Ok(())
    }

    fn check_capacity(&self, replacing_pending: bool) -> Result<()> {
        let total = self.runtime.active_count()
            + self
                .routes
                .iter()
                .filter(|r| matches!(r, Route::Station(s) if !s.player.dead))
                .count()
            + usize::from(self.music_next.is_some());
        if total.saturating_sub(usize::from(replacing_pending)) >= self.runtime.host.limits.threads
        {
            return Err(AudioError::Limit("audio system players"));
        }
        Ok(())
    }

    fn owned_station(
        &mut self,
        name: String,
        player: StationPlayer,
        owner: Option<EntityRef>,
    ) -> Result<OwnedStation> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(AudioError::Limit("station serial"))?;
        Ok(OwnedStation {
            id: self.next_id,
            name,
            player,
            owners: owner.into_iter().collect(),
            ever_owned: owner.is_some(),
            volume_set: false,
        })
    }

    fn share_station(&mut self, name: &str, owner: Option<EntityRef>) -> Result<bool> {
        let found = self
            .routes
            .iter_mut()
            .find_map(|r| match r {
                Route::Station(s) if s.name == name && !s.player.dead => Some(s),
                _ => None,
            })
            .or_else(|| {
                self.music_next
                    .as_mut()
                    .filter(|s| s.name == name && !s.player.dead)
            });
        if let Some(sound) = found {
            if let Some(owner) = owner {
                if !sound.owners.contains(&owner) {
                    if sound.owners.len() >= self.runtime.host.limits.threads {
                        return Err(AudioError::Limit("station owners"));
                    }
                    sound.owners.push(owner);
                    sound.ever_owned = true;
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn tick(&mut self) -> Result<Vec<MixerIntent>> {
        self.runtime.begin_tick();
        let mut i = 0;
        while i < self.routes.len() {
            let dead = match &mut self.routes[i] {
                Route::Hit(id) => {
                    self.runtime.tick_thread(*id);
                    self.runtime.thread(*id).is_none()
                }
                Route::Station(sound) => {
                    if sound.ever_owned && sound.owners.is_empty() {
                        sound.player.kill(&mut self.runtime.host);
                    } else if let Err(error) = sound.player.tick(&mut self.runtime.host) {
                        sound.player.kill(&mut self.runtime.host);
                        if self.faults.len() < self.runtime.host.limits.threads.saturating_add(39) {
                            self.faults.push((sound.name.clone(), error));
                        }
                    }
                    sound.volume_set = false;
                    sound.player.dead
                }
            };
            if dead {
                if let Route::Station(s) = &self.routes[i] {
                    if self.music_current == Some(s.id) {
                        self.music_current = None;
                    }
                }
                self.routes.remove(i);
            } else {
                i += 1;
            }
        }
        let mut out = self.runtime.host.drain_intents();
        if self.music_current.is_none() {
            if let Some(mut next) = self.music_next.take() {
                if next.ever_owned && next.owners.is_empty() {
                    next.player.kill(&mut self.runtime.host);
                } else {
                    self.music_current = Some(next.id);
                    self.routes.push(Route::Station(next));
                }
            }
        }
        let mut failed = vec![];
        for &id in &self.ambient_order {
            if let Some(AmbientPlayer::Fsc(fsc)) = self.ambient.get_mut(&id) {
                let result = match self.content.fsc_samples.get(&id) {
                    Some(samples) => fsc.tick(1.0 / 60.0, &mut self.runtime.host, samples),
                    None => Err(AudioError::Missing("FSC samples")),
                };
                if let Err(error) = result {
                    fsc.stop(&mut self.runtime.host);
                    failed.push(id);
                    if self.faults.len() < self.runtime.host.limits.threads.saturating_add(39) {
                        self.faults.push((format!("ambience:{id}"), error));
                    }
                }
            }
        }
        for id in failed {
            self.ambient.remove(&id);
            self.ambient_order.retain(|bit| *bit != id);
        }
        // Failed FSC content must not discard starts already emitted by another
        // player. Faults are local diagnostics; cleanup remains ordered.
        out.extend(self.runtime.host.drain_intents());
        Ok(out)
    }

    pub fn set_ambience(&mut self, id: u8, enabled: bool) -> Result<()> {
        let mut selection = self.ambience.clone();
        let mut changes = selection.set(id, enabled)?;
        // Explicit reapplication may retry a failed presentation materialization.
        if enabled && changes.is_empty() && !self.ambient.contains_key(&id) {
            changes.push(AmbienceChange::Start(id));
        }
        let mut created = vec![];
        for change in &changes {
            if let AmbienceChange::Start(bit) = *change {
                let player = if AMBIENCE[usize::from(bit)].category == 4 {
                    let mut sample = self
                        .content
                        .ambience_loops
                        .get(&bit)
                        .ok_or(AudioError::Missing("authorized ambience loop"))?
                        .clone();
                    sample.group = VolumeGroup::Ambience;
                    AmbientPlayer::Loop(self.runtime.host.start(
                        &sample,
                        self.runtime.host.masters[3],
                        0.0,
                        true,
                    )?)
                } else {
                    let fsc = self
                        .content
                        .fsc
                        .get(&bit)
                        .ok_or(AudioError::Missing("authorized FSC"))?
                        .clone();
                    if !self.content.fsc_samples.contains_key(&bit) {
                        return Err(AudioError::Missing("FSC sample manifest"));
                    }
                    let mut p = FscPlayer::new(fsc, &mut self.runtime.host)?;
                    p.set_volume(0.33)?;
                    AmbientPlayer::Fsc(p)
                };
                created.push((bit, player));
            }
        }
        for change in changes {
            if let AmbienceChange::Stop(bit) = change {
                if let Some(player) = self.ambient.remove(&bit) {
                    self.stop_ambient(player);
                    self.ambient_order.retain(|id| *id != bit);
                }
            }
        }
        for (bit, player) in created {
            self.ambient.insert(bit, player);
            self.ambient_order.push(bit);
        }
        self.ambience = selection;
        Ok(())
    }

    fn stop_ambient(&mut self, player: AmbientPlayer) {
        match player {
            AmbientPlayer::Loop(voice) => self.runtime.host.stop_release(voice),
            AmbientPlayer::Fsc(mut p) => p.stop(&mut self.runtime.host),
        }
    }

    pub fn stop_all(&mut self) -> Vec<MixerIntent> {
        self.runtime.stop_all();
        for route in self.routes.drain(..) {
            if let Route::Station(mut s) = route {
                s.player.kill(&mut self.runtime.host);
            }
        }
        if let Some(mut next) = self.music_next.take() {
            next.player.kill(&mut self.runtime.host);
        }
        self.music_current = None;
        let ambient = std::mem::take(&mut self.ambient);
        for (_, player) in ambient {
            self.stop_ambient(player);
        }
        self.ambience.bits = 0;
        self.ambient_order.clear();
        self.runtime.host.drain_intents()
    }

    /// Backend feedback releases presentation resources only.
    pub fn complete_voice(&mut self, voice: VoiceId) -> bool {
        if self.runtime.complete_voice(voice) {
            return true;
        }
        for route in &mut self.routes {
            if let Route::Station(s) = route {
                if s.player.complete_voice(voice) {
                    return true;
                }
            }
        }
        if self
            .music_next
            .as_mut()
            .map_or(false, |s| s.player.complete_voice(voice))
        {
            return true;
        }
        let loop_bit = self.ambient.iter().find_map(|(&id, player)| match player {
            AmbientPlayer::Loop(active) if *active == voice => Some(id),
            _ => None,
        });
        if let Some(bit) = loop_bit {
            self.ambient.remove(&bit);
            self.ambient_order.retain(|id| *id != bit);
            self.runtime.host.stop_release(voice);
            return true;
        }
        for player in self.ambient.values_mut() {
            if let AmbientPlayer::Fsc(p) = player {
                if p.complete_voice(voice, &mut self.runtime.host) {
                    return true;
                }
            }
        }
        false
    }

    /// Maximum owner gain wins per tick; the first equal-gain owner retains pan.
    pub fn submit_volume(
        &mut self,
        event: &str,
        owner: EntityRef,
        gain: f32,
        pan: f32,
        objects: Option<Vec<i32>>,
    ) -> Result<()> {
        crate::pcm::validate_gain_pan(gain, pan)?;
        if objects.as_ref().map_or(false, |o| o.len() > 29) {
            return Err(AudioError::Limit("owner audio fields"));
        }
        let name = self.runtime.bank.resolve(event, &self.runtime.host)?.name;
        if let Some(id) = self.runtime.event_thread(&name) {
            return self.runtime.submit_volume(id, owner, gain, pan, objects);
        }
        let sound = self
            .routes
            .iter_mut()
            .find_map(|r| match r {
                Route::Station(s) if s.name == name && !s.player.dead => Some(s),
                _ => None,
            })
            .or_else(|| {
                self.music_next
                    .as_mut()
                    .filter(|s| s.name == name && !s.player.dead)
            })
            .ok_or(AudioError::Stale)?;
        if !sound.owners.contains(&owner) {
            return Err(AudioError::Stale);
        }
        if !sound.volume_set || gain > sound.player.gain {
            sound.player.gain = gain;
            sound.player.pan = pan;
        }
        sound.volume_set = true;
        Ok(())
    }

    pub fn reconcile_owners(&mut self, owners: &[EntityRef]) -> Result<()> {
        self.runtime.reconcile_owners(owners)?;
        let live: std::collections::BTreeSet<_> = owners.iter().copied().collect();
        for route in &mut self.routes {
            if let Route::Station(s) = route {
                s.owners.retain(|o| live.contains(o));
            }
        }
        if let Some(s) = &mut self.music_next {
            s.owners.retain(|o| live.contains(o));
        }
        Ok(())
    }

    pub fn take_faults(&mut self) -> Vec<(String, AudioError)> {
        std::mem::take(&mut self.faults)
    }

    pub fn set_master(&mut self, group: VolumeGroup, gain: f32) -> Result<()> {
        self.runtime.set_master(group, gain)?;
        if group == VolumeGroup::Ambience {
            for player in self.ambient.values() {
                if let AmbientPlayer::Loop(voice) = player {
                    self.runtime.host.intents.push(MixerIntent::SetGainPan {
                        voice: *voice,
                        gain,
                        pan: 0.0,
                    });
                }
            }
        }
        Ok(())
    }
}

fn validate_sample(sample: &SampleRef) -> Result<()> {
    if sample.frames == 0 || sample.sample_rate == 0 || sample.sample_rate > 384_000 {
        return Err(AudioError::Invalid("sample duration"));
    }
    Ok(())
}
