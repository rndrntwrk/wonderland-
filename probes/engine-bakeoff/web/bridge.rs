//! Shared control plane. Engine objects remain private and non-authoritative.
use serde::{Deserialize, Serialize};
use wonderland_engine_fixture::{hash_hex, pick_at, representative_scene, FixtureScene};
use wonderland_render_core::{frame::FrameStore, EntityRef, RenderLimits, ViewMode};

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
        })
    }
    pub fn pick_index(&self, owner: Option<EntityRef>) -> u32 {
        owner
            .and_then(|id| self.ids.iter().position(|i| *i == id))
            .map_or(0, |i| i as u32 + 1)
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
                self.selected = pick_at(&self.scene, x, y)
                    .and_then(|id| self.store.pick_ticket(id))
                    .and_then(|ticket| self.store.resolve_pick(&ticket));
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
                self.suspended = true;
                Ok(false)
            }
            Action::Resume => {
                self.suspended = false;
                Ok(false)
            }
            Action::SimulateLoss | Action::DeviceLost => {
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
            "tick":self.config.tick,"selection":self.selected,"selectionSource":"cpu-reference-generation-checked",
            "pass":if self.pick_pass {"pick"} else {"color"},"idMap":ids,
            "suspended":self.suspended,"updateCount":self.update_count,"renderScheduleVisits":render_visits,
            "fixtureRevision":self.revision,"lastCommand":self.last_command,"engineErrors":self.errors,
            "meshCount":self.scene.draws.len(),"spriteCount":self.scene.sprites.len(),
            "vertexCount":self.scene.draws.iter().map(|d|d.mesh.vertices.len()).sum::<usize>(),
            "gpuReadback":"pending","authoritativeTicksAdvanced":0
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
