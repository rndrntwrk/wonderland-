//! Original Timer controls consume only the current owned private EOD stream.
use crate::native_account::NativeOwnedUi;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, prelude::*};
use wonderland_game_runtime::live_wire::player::PlayerBinding;

#[wasm_bindgen(inline_js = r#"
export async function openOwnedTimer(element,account,signal,onShow) {
  const module=await import('/native-timer.mjs');
  if(signal.aborted)return null;
  const chat=module.mountNativeTimer(element,(query,requestSignal)=>account.lotEod(query,requestSignal),()=>onShow());
  const abort=()=>chat.dispose();signal.addEventListener('abort',abort,{once:true});
  return {setLive:v=>chat.setLive(v),dispose:()=>{signal.removeEventListener('abort',abort);chat.dispose();}};
}
export function ownedTimerLive(host,live){if(host)host.setLive(live);}
export function closeOwnedTimer(host){if(host)host.dispose();}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=openOwnedTimer)]
    fn open(
        element: &web_sys::HtmlElement,
        account: &JsValue,
        signal: &web_sys::AbortSignal,
        shown: &js_sys::Function,
    ) -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=ownedTimerLive)]
    fn set_live(host: &JsValue, live: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=closeOwnedTimer)]
    fn close(host: &JsValue) -> Result<(), JsValue>;
}
#[derive(Default)]
struct Mounted {
    generation: u64,
    key: Option<(u64, PlayerBinding)>,
    abort: Option<web_sys::AbortController>,
    host: Option<JsValue>,
    shown: Option<Closure<dyn FnMut()>>,
}
impl Mounted {
    fn clear(&mut self) {
        self.generation = self.generation.saturating_add(1);
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
        if let Some(host) = self.host.take() {
            let _ = close(&host);
        }
        self.shown = None;
        self.key = None;
    }
}
#[component]
pub fn NativeTimer(
    owned: NativeOwnedUi,
    live: RwSignal<bool>,
    on_show: Callback<()>,
) -> impl IntoView {
    let node = NodeRef::<leptos::html::Div>::new();
    let mounted = StoredValue::new_local(Mounted::default());
    let revision = RwSignal::new(0u64);
    let notice = RwSignal::new(String::new());
    Effect::new(move |_| {
        owned.track();
        live.track();
        let key = owned.scope();
        let Some(element) = node.get() else {
            return;
        };
        if mounted.with_value(|m| m.key == key) {
            return;
        }
        mounted.update_value(Mounted::clear);
        let Some(key) = key else {
            return;
        };
        let Some(account) = owned.chat_provider() else {
            return;
        };
        let Ok(abort) = web_sys::AbortController::new() else {
            notice.set("Object dialogs could not start.".into());
            return;
        };
        mounted.update_value(|m| {
            m.key = Some(key);
            m.abort = Some(abort.clone());
        });
        let generation = mounted.with_value(|m| m.generation);
        notice.set("Loading object dialogs…".into());
        let shown = Closure::wrap(Box::new(move || on_show.run(())) as Box<dyn FnMut()>);
        let promise = open(
            element.unchecked_ref(),
            &account,
            &abort.signal(),
            shown.as_ref().unchecked_ref(),
        );
        mounted.update_value(|m| m.shown = Some(shown));
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(promise).await;
            if mounted.try_with_value(|m| m.generation) != Some(generation) {
                if let Ok(host) = result {
                    let _ = close(&host);
                }
                return;
            }
            match result {
                Ok(host) if !host.is_null() => {
                    mounted.update_value(|m| m.host = Some(host));
                    notice.set(String::new());
                    revision.update(|v| *v = v.saturating_add(1));
                }
                _ => {
                    mounted.update_value(|m| m.key = None);
                    notice.set("Object dialogs could not load. Reconnect to try again.".into());
                }
            }
        });
    });
    Effect::new(move |_| {
        let enabled = live.get();
        revision.track();
        mounted.with_value(|m| {
            if let Some(host) = &m.host {
                let _ = set_live(host, enabled);
            }
        });
    });
    on_cleanup(move || {
        mounted.try_update_value(Mounted::clear);
    });
    view! {<div class="native-owned-timer"><p role="status">{move ||notice.get()}</p><div node_ref=node/></div>}
}
