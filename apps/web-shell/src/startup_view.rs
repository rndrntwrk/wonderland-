//! Bounded startup configuration loading; a failure never opens another mode.
use crate::startup::{ClientMode, MAX_CONFIG_BYTES, RuntimeConfiguration, parse_configuration};
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[derive(Clone)]
enum StartupState {
    Loading,
    Failed(String),
    Ready(RuntimeConfiguration),
}

#[derive(Default)]
struct Attempt {
    generation: u64,
    controller: Option<web_sys::AbortController>,
}

impl Attempt {
    fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(controller) = self.controller.take() {
            controller.abort();
        }
    }
}

#[component]
pub fn Startup() -> impl IntoView {
    let state = RwSignal::new(StartupState::Loading);
    let attempt = StoredValue::new_local(Attempt::default());
    on_cleanup(move || {
        attempt.try_update_value(Attempt::cancel);
    });
    begin(state, attempt);
    view! {
        {move || match state.get() {
            StartupState::Ready(config) => match config.mode {
                ClientMode::Preview => view! { <crate::app::PreviewApp/> }.into_any(),
                ClientMode::Connected => view! { <crate::connected::ConnectedGame gateway_url=config.gateway_url.unwrap_or_default()/> }.into_any(),
            },
            StartupState::Loading => view! {
                <main class="game-startup" aria-busy="true">
                    <div class="startup-card"><span class="eyebrow">"WONDERLAND"</span>
                        <h1>"Opening your world"</h1><p role="status">"Getting ready…"</p>
                    </div>
                </main>
            }.into_any(),
            StartupState::Failed(message) => view! {
                <main class="game-startup">
                    <div class="startup-card"><span class="eyebrow">"WONDERLAND"</span>
                        <h1>"We couldn’t open this world"</h1><p role="alert">{message}</p>
                        <button class="primary" on:click=move |_| begin(state, attempt)>"Try again"</button>
                    </div>
                </main>
            }.into_any(),
        }}
    }
}

fn begin(state: RwSignal<StartupState>, attempt: StoredValue<Attempt, LocalStorage>) {
    let Ok(controller) = web_sys::AbortController::new() else {
        state.set(StartupState::Failed(
            "This browser could not start the connection.".into(),
        ));
        return;
    };
    let mut generation = 0;
    attempt.update_value(|attempt| {
        attempt.cancel();
        generation = attempt.generation;
        attempt.controller = Some(controller.clone());
    });
    state.set(StartupState::Loading);
    spawn_local(async move {
        let result = load_configuration(controller).await;
        if !attempt
            .try_with_value(|attempt| attempt.generation == generation)
            .unwrap_or(false)
        {
            return;
        }
        attempt.try_update_value(|attempt| {
            attempt.controller.take();
        });
        state.try_set(match result {
            Ok(config) => StartupState::Ready(config),
            Err(message) => StartupState::Failed(message),
        });
    });
}

struct Timeout {
    id: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Drop for Timeout {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(self.id);
        }
    }
}

async fn load_configuration(
    controller: web_sys::AbortController,
) -> Result<RuntimeConfiguration, String> {
    let failure = || {
        "Check your connection and try again. If it keeps failing, contact the world operator."
            .to_string()
    };
    let window = web_sys::window().ok_or_else(failure)?;
    let abort = controller.clone();
    let callback = Closure::wrap(Box::new(move || abort.abort()) as Box<dyn FnMut()>);
    let id = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            15_000,
        )
        .map_err(|_| failure())?;
    let _timeout = Timeout {
        id,
        _callback: callback,
    };
    let options = web_sys::RequestInit::new();
    options.set_method("GET");
    options.set_credentials(web_sys::RequestCredentials::Omit);
    options.set_cache(web_sys::RequestCache::NoStore);
    options.set_redirect(web_sys::RequestRedirect::Error);
    options.set_signal(Some(&controller.signal()));
    let request = web_sys::Request::new_with_str_and_init("./wonderland-config.json", &options)
        .map_err(|_| failure())?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| failure())?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| failure())?;
    if !response.ok() {
        controller.abort();
        return Err(failure());
    }
    if response
        .headers()
        .get("Content-Length")
        .ok()
        .flatten()
        .and_then(|length| length.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_CONFIG_BYTES)
    {
        controller.abort();
        return Err("This world’s setup needs attention. Contact the world operator.".into());
    }
    let stream = response.body().ok_or_else(failure)?;
    let reader = stream
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| failure())?;
    let mut bytes = Vec::new();
    loop {
        let chunk = JsFuture::from(reader.read()).await.map_err(|_| failure())?;
        if js_sys::Reflect::get(&chunk, &JsValue::from_str("done"))
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            break;
        }
        let value =
            js_sys::Reflect::get(&chunk, &JsValue::from_str("value")).map_err(|_| failure())?;
        let array = value
            .dyn_into::<js_sys::Uint8Array>()
            .map_err(|_| failure())?;
        if bytes.len().saturating_add(array.length() as usize) > MAX_CONFIG_BYTES {
            controller.abort();
            let _ = reader.cancel();
            return Err("This world’s setup needs attention. Contact the world operator.".into());
        }
        bytes.extend(array.to_vec());
    }
    reader.release_lock();
    let text = std::str::from_utf8(&bytes).map_err(|_| failure())?;
    parse_configuration(text)
        .map_err(|_| "This world’s setup needs attention. Contact the world operator.".into())
}
