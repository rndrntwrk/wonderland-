//! Shared control plane. Engine objects remain private and non-authoritative.
use serde::{Deserialize, Serialize};
// Shared by the Rust 1.75 harness and edition-2024 engines; those rustfmt
// editions sort mixed type/function imports differently. Keep 1.75 ordering.
#[rustfmt::skip]
use wonderland_engine_fixture::{hash_hex, representative_scene, FixtureScene};
#[rustfmt::skip]
use wonderland_render_core::{
    frame::{FrameStore, PickTicket},
    EntityRef, RenderLimits, ViewMode,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub mode: String,
    pub avatars: u16,
    pub tick: u64,
    pub frames: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "hybrid2d".into(),
            avatars: 32,
            tick: 30,
            frames: 0,
        }
    }
}
pub fn mode(value: &str) -> Result<ViewMode, String> {
    match value {
        "full2d" => Ok(ViewMode::Full2D),
        "hybrid2d" => Ok(ViewMode::Hybrid2D),
        "full3d" => Ok(ViewMode::Full3D),
        _ => Err(format!("unknown view mode {value}")),
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    SetMode { mode: String },
    SetTick { tick: u64 },
    SelectAt { x: u32, y: u32 },
    SetPass { pass: String },
    Suspend,
    Resume,
    SimulateLoss,
    DeviceLost,
    ReloadFixture,
}
#[derive(Debug, Deserialize)]
pub struct Command {
    pub seq: u64,
    #[serde(flatten)]
    pub action: Action,
}

/// Disposable GPU work identity, never a game identity. Coordinates use the
/// fixed 640x480 fixture grid, origin at the top left, independent of canvas DPR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuPickRequest {
    pub serial: u64,
    pub command: u64,
    pub revision: u64,
    pub x: u32,
    pub y: u32,
}
struct PendingPick {
    request: GpuPickRequest,
    tickets: Vec<Option<PickTicket>>,
}

pub struct State {
    pub config: Config,
    pub scene: FixtureScene,
    pub selected: Option<EntityRef>,
    pub pick_pass: bool,
    pub suspended: bool,
    pub update_count: u64,
    pub revision: u64,
    pub last_command: u64,
    pub errors: Vec<String>,
    pub ids: Vec<EntityRef>,
    pub pick_completed_command: u64,
    pub pick_state: &'static str,
    pub pick_error: Option<String>,
    pub selection_source: &'static str,
    pending_pick: Option<PendingPick>,
    pick_serial: u64,
    store: FrameStore,
}
impl State {
    pub fn new(config: Config) -> Result<Self, String> {
        if config.avatars == 0 || config.avatars > 64 {
            return Err("avatars must be 1..64".into());
        }
        let scene = representative_scene(mode(&config.mode)?, config.avatars, config.tick)
            .map_err(|e| e.to_string())?;
        let mut store = FrameStore::new(RenderLimits::default());
        store.reset(scene.frame.stamp.lot_id, scene.frame.stamp.epoch);
        store
            .admit(scene.frame.clone())
            .map_err(|e| e.to_string())?;
        let ids = scene.frame.entities.iter().map(|e| e.reference).collect();
        Ok(Self {
            selected: scene.frame.selected,
            config,
            scene,
            pick_pass: false,
            suspended: false,
            update_count: 0,
            revision: 1,
            last_command: 0,
            errors: Vec::new(),
            ids,
            store,
            pick_completed_command: 0,
            pick_state: "idle",
            pick_error: None,
            selection_source: "immutable-fixture-initial-selection",
            pending_pick: None,
            pick_serial: 0,
        })
    }
    pub fn pick_index(&self, owner: Option<EntityRef>) -> u32 {
        owner
            .and_then(|id| self.ids.iter().position(|i| *i == id))
            .map_or(0, |i| i as u32 + 1)
    }
    pub fn pending_gpu_pick(&self) -> Option<GpuPickRequest> {
        self.pending_pick.as_ref().map(|pending| pending.request)
    }
    fn cancel_gpu_pick(&mut self, reason: &str) {
        if let Some(pending) = self.pending_pick.take() {
            self.pick_completed_command = pending.request.command;
            self.pick_state = "cancelled";
            self.pick_error = Some(reason.into());
        }
    }
    /// Admit only bytes returned by the adapter's offscreen GPU pass. The table
    /// was frozen when the request was made; completion never makes new tickets.
    pub fn complete_gpu_pick(
        &mut self,
        request: GpuPickRequest,
        pixel: Result<[u8; 4], String>,
    ) -> bool {
        if self.pending_gpu_pick() != Some(request) || self.suspended {
            return false;
        }
        let pending = self.pending_pick.take().unwrap();
        self.pick_completed_command = request.command;
        let decoded = pixel.and_then(|pixel| {
            if pixel[3] != 255 {
                return Err("GPU ID pixel is not opaque RGBA8".into());
            }
            let index = u32::from(pixel[0]) | u32::from(pixel[1]) << 8 | u32::from(pixel[2]) << 16;
            if index as usize > pending.tickets.len() {
                return Err("GPU ID index exceeds request table".into());
            }
            Ok(index)
        });
        let index = match decoded {
            Ok(index) => index,
            Err(error) => {
                self.pick_state = "failed";
                self.pick_error = Some(error);
                return false;
            }
        };
        let selected = if index == 0 {
            None
        } else if let Some(ticket) = &pending.tickets[index as usize - 1] {
            let Some(reference) = self.store.resolve_pick(ticket) else {
                self.pick_state = "stale";
                self.pick_error =
                    Some("GPU pick identity changed while readback was pending".into());
                return false;
            };
            Some(reference)
        } else {
            self.pick_state = "failed";
            self.pick_error = Some("GPU ID has no eligible request-time ticket".into());
            return false;
        };
        self.selected = selected;
        self.pick_state = "completed";
        self.pick_error = None;
        self.selection_source = "gpu-id-readback-generation-checked";
        true
    }
    pub fn apply(&mut self, command: Command) -> bool {
        self.last_command = command.seq;
        match self.try_apply(command.action) {
            Ok(rebuild) => rebuild,
            Err(error) => {
                if self.errors.len() == 32 {
                    self.errors.remove(0);
                }
                self.errors.push(error);
                false
            }
        }
    }
    fn try_apply(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::SelectAt { x, y } => {
                if x >= 640 || y >= 480 {
                    return Err("pick outside 640x480 fixture".into());
                }
                if self.suspended {
                    return Err("pick is suspended".into());
                }
                let serial = self
                    .pick_serial
                    .checked_add(1)
                    .ok_or("GPU pick serial exhausted")?;
                self.cancel_gpu_pick("Superseded by a newer selection");
                self.pick_serial = serial;
                self.pending_pick = Some(PendingPick {
                    request: GpuPickRequest {
                        serial,
                        command: self.last_command,
                        revision: self.revision,
                        x,
                        y,
                    },
                    tickets: self
                        .ids
                        .iter()
                        .map(|id| self.store.pick_ticket(*id))
                        .collect(),
                });
                self.pick_state = "pending";
                self.pick_error = None;
                Ok(false)
            }
            Action::SetPass { pass } => {
                self.pick_pass = match pass.as_str() {
                    "color" => false,
                    "pick" => true,
                    _ => return Err("pass must be color or pick".into()),
                };
                Ok(false)
            }
            Action::Suspend => {
                self.cancel_gpu_pick("Presentation suspended");
                self.suspended = true;
                Ok(false)
            }
            Action::Resume => {
                self.suspended = false;
                Ok(false)
            }
            Action::SimulateLoss | Action::DeviceLost => {
                self.cancel_gpu_pick("Graphics device lifetime ended");
                self.store.device_reset();
                self.suspended = true;
                Ok(false)
            }
            action => {
                let mut candidate = self.config.clone();
                match action {
                    Action::SetMode { mode: view } => {
                        mode(&view)?;
                        candidate.mode = view;
                    }
                    Action::SetTick { tick } => candidate.tick = tick,
                    Action::ReloadFixture => (),
                    _ => unreachable!(),
                }
                let next =
                    representative_scene(mode(&candidate.mode)?, candidate.avatars, candidate.tick)
                        .map_err(|e| e.to_string())?;
                self.cancel_gpu_pick("Displayed fixture replaced");
                // A user-requested fixture change is an explicit reset boundary. No wall clock advances it.
                let mut store = FrameStore::new(RenderLimits::default());
                store.reset(next.frame.stamp.lot_id, next.frame.stamp.epoch);
                store.admit(next.frame.clone()).map_err(|e| e.to_string())?;
                self.selected = self.selected.filter(|id| store.entity(*id).is_some());
                self.ids = next.frame.entities.iter().map(|e| e.reference).collect();
                // Keep reset_generation monotonic even when reloading byte-identical input.
                self.store
                    .reset(next.frame.stamp.lot_id, next.frame.stamp.epoch);
                self.store
                    .admit(next.frame.clone())
                    .map_err(|e| e.to_string())?;
                self.scene = next;
                self.config = candidate;
                self.revision += 1;
                Ok(true)
            }
        }
    }
    pub fn publish(&self, engine: &str, render_visits: u64) {
        let ids:Vec<_>=self.ids.iter().enumerate().map(|(i,id)|serde_json::json!({"index":i+1,"objectId":id.object_id,"generation":id.generation})).collect();
        let value = serde_json::json!({
            "engine":engine,"ready":true,"readiness":"scene-uploaded",
            "sceneHash":hash_hex(self.scene.hash),"mode":self.config.mode,"avatars":self.config.avatars,
            "tick":self.config.tick,"selection":self.selected,"selectionSource":self.selection_source,
            "pickState":self.pick_state,"pickPendingCommand":self.pending_gpu_pick().map(|request|request.command),
            "pickCompletedCommand":self.pick_completed_command,"pickError":self.pick_error,
            "pass":if self.pick_pass {"pick"} else {"color"},"idMap":ids,
            "suspended":self.suspended,"updateCount":self.update_count,"renderScheduleVisits":render_visits,
            "fixtureRevision":self.revision,"lastCommand":self.last_command,"engineErrors":self.errors,
            "meshCount":self.scene.draws.len(),"spriteCount":self.scene.sprites.len(),
            "vertexCount":self.scene.draws.iter().map(|d|d.mesh.vertices.len()).sum::<usize>(),
            "gpuReadback":"offscreen-id-rgba8-async","authoritativeTicksAdvanced":0
        });
        publish(&value.to_string());
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(
    inline_js = "export function config_json(){return JSON.stringify(window.__wonderlandHost.config)} export function drain_json(){return JSON.stringify(window.__wonderlandHost.drain())} export function publish_json(s){window.__wonderlandHost.publish(JSON.parse(s))} export function fail_json(s){window.__wonderlandHost.fail(s)}"
)]
extern "C" {
    fn config_json() -> String;
    fn drain_json() -> String;
    fn publish_json(value: &str);
    fn fail_json(value: &str);
}
pub fn initial_config() -> Result<Config, String> {
    #[cfg(target_arch = "wasm32")]
    {
        serde_json::from_str(&config_json()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut config = Config::default();
        config.frames = std::env::var("WONDERLAND_PROBE_FRAMES")
            .ok()
            .map(|s| s.parse())
            .transpose()
            .map_err(|_| "invalid WONDERLAND_PROBE_FRAMES")?
            .unwrap_or(0);
        let mut args = std::env::args().skip(1);
        while let Some(key) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {key}"))?;
            match key.as_str() {
                "--mode" => config.mode = value,
                "--avatars" => config.avatars = value.parse().map_err(|_| "invalid avatars")?,
                "--tick" => config.tick = value.parse().map_err(|_| "invalid tick")?,
                "--frames" => config.frames = value.parse().map_err(|_| "invalid frames")?,
                _ => return Err(format!("unknown argument {key}")),
            }
        }
        Ok(config)
    }
}
pub fn commands() -> Vec<Command> {
    #[cfg(target_arch = "wasm32")]
    {
        match serde_json::from_str(&drain_json()) {
            Ok(c) => c,
            Err(e) => {
                fail(&e.to_string());
                Vec::new()
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}
fn publish(value: &str) {
    #[cfg(target_arch = "wasm32")]
    publish_json(value);
    #[cfg(not(target_arch = "wasm32"))]
    println!("WONDERLAND_PROBE {value}");
}
/// Partial runtime observations; the browser host merges these with fixture state.
pub fn metrics(value: serde_json::Value) {
    #[cfg(target_arch = "wasm32")]
    publish_json(&value.to_string());
    #[cfg(not(target_arch = "wasm32"))]
    println!("WONDERLAND_ENGINE_METRICS {value}");
}

pub fn fail(value: &str) {
    #[cfg(target_arch = "wasm32")]
    fail_json(value);
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("WONDERLAND_PROBE_ERROR {value}");
}

pub fn fatal(value: &str) -> ! {
    fail(value);
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen::throw_str(value);
    #[cfg(not(target_arch = "wasm32"))]
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_waits_for_gpu_completion_without_changing_the_visible_pass() {
        let mut state = State::new(Config::default()).unwrap();
        let selected = state.selected;
        assert!(!state.apply(Command {
            seq: 7,
            action: Action::SelectAt { x: 0, y: 0 },
        }));
        assert_eq!(
            state.selected, selected,
            "a queued selection must not resolve on the CPU"
        );
        assert!(
            !state.pick_pass,
            "selection must never flash an ID pass onscreen"
        );
    }
    #[test]
    fn gpu_zero_clears_selection_and_exact_rgb_id_resolves_the_request_ticket() {
        let mut state = State::new(Config::default()).unwrap();
        state.apply(Command {
            seq: 7,
            action: Action::SelectAt { x: 11, y: 19 },
        });
        let request = state.pending_gpu_pick().unwrap();
        assert_eq!((request.command, request.x, request.y), (7, 11, 19));
        assert!(state.complete_gpu_pick(request, Ok([0, 0, 0, 255])));
        assert_eq!(state.selected, None);
        assert_eq!(state.pick_completed_command, 7);
        state.apply(Command {
            seq: 8,
            action: Action::SelectAt { x: 0, y: 0 },
        });
        let request = state.pending_gpu_pick().unwrap();
        assert!(state.complete_gpu_pick(request, Ok([1, 0, 0, 255])));
        assert_eq!(state.selected, Some(state.ids[0]));
        assert_eq!(state.selection_source, "gpu-id-readback-generation-checked");
        assert!(
            !state.complete_gpu_pick(request, Ok([0, 0, 0, 255])),
            "completion is one-shot"
        );
        assert_eq!(state.selected, Some(state.ids[0]));
    }
    #[test]
    fn newer_selection_cancels_old_completion_without_consuming_the_new_request() {
        let mut state = State::new(Config::default()).unwrap();
        state.apply(Command {
            seq: 1,
            action: Action::SelectAt { x: 0, y: 0 },
        });
        let old = state.pending_gpu_pick().unwrap();
        state.apply(Command {
            seq: 2,
            action: Action::SelectAt { x: 50, y: 100 },
        });
        let next = state.pending_gpu_pick().unwrap();
        assert!(!state.complete_gpu_pick(old, Ok([0, 0, 0, 255])));
        assert_eq!(state.pending_gpu_pick(), Some(next));
        assert!(state.complete_gpu_pick(next, Ok([2, 0, 0, 255])));
        assert_eq!(state.selected, Some(state.ids[1]));
    }
    #[test]
    fn every_display_lifetime_boundary_cancels_pending_pick_including_no_hit() {
        for action in [
            Action::ReloadFixture,
            Action::SetTick { tick: 31 },
            Action::SetMode {
                mode: "full3d".into(),
            },
            Action::Suspend,
            Action::SimulateLoss,
            Action::DeviceLost,
        ] {
            let mut state = State::new(Config::default()).unwrap();
            let selected = state.selected;
            state.apply(Command {
                seq: 1,
                action: Action::SelectAt { x: 0, y: 0 },
            });
            let request = state.pending_gpu_pick().unwrap();
            state.apply(Command { seq: 2, action });
            assert_eq!(state.pending_gpu_pick(), None);
            assert!(!state.complete_gpu_pick(request, Ok([0, 0, 0, 255])));
            assert_eq!(state.selected, selected);
        }
    }
    #[test]
    fn content_aba_and_entity_replacement_cannot_resolve_request_time_tickets() {
        for change in 0..4 {
            let mut state = State::new(Config::default()).unwrap();
            state.apply(Command {
                seq: 1,
                action: Action::SelectAt { x: 0, y: 0 },
            });
            let request = state.pending_gpu_pick().unwrap();
            let selected = state.selected;
            let mut next = state.store.current().unwrap().clone();
            next.stamp.tick += 1;
            match change {
                0 => next.entities[0].reference.generation += 1,
                1 => next.entities[0].visual_revision += 1,
                2 => {
                    next.entities[0].visible = false;
                    next.entities[0].visual_revision += 1;
                }
                _ => next.stamp.content.0[0] ^= 1,
            }
            next.selected = None;
            state.store.admit(next.clone()).unwrap();
            if change == 3 {
                next.stamp.tick += 1;
                next.stamp.content.0[0] ^= 1;
                state.store.admit(next).unwrap();
            }
            assert!(!state.complete_gpu_pick(request, Ok([1, 0, 0, 255])));
            assert_eq!(state.selected, selected);
            assert_eq!(state.pick_state, "stale");
        }
    }
    #[test]
    fn invalid_gpu_bytes_and_read_errors_preserve_selection() {
        for pixel in [
            Ok([255, 255, 255, 255]),
            Ok([1, 0, 0, 0]),
            Err("device lost".into()),
        ] {
            let mut state = State::new(Config::default()).unwrap();
            let selected = state.selected;
            state.apply(Command {
                seq: 3,
                action: Action::SelectAt { x: 0, y: 0 },
            });
            let request = state.pending_gpu_pick().unwrap();
            assert!(!state.complete_gpu_pick(request, pixel));
            assert_eq!(state.selected, selected);
            assert_eq!(state.pick_state, "failed");
            assert_eq!(state.pick_completed_command, 3);
            assert!(state.pick_error.is_some());
        }
    }
    #[test]
    fn nonzero_id_without_an_eligible_request_ticket_is_not_a_no_hit() {
        let mut state = State::new(Config::default()).unwrap();
        let selected = state.selected;
        let mut next = state.store.current().unwrap().clone();
        next.stamp.tick += 1;
        next.entities[0].selectable = false;
        next.entities[0].visual_revision += 1;
        next.selected = None;
        state.store.admit(next).unwrap();
        state.apply(Command {
            seq: 3,
            action: Action::SelectAt { x: 0, y: 0 },
        });
        let request = state.pending_gpu_pick().unwrap();
        assert!(!state.complete_gpu_pick(request, Ok([1, 0, 0, 255])));
        assert_eq!(state.selected, selected);
        assert_eq!(state.pick_state, "failed");
    }
    #[test]
    fn same_fixture_reload_invalidates_earlier_pick_ticket() {
        let mut state = State::new(Config::default()).unwrap();
        let id = state
            .scene
            .frame
            .entities
            .iter()
            .find(|e| e.selectable && e.visible)
            .unwrap()
            .reference;
        let ticket = state.store.pick_ticket(id).unwrap();
        assert_eq!(state.store.resolve_pick(&ticket), Some(id));
        assert!(state.apply(Command {
            seq: 1,
            action: Action::ReloadFixture
        }));
        assert_eq!(state.store.resolve_pick(&ticket), None);
    }
    #[test]
    fn rejected_fixture_command_keeps_current_hash_and_tick() {
        let mut state = State::new(Config::default()).unwrap();
        let hash = state.scene.hash;
        let tick = state.config.tick;
        assert!(!state.apply(Command {
            seq: 1,
            action: Action::SetMode {
                mode: "webgl2".into()
            }
        }));
        assert_eq!(state.scene.hash, hash);
        assert_eq!(state.config.tick, tick);
    }
}
