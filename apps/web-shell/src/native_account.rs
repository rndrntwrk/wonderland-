//! Native account-ready UI. No legacy SessionState or player admission is fabricated.
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::{JsCast, prelude::*};

#[wasm_bindgen(inline_js = r#"
export async function openNativeAccount(base, update) {
 const {createNativeAccountProvider}=await import('/native-account.mjs');
 return createNativeAccountProvider(base,v=>update(JSON.stringify(v)));
}
export function nativeAccountCall(host, action, username, password) {
 if(!host)return; if(action==='login')void host.login(username,password);
 else if(action==='refresh')void host.refresh();else if(action==='logout')void host.logout();else if(action==='dispose')host.dispose();
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=openNativeAccount)]
    fn open(base: &str, update: &js_sys::Function) -> js_sys::Promise;
    #[wasm_bindgen(js_name=nativeAccountCall)]
    fn call(host: &JsValue, action: &str, username: &str, password: &str);
}
#[derive(Clone, Default, Deserialize)]
struct Snapshot {
    state: String,
    avatars: Vec<Avatar>,
    message: String,
}
#[derive(Clone, Deserialize)]
struct Avatar {
    avatar_id: String,
    name: String,
}
#[derive(Default)]
struct Provider {
    host: Option<JsValue>,
    callback: Option<Closure<dyn FnMut(String)>>,
    disposed: bool,
}
#[component]
pub fn NativeAccountGame() -> impl IntoView {
    let state = RwSignal::new(Snapshot::default());
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let provider = StoredValue::new_local(Provider::default());
    let callback = Closure::wrap(Box::new(move |raw: String| {
        if let Ok(next) = serde_json::from_str::<Snapshot>(&raw) {
            state.try_set(next);
        }
    }) as Box<dyn FnMut(String)>);
    let promise = open("/native-account", callback.as_ref().unchecked_ref());
    wasm_bindgen_futures::spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(host) => {
                let retained = provider.try_update_value(|p| {
                    if p.disposed {
                        call(&host, "dispose", "", "");
                    } else {
                        p.host = Some(host.clone());
                        p.callback = Some(callback);
                    }
                });
                if retained.is_none() {
                    call(&host, "dispose", "", "");
                }
            }
            Err(_) => {
                state.try_set(Snapshot {
                    state: "signed_out".into(),
                    avatars: vec![],
                    message: "Native account connection could not start. Reload to try again."
                        .into(),
                });
            }
        }
    });
    on_cleanup(move || {
        provider.try_update_value(|p| {
            p.disposed = true;
            if let Some(h) = p.host.take() {
                call(&h, "dispose", "", "");
            }
            p.callback = None;
        });
    });
    let invoke = move |action: &str, user: &str, secret: &str| {
        provider.with_value(|p| {
            if let Some(h) = &p.host {
                call(h, action, user, secret);
            }
        });
    };
    view! {
    <main class="game-shell connected-shell"><section class="connected-login avatars-screen" aria-labelledby="native-account-title">
    <div class="connected-login-heading"><p>"Welcome to"</p><h1 id="native-account-title">"Wonderland"</h1><p>"Native account connection"</p></div>
    <Show when=move ||state.with(|s|s.state=="account_ready"||s.state=="account_error") fallback=move ||view!{
    <form class="connected-login-card chrome" on:submit=move |e|{e.prevent_default();let secret=password.get_untracked();password.set(String::new());invoke("login",&username.get_untracked(),&secret);}>
    <h2>"Come on in"</h2><label for="native-username">"Account name"</label><input id="native-username" autocomplete="username" required=true prop:value=move ||username.get() on:input=move |e|username.set(event_target_value(&e))/>
    <label for="native-password">"Password"</label><input id="native-password" type="password" autocomplete="current-password" required=true prop:value=move ||password.get() on:input=move |e|password.set(event_target_value(&e))/>
    <button class="chrome primary" type="submit" disabled=move ||state.with(|s|s.state=="loading"||s.state.is_empty())>"Sign in"</button>
    </form>}>
    <div class="connected-login-card chrome"><h2>"Your Sims"</h2><For each=move ||state.with(|s|s.avatars.clone()) key=|v|v.avatar_id.clone() children=move |v|view!{<p>{v.name}</p>}/><Show when=move ||state.with(|s|s.state=="account_ready" && s.avatars.is_empty())><p>"No Sims are saved to this account yet."</p></Show>
    <button class="chrome" on:click=move |_|invoke("refresh","","")>"Refresh roster"</button><button class="chrome" on:click=move |_|invoke("logout","","")>"Sign out"</button>
    <p>"World entry and character creation are not available in this staged connection."</p></div></Show>
    <p role="status" aria-live="polite">{move ||state.with(|s|s.message.clone())}</p>
    </section></main>}
}
