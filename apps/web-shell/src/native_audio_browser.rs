//! Device lifetime for the native accepted-audio consumer. Never sends gameplay
//! requests, settles receipts, advances the VM, or fetches sample URLs.
use crate::native_audio::NativeAudio;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use wasm_bindgen::{JsCast, prelude::*};
use wonderland_audio_content::{pack::AudioPack, session::AudioCadence};
use wonderland_game_runtime::{RuntimeProjection, TickOutcome};
use wonderland_world_view::{ViewportControls, WorldDocument};
#[wasm_bindgen(inline_js = r#"
export function loadNativeAudioModule(){return import('/native-audio.mjs');}
export function newNativeAudioHost(module){return module.openNativeAudio();}
export function nativeAudioGeneration(host){return host.generation;}
export function nativeAudioRegister(host,key,rate,channels,samples){host.register(key,rate,channels,samples);}
export function nativeAudioDeliver(host,json){host.deliver(json);}
export function nativeAudioFinished(host){return host.finished();}
export function nativeAudioPlayable(host){return host.playable() && !document.hidden;}
export function nativeAudioStop(host){host.stop();}
export function nativeAudioDispose(host){host.dispose();}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=loadNativeAudioModule)]
    pub fn load_module() -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=newNativeAudioHost)]
    fn open(module: &JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_name=nativeAudioGeneration)]
    fn generation(host: &JsValue) -> String;
    #[wasm_bindgen(catch,js_name=nativeAudioRegister)]
    fn register(
        host: &JsValue,
        key: &str,
        rate: u32,
        channels: u16,
        samples: &[i16],
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=nativeAudioDeliver)]
    fn deliver(host: &JsValue, json: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=nativeAudioFinished)]
    fn finished(host: &JsValue) -> Result<String, JsValue>;
    #[wasm_bindgen(js_name=nativeAudioPlayable)]
    fn playable(host: &JsValue) -> bool;
    #[wasm_bindgen(catch,js_name=nativeAudioStop)]
    fn stop(host: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=nativeAudioDispose)]
    fn dispose(host: &JsValue) -> Result<(), JsValue>;
}
struct State {
    audio: NativeAudio,
    host: JsValue,
    playing: bool,
    failed: bool,
}
impl State {
    fn flush(
        &mut self,
        intents: &[wonderland_audio_runtime::mixer::MixerIntent],
    ) -> Result<(), String> {
        let json = crate::audio_bridge::mixer_json(intents).map_err(|e| e.to_string())?;
        deliver(&self.host, &json)
            .map_err(|_| "The sound device could not accept a mixer update".into())
    }
    fn silence(&mut self) {
        self.audio.stop();
        let _ = stop(&self.host);
        self.playing = false;
    }
    fn tick(
        &mut self,
        count: usize,
        view: Option<(Arc<WorldDocument>, ViewportControls, f32)>,
    ) -> Result<(), String> {
        if self.failed {
            return Ok(());
        }
        if !playable(&self.host) {
            if self.playing {
                self.silence();
            }
            return Ok(());
        }
        self.playing = true;
        let raw = finished(&self.host).map_err(|_| "Sound completion is unavailable")?;
        let voices: Vec<serde_json::Value> =
            serde_json::from_str(&raw).map_err(|_| "Invalid sound completion")?;
        if voices.len() > 4096 {
            return Err("Sound completion count exceeded".into());
        }
        for voice in voices {
            let number = |name| {
                voice
                    .get(name)
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse().ok())
            };
            if let (Some(generation), Some(serial)) = (number("generation"), number("serial")) {
                self.audio
                    .session
                    .complete_voice(wonderland_audio_runtime::mixer::VoiceId {
                        generation,
                        serial,
                    });
            }
        }
        for _ in 0..count {
            if let Some((world, controls, aspect)) = &view {
                self.audio.spatial(world, *controls, *aspect);
            }
            let intents = self.audio.session.tick().map_err(|e| e.to_string())?;
            if !intents.is_empty() {
                self.flush(&intents)?;
            }
        }
        Ok(())
    }
}
pub struct AudioHandle {
    state: Rc<RefCell<State>>,
    timer: i32,
    _callback: Closure<dyn FnMut()>,
}
impl AudioHandle {
    /// Must only be called after the async module import's connection and content
    /// identity have been rechecked. Opening supersedes the prior device lifetime.
    pub fn open(
        module: &JsValue,
        pack: Arc<AudioPack>,
        projection: &RuntimeProjection,
        view: impl Fn() -> Option<(Arc<WorldDocument>, ViewportControls, f32)> + 'static,
        notice: impl Fn(String) + 'static,
    ) -> Result<Self, String> {
        let host = open(module).map_err(|_| "Native sound could not initialize")?;
        let created = (|| {
            let id = generation(&host)
                .parse()
                .map_err(|_| "Invalid sound device generation")?;
            let mut audio =
                NativeAudio::new(pack.clone(), projection.lot_id, projection.epoch, id)?;
            audio.checkpoint(projection.tick);
            for (key, pcm) in &pack.samples {
                let key = key.0.iter().map(|v| format!("{v:02x}")).collect::<String>();
                register(&host, &key, pcm.sample_rate, pcm.channels, &pcm.samples)
                    .map_err(|_| "Game sound resource could not be admitted")?;
            }
            let state = Rc::new(RefCell::new(State {
                audio,
                host: host.clone(),
                playing: false,
                failed: false,
            }));
            let current = state.clone();
            let mut cadence = AudioCadence::default();
            let mut previous = js_sys::Date::now();
            let mut last_notice = None;
            let callback = Closure::wrap(Box::new(move || {
                let now = js_sys::Date::now();
                let elapsed = now - previous;
                previous = now;
                let count = cadence.advance(elapsed);
                let mut state = current.borrow_mut();
                // Timer lateness is not a visibility or device transition. The
                // cadence already drops excessive catch-up work; keep accepted
                // loops alive during foreground load. tick() independently stops
                // playback for a hidden document or a non-running audio device.
                if let Err(error) = state.tick(count, view()) {
                    state.silence();
                    state.failed = true;
                    notice(format!("Game sound stopped: {error}"));
                } else if let Some(message) = state.audio.notice.as_ref()
                    && last_notice.as_ref() != Some(message)
                {
                    last_notice = Some(message.clone());
                    notice(message.clone());
                }
            }) as Box<dyn FnMut()>);
            let window = web_sys::window().ok_or("Browser window unavailable")?;
            let timer = window
                .set_interval_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    16,
                )
                .map_err(|_| "Audio cadence could not start")?;
            Ok(Self {
                state,
                timer,
                _callback: callback,
            })
        })();
        if created.is_err() {
            let _ = dispose(&host);
        }
        created
    }
    pub fn accept(
        &self,
        outcomes: &[TickOutcome],
        projection: &RuntimeProjection,
    ) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        let audible = !state.failed && playable(&state.host);
        if !audible {
            state.silence();
        }
        state.audio.accept(outcomes, projection, audible)
    }
    pub fn checkpoint(&self, tick: u64) {
        let mut state = self.state.borrow_mut();
        state.audio.checkpoint(tick);
        let _ = stop(&state.host);
    }
}
impl Drop for AudioHandle {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_interval_with_handle(self.timer);
        }
        let mut state = self.state.borrow_mut();
        state.audio.stop();
        let _ = dispose(&state.host);
    }
}
