//! Lot chat shares the existing owned account provider; it never owns a token.
use crate::native_account::NativeOwnedUi;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, prelude::*};
use wonderland_game_runtime::live_wire::player::PlayerBinding;

#[wasm_bindgen(inline_js = r#"
export async function openOwnedChat(element,account,signal) {
  const module=await import('/native-chat.mjs');
  if(signal.aborted)return null;
  const chat=module.mountNativeLotChat(element,(action,payload,requestSignal)=>account.lotChat(action,payload,requestSignal));
  const abort=()=>chat.dispose();signal.addEventListener('abort',abort,{once:true});
  return {setLive:v=>chat.setLive(v),dispose:()=>{signal.removeEventListener('abort',abort);chat.dispose();}};
}
export function ownedChatLive(host,live){if(host)host.setLive(live);}
export function closeOwnedChat(host){if(host)host.dispose();}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=openOwnedChat)]
    fn open(
        element: &web_sys::HtmlElement,
        account: &JsValue,
        signal: &web_sys::AbortSignal,
    ) -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=ownedChatLive)]
    fn set_live(host: &JsValue, live: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(catch,js_name=closeOwnedChat)]
    fn close(host: &JsValue) -> Result<(), JsValue>;
}
#[derive(Default)]
struct Mounted {
    generation: u64,
    key: Option<(u64, PlayerBinding)>,
    abort: Option<web_sys::AbortController>,
    host: Option<JsValue>,
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
        self.key = None;
    }
}
#[component]
pub fn NativeChat(
    owned: NativeOwnedUi,
    live: RwSignal<bool>,
    open_panel: RwSignal<bool>,
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
            notice.set("Chat could not start.".into());
            return;
        };
        mounted.update_value(|m| {
            m.key = Some(key);
            m.abort = Some(abort.clone());
        });
        let generation = mounted.with_value(|m| m.generation);
        notice.set("Loading lot chat…".into());
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(open(
                element.unchecked_ref(),
                &account,
                &abort.signal(),
            ))
            .await;
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
                    notice.set("Chat could not load. Reconnect to try again.".into());
                }
            }
        });
    });
    Effect::new(move |_| {
        let enabled = live.get() && open_panel.get();
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
    view! {<div class="native-owned-chat" hidden=move ||!open_panel.get()><p role="status">{move ||notice.get()}</p><div node_ref=node/></div>}
}
