//! Accepted runtime audio and a separate user-selected source-file player.
//! Playback completion is presentation evidence and never a gameplay ACK.
pub use wonderland_audio_content::session::AcceptedAudioSession;

/// JavaScript numbers cannot carry original u64 identities. Only mixer voice
/// and seek fields cross this presentation boundary; serialize them as decimal.
pub fn mixer_json(
    intents: &[wonderland_audio_runtime::mixer::MixerIntent],
) -> Result<String, serde_json::Error> {
    let mut values = serde_json::to_value(intents)?;
    if let Some(items) = values.as_array_mut() {
        for item in items {
            if let Some(fields) = item
                .as_object_mut()
                .and_then(|o| o.values_mut().next())
                .and_then(|v| v.as_object_mut())
            {
                if let Some(voice) = fields.get_mut("voice").and_then(|v| v.as_object_mut()) {
                    for field in ["generation", "serial"] {
                        if let Some(n) = voice.get(field).and_then(|v| v.as_u64()) {
                            voice.insert(field.into(), serde_json::Value::String(n.to_string()));
                        }
                    }
                }
                if let Some(n) = fields.get("seek_frame").and_then(|v| v.as_u64()) {
                    fields.insert(
                        "seek_frame".into(),
                        serde_json::Value::String(n.to_string()),
                    );
                }
            }
        }
    }
    serde_json::to_string(&values)
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::AcceptedAudioSession;
    use leptos::prelude::*;
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(inline_js = r#"
export function createSourceAudioBridge({load = () => import('/audio/source-audio.mjs')} = {}) {
  let module = null, pending = null, epoch = 0, lastError = null;
  const initialize = () => {
    if (module) return Promise.resolve(module);
    if (pending) return pending;
    const lifetime = epoch;
    const attempt = Promise.resolve().then(load).then(candidate => {
      if (lifetime !== epoch) throw Error('sound initialization cancelled');
      for (const name of ['acceptedAudioHost', 'openSourceAudioControls', 'disposeSourceAudio']) {
        if (typeof candidate?.[name] !== 'function') throw Error('Incompatible sound module');
      }
      module = candidate;
      lastError = null;
      return module;
    }).catch(error => {
      if (lifetime !== epoch) throw Error('sound initialization cancelled');
      lastError = `Sound settings could not load. Try again or reload this page. ${String(error?.message ?? error).slice(0, 512)}`;
      throw Error(lastError);
    }).finally(() => { if (pending === attempt) pending = null; });
    pending = attempt;
    return attempt;
  };
  return {
    initialize,
    ready: () => module !== null,
    error: () => {
      if (lastError) return lastError;
      try {
        const state = module?.acceptedAudioHost().snapshot();
        return state?.lastNotice ?? state?.lastError ?? null;
      } catch (error) { return String(error?.message ?? error).slice(0, 1024); }
    },
    open: async () => {
      const lifetime = epoch, current = await initialize();
      if (lifetime !== epoch) throw Error('sound settings opening cancelled');
      current.openSourceAudioControls();
    },
    deliver: json => {
      if (!module) throw Error(lastError ?? 'Sound adapter is not loaded');
      module.acceptedAudioHost().applyAll(JSON.parse(json));
    },
    finished: () => JSON.stringify(module?.acceptedAudioHost().takeFinished() ?? []),
    clean: async () => {
      const current = module, lifetime = ++epoch;
      module = null;
      pending = null;
      lastError = null;
      try { await current?.disposeSourceAudio(); }
      catch (error) {
        if (lifetime === epoch) lastError = `Sound could not close: ${String(error?.message ?? error).slice(0, 512)}`;
        throw error;
      }
    }
  };
}
const sourceAudioBridge = createSourceAudioBridge();
export function initializeSourceAudio() { sourceAudioBridge.initialize().catch(() => {}); }
export function sourceAudioReady() { return sourceAudioBridge.ready(); }
export function sourceAudioError() { return sourceAudioBridge.error(); }
export function openSourceAudio() { return sourceAudioBridge.open(); }
export function deliverSourceAudio(json) { sourceAudioBridge.deliver(json); }
export function finishedSourceAudio() { return sourceAudioBridge.finished(); }
export function cleanSourceAudio() { sourceAudioBridge.clean().catch(() => {}); }
"#)]
    extern "C" {
        #[wasm_bindgen(js_name=initializeSourceAudio)]
        fn initialize();
        #[wasm_bindgen(js_name=sourceAudioReady)]
        fn ready() -> bool;
        #[wasm_bindgen(js_name=sourceAudioError)]
        fn device_error() -> Option<String>;
        #[wasm_bindgen(js_name=openSourceAudio)]
        fn open() -> js_sys::Promise;
        #[wasm_bindgen(catch,js_name=deliverSourceAudio)]
        pub fn deliver(json: &str) -> Result<(), JsValue>;
        #[wasm_bindgen(js_name=finishedSourceAudio)]
        pub fn finished() -> String;
        #[wasm_bindgen(js_name=cleanSourceAudio)]
        fn clean();
    }
    /// Own this value for the active source/live audio incarnation. Drop clears
    /// the timer and releases voices. Source content must already be authorized.
    pub struct AudioDriver {
        timer: i32,
        _callback: Closure<dyn FnMut()>,
        session: std::rc::Rc<std::cell::RefCell<AcceptedAudioSession>>,
    }
    impl Drop for AudioDriver {
        fn drop(&mut self) {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(self.timer);
            }
            let intents = self.session.borrow_mut().stop_all();
            if let Ok(json) = super::mixer_json(&intents) {
                let _ = deliver(&json);
            }
        }
    }
    pub fn start_audio_driver(
        session: std::rc::Rc<std::cell::RefCell<AcceptedAudioSession>>,
        notice: impl Fn(String) + 'static,
    ) -> Result<AudioDriver, JsValue> {
        use wasm_bindgen::JsCast;
        initialize();
        let current = session.clone();
        let mut cadence = wonderland_audio_content::session::AudioCadence::default();
        let mut previous = js_sys::Date::now();
        let mut last_device_error = None;
        let callback = Closure::wrap(Box::new(move || {
            let now = js_sys::Date::now();
            let count = cadence.advance(now - previous);
            previous = now;
            if let Some(error) = device_error()
                && last_device_error.as_ref() != Some(&error)
            {
                notice(format!("Source sound could not play: {error}"));
                last_device_error = Some(error);
            } else if device_error().is_none() {
                last_device_error = None;
            }
            if !ready() {
                return;
            }
            let mut session = current.borrow_mut();
            match serde_json::from_str::<Vec<serde_json::Value>>(&finished()) {
                Ok(voices) => {
                    for voice in voices {
                        let generation = voice
                            .get("generation")
                            .and_then(|v| v.as_str())
                            .and_then(|v| v.parse::<u64>().ok());
                        let serial = voice
                            .get("serial")
                            .and_then(|v| v.as_str())
                            .and_then(|v| v.parse::<u64>().ok());
                        if let (Some(generation), Some(serial)) = (generation, serial) {
                            session.complete_voice(wonderland_audio_runtime::mixer::VoiceId {
                                generation,
                                serial,
                            });
                        }
                    }
                }
                Err(e) => notice(format!("Sound completion could not be read: {e}")),
            }
            for _ in 0..count {
                match session.tick() {
                    Ok(intents) if !intents.is_empty() => match super::mixer_json(&intents) {
                        Ok(json) => {
                            if let Err(error) = deliver(&json) {
                                notice(format!("Source sound could not play: {:?}", error));
                            }
                        }
                        Err(e) => notice(format!("Source sound could not be prepared: {e}")),
                    },
                    Ok(_) => {}
                    Err(e) => notice(format!("Source sound stopped: {e}")),
                }
            }
            for (name, error) in session.system.take_faults() {
                notice(format!("Source sound {name} is unavailable: {error}"));
            }
        }) as Box<dyn FnMut()>);
        let window =
            web_sys::window().ok_or_else(|| JsValue::from_str("Browser window unavailable"))?;
        let timer = window.set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            16,
        )?;
        Ok(AudioDriver {
            timer,
            _callback: callback,
            session,
        })
    }
    #[component]
    pub fn SourceAudioControls() -> impl IntoView {
        initialize();
        let notice = RwSignal::new(String::new());
        let opening = RwSignal::new(false);
        on_cleanup(clean);
        view! {
            <span class="source-audio-control">
                <button type="button" class="chrome round settings-audio" aria-label="Sound settings"
                    disabled=move || opening.get() aria-busy=move || opening.get().to_string()
                    on:click=move |_| {
                        opening.set(true);
                        notice.set("Opening sound settings…".into());
                        wasm_bindgen_futures::spawn_local(async move {
                            let result = wasm_bindgen_futures::JsFuture::from(open()).await;
                            let message = result.err().map(|error| {
                                error.as_string().or_else(|| {
                                    js_sys::Reflect::get(&error, &JsValue::from_str("message"))
                                        .ok().and_then(|value| value.as_string())
                                }).unwrap_or_else(|| "Sound settings could not open. Try again.".into())
                            }).unwrap_or_default();
                            let _ = opening.try_set(false);
                            let _ = notice.try_set(message);
                        });
                    }>
                    <crate::components::Icon name="volume"/>
                </button>
                <span role="status" aria-live="polite">{move || notice.get()}</span>
            </span>
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::{AudioDriver, SourceAudioControls, deliver, finished, start_audio_driver};
