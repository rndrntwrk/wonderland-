//! Native owned account/lot UI. No legacy SessionState or actor identity is fabricated.
use crate::avatar_content::{AvatarMotion, ContentLoader, ContentTarget, ContentUi};
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::{JsCast, prelude::*};
use wonderland_game_runtime::live_wire::player::PlayerBinding;

#[wasm_bindgen(inline_js = r#"
export async function openNativeAccount(base, update, nativeLots) {
 const {createNativeAccountProvider}=await import('/native-account.mjs');
 return createNativeAccountProvider(base,v=>update(JSON.stringify(v)),{nativeLots});
}
export function nativeAccountCall(host, action, first, second) {
 if(!host)return; if(action==='login')void host.login(first,second);
 else if(action==='openCreator')void host.openCreator();
 else if(action==='closeCreator')host.closeCreator();
 else if(action==='create')void host.createAvatar(JSON.parse(first));
 else if(action==='retryCreate')void host.retryCreate();
 else if(action==='enter')void host.enter(first,second);
 else if(action==='retryPlacement')void host.retryPlacement();
 else if(action==='placementUnavailable')host.placementUnavailable();
 else if(action==='leave')void host.leave();
 else if(action==='refresh')void host.refresh();else if(action==='logout')void host.logout();else if(action==='dispose')host.dispose();
}
export async function nativeOwnedSocketGrant(host) { if(!host)throw Error('Account closed');return JSON.stringify(await host.socketGrant()); }
export async function nativeOwnedAssetPage(host,avatar,after) { if(!host)throw Error('Account closed');return JSON.stringify(await host.ownedAssets(avatar,after)); }
"#)]
extern "C" {
    #[wasm_bindgen(js_name=openNativeAccount)]
    fn open(base: &str, update: &js_sys::Function, native_lots: bool) -> js_sys::Promise;
    #[wasm_bindgen(js_name=nativeAccountCall)]
    fn call(host: &JsValue, action: &str, first: &str, second: &str);
    #[wasm_bindgen(js_name=nativeOwnedSocketGrant)]
    fn socket_grant(host: &JsValue) -> js_sys::Promise;
    #[wasm_bindgen(js_name=nativeOwnedAssetPage)]
    fn asset_page(host: &JsValue, avatar: &str, after: &str) -> js_sys::Promise;
}
#[derive(Clone, Default, Deserialize)]
struct Snapshot {
    #[serde(default)]
    creator_open: bool,
    #[serde(default)]
    creation_options: Option<crate::native_creation::CreationCatalog>,
    #[serde(default)]
    creation_pending: bool,
    #[serde(default)]
    creation_unknown: bool,
    #[serde(default)]
    created_avatar_id: Option<String>,
    state: String,
    avatars: Vec<Avatar>,
    message: String,
    #[serde(default)]
    destinations: Vec<Destination>,
    #[serde(default)]
    admission_available: bool,
    #[serde(default)]
    grant: Option<OwnedGrant>,
    #[serde(default)]
    entry_pending: Option<PendingEntry>,
}
#[derive(Clone, Deserialize)]
struct Avatar {
    avatar_id: String,
    name: String,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
struct Destination {
    lot_id: String,
    name: String,
    lot_location: u32,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
struct PendingEntry {
    avatar_id: String,
    lot_id: String,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
struct OwnedGrant {
    recovery_scope: String,
    binding: OwnedBinding,
    lot_id: String,
    lot_name: String,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedBinding {
    source_epoch: String,
    lot_incarnation: String,
    lot_location: u32,
    avatar_id: u32,
}
impl OwnedBinding {
    fn player(&self) -> Option<PlayerBinding> {
        let parse = |v: &str| {
            v.parse::<u64>()
                .ok()
                .filter(|n| *n > 0 && n.to_string() == v)
        };
        if self.avatar_id == 0 || self.avatar_id > 16_777_215 || self.lot_location == 0 {
            return None;
        }
        Some(PlayerBinding {
            source_epoch: parse(&self.source_epoch)?,
            lot_incarnation: parse(&self.lot_incarnation)?,
            lot_location: self.lot_location,
            avatar_id: self.avatar_id,
        })
    }
}
#[derive(Default)]
struct Provider {
    host: Option<JsValue>,
    callback: Option<Closure<dyn FnMut(String)>>,
    disposed: bool,
}
#[derive(Clone, Copy)]
pub struct NativeOwnedUi {
    state: RwSignal<Snapshot>,
    epoch: RwSignal<u64>,
    provider: StoredValue<Provider, LocalStorage>,
}
impl NativeOwnedUi {
    pub(super) fn chat_provider(self) -> Option<JsValue> {
        self.provider.with_value(|p| p.host.clone())
    }
    pub fn track(self) {
        self.state.track();
    }
    pub fn scope(self) -> Option<(u64, PlayerBinding)> {
        self.state
            .try_with_untracked(|s| {
                if matches!(
                    s.state.as_str(),
                    "lot_ready" | "admitted_awaiting_placement"
                ) {
                    s.grant
                        .as_ref()?
                        .binding
                        .player()
                        .map(|b| (self.epoch.get_untracked(), b))
                } else {
                    None
                }
            })
            .flatten()
    }
    /// Private local continuity for one authenticated account entry. Server
    /// authorization still comes from the fresh grant and committed OwnedControl.
    pub fn recovery_scope(self) -> Option<String> {
        self.state
            .try_with_untracked(|s| {
                matches!(
                    s.state.as_str(),
                    "lot_ready" | "admitted_awaiting_placement"
                )
                .then_some(s.grant.as_ref())
                .flatten()
                .filter(|g| !g.recovery_scope.is_empty() && g.recovery_scope.len() <= 32)
                .map(|g| g.recovery_scope.clone())
            })
            .flatten()
    }
    pub fn leave(self) {
        self.invoke("leave", "", "");
    }
    pub fn awaiting_placement(self) -> bool {
        self.state
            .with_untracked(|s| s.state == "admitted_awaiting_placement")
    }
    pub fn placement_unavailable(self) {
        self.invoke("placementUnavailable", "", "");
    }
    fn invoke(self, action: &str, first: &str, second: &str) {
        self.provider.with_value(|p| {
            if let Some(h) = &p.host {
                call(h, action, first, second)
            }
        });
    }
    pub async fn admit(
        self,
        epoch: u64,
        binding: PlayerBinding,
    ) -> Result<(String, String, String), String> {
        if self.scope() != Some((epoch, binding)) {
            return Err("This owned property session changed.".into());
        }
        let host = self
            .provider
            .with_value(|p| p.host.clone())
            .ok_or("Account connection closed.")?;
        let raw = wasm_bindgen_futures::JsFuture::from(socket_grant(&host))
            .await
            .map_err(|_| "Owned property admission could not be revalidated.")?
            .as_string()
            .ok_or("Invalid owned property response.")?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Ticket {
            binding: OwnedBinding,
            ticket: String,
        }
        let ticket: Ticket =
            serde_json::from_str(&raw).map_err(|_| "Invalid owned property response.")?;
        if self.scope() != Some((epoch, binding))
            || ticket.binding.player() != Some(binding)
            || ticket.ticket.is_empty()
            || ticket.ticket.len() > 4096
            || ticket.ticket.chars().any(char::is_control)
        {
            return Err("Owned property admission no longer matches this Sim.".into());
        }
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .ok_or("Invalid browser origin.")?;
        let mut url = url::Url::parse(&origin)
            .map_err(|_| "Invalid browser origin.")?
            .join("/native-lot/stream")
            .map_err(|_| "Invalid native endpoint.")?;
        let scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            _ => return Err("Invalid native endpoint.".into()),
        };
        url.set_scheme(scheme)
            .map_err(|_| "Invalid native endpoint.")?;
        Ok((url.into(), origin, ticket.ticket))
    }
}
#[component]
pub fn NativeAccountGame(#[prop(default = false)] native_lots: bool) -> impl IntoView {
    let state = RwSignal::new(Snapshot::default());
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<String>);
    let provider = StoredValue::new_local(Provider::default());
    let owned = NativeOwnedUi {
        state,
        provider,
        epoch: RwSignal::new(1),
    };
    let content = ContentUi::new();
    provide_context(content);
    provide_context(AvatarMotion(Signal::derive(|| false)));
    let content_target = ContentTarget {
        busy: Signal::derive(move || {
            state.with(|s| matches!(s.state.as_str(), "loading" | "lot_entering" | "lot_leaving"))
        }),
        state_revision: Signal::derive(move || owned.epoch.get()),
        content_revision: Signal::derive(move || {
            content
                .choices
                .with(|c| c.as_ref().map(|c| c.revision).unwrap_or(0))
        }),
        install: Callback::new(|_: wonderland_contracts::authoring::AppearanceContent| Ok(())),
    };
    provide_context(content_target);
    let loaded_scope = StoredValue::new(None::<String>);
    Effect::new(move |_| {
        let snapshot = state.get();
        if matches!(snapshot.state.as_str(), "signed_out" | "signing_out") {
            if loaded_scope.get_value().is_some() {
                content.clear_authenticated();
                loaded_scope.set_value(None);
            }
            return;
        }
        let scope = if snapshot.creator_open {
            snapshot
                .creation_options
                .as_ref()
                .map(|c| format!("creator:{}:{}", c.release, owned.epoch.get_untracked()))
        } else if snapshot.state == "lot_ready" {
            snapshot
                .grant
                .as_ref()
                .map(|g| format!("lot:{}:{}", g.recovery_scope, g.binding.lot_incarnation))
        } else {
            None
        };
        let Some(scope) = scope else {
            if loaded_scope
                .get_value()
                .as_ref()
                .is_some_and(|v| v.starts_with("creator:"))
            {
                content.clear_authenticated();
                loaded_scope.set_value(None);
            }
            return;
        };
        if loaded_scope.get_value().as_ref() == Some(&scope) {
            return;
        }
        if let Some(host) = provider.with_value(|p| p.host.clone()) {
            loaded_scope.set_value(Some(scope));
            content.load_authenticated(content_target, host);
        }
    });
    let callback = Closure::wrap(Box::new(move |raw: String| {
        if let Ok(next) = serde_json::from_str::<Snapshot>(&raw) {
            let changed = state.with_untracked(|old| {
                old.grant != next.grant || old.creator_open != next.creator_open
            });
            if changed {
                owned.epoch.update(|n| *n = n.saturating_add(1));
            }
            if next.state == "signed_out" || next.state == "signing_out" {
                selected.set(None);
            }
            if let Some(id) = &next.created_avatar_id {
                if next.avatars.iter().any(|v| &v.avatar_id == id) {
                    selected.set(Some(id.clone()));
                }
            }
            state.try_set(next);
        }
    }) as Box<dyn FnMut(String)>);
    let promise = open(
        "/native-account",
        callback.as_ref().unchecked_ref(),
        native_lots,
    );
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
                    message: "Native account connection could not start. Reload to try again."
                        .into(),
                    ..Snapshot::default()
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
    let authenticated = move || {
        state.with(|s| {
            matches!(
                s.state.as_str(),
                "account_ready"
                    | "account_error"
                    | "account_refreshing"
                    | "lot_entering"
                    | "lot_leaving"
                    | "detach_pending"
                    | "admitted_awaiting_placement"
                    | "lot_ready"
            )
        })
    };
    view! {
    <main class="game-shell connected-shell">
    <Show when=move||state.with(|s|matches!(s.state.as_str(),"lot_ready"|"admitted_awaiting_placement") && s.grant.is_some()) fallback=move||view!{
    <section class="connected-login avatars-screen" aria-labelledby="native-account-title">
    <div class="connected-login-heading"><p>"Welcome to"</p><h1 id="native-account-title">"Wonderland"</h1><p>"Native account connection"</p></div>
    <Show when=authenticated fallback=move||view!{
    <form class="connected-login-card chrome" on:submit=move|e|{e.prevent_default();let secret=password.get_untracked();password.set(String::new());owned.invoke("login",&username.get_untracked(),&secret);}>
    <h2>"Come on in"</h2><label for="native-username">"Account name"</label><input id="native-username" autocomplete="username" required=true prop:value=move||username.get() on:input=move|e|username.set(event_target_value(&e))/>
    <label for="native-password">"Password"</label><input id="native-password" type="password" autocomplete="current-password" required=true prop:value=move||password.get() on:input=move|e|password.set(event_target_value(&e))/>
    <button class="chrome primary" type="submit" disabled=move||state.with(|s|s.state=="loading"||s.state=="signing_out"||s.state.is_empty())>"Sign in"</button></form>}>
    <Show when=move||state.with(|s|s.creator_open) fallback=move||view!{
    <div class=move||if selected.get().is_some(){"connected-login-card native-account-card native-directory-card chrome"}else{"connected-login-card native-account-card chrome"}><h2>"Your Sims"</h2>
    <For each=move||state.with(|s|s.avatars.clone()) key=|v|v.avatar_id.clone() children=move|v|{
        let id=v.avatar_id;let name=v.name;
        view!{<div class="native-owned-avatar"><p>{name.clone()}</p><Show when=move||native_lots && state.with(|s|s.admission_available)><button class="chrome primary" aria-pressed={let id=id.clone();move||(selected.get().as_ref()==Some(&id)).to_string()} disabled=move||state.with(|s|s.entry_pending.is_some()||!matches!(s.state.as_str(),"account_ready"|"account_error")) on:click={let id=id.clone();move|_|selected.set(Some(id.clone()))}>{format!("Play as {name}")}</button></Show></div>}
    }/>
    <Show when=move||native_lots && selected.get().is_some()><SavedPossessions state selected owned/></Show>
    <Show when=move||state.with(|s|s.state=="account_ready"&&s.avatars.is_empty())><p>"No Sims are saved to this account yet."</p></Show>
    <Show when=move||native_lots><button class="chrome primary" disabled=move||state.with(|s|s.entry_pending.is_some()||!matches!(s.state.as_str(),"account_ready"|"account_error")||s.avatars.len()>=3) on:click=move |_|owned.invoke("openCreator","","")>"Create a Sim"</button></Show>
    <Show when=move||native_lots && selected.get().is_some()><PropertyDirectory state selected owned/></Show>
    <button class="chrome" disabled=move||state.with(|s|s.entry_pending.is_some()||s.state!="account_ready"&&s.state!="account_error") on:click=move|_|owned.invoke("refresh","","")>"Refresh roster"</button>
    <button class="chrome" on:click=move|_|owned.invoke("logout","","")>"Sign out"</button>
    <Show when=move||state.with(|s|s.state=="admitted_awaiting_placement")><button class="chrome" on:click=move|_|owned.invoke("retryPlacement","","")>"Check placement"</button><button class="chrome" on:click=move|_|owned.leave()>"Leave property"</button></Show>
    <Show when=move||state.with(|s|s.state=="detach_pending")><button class="chrome" on:click=move|_|owned.leave()>"Retry leaving"</button></Show>
    <Show when=move||!native_lots><p>"World entry and character creation are not available in this staged connection."</p></Show>
    <Show when=move||native_lots><details><summary>"Game content"</summary><ContentLoader/></details></Show>
    </div>}>
    <crate::native_creator_browser::NativeCreator
      catalog=Signal::derive(move||state.with(|s|s.creation_options.clone()))
      busy=Signal::derive(move||state.with(|s|s.state=="account_refreshing"))
      pending=Signal::derive(move||state.with(|s|s.creation_pending))
      unknown=Signal::derive(move||state.with(|s|s.creation_unknown))
      submit=Callback::new(move|spec:String|owned.invoke("create",&spec,""))
      close=Callback::new(move|()|owned.invoke("closeCreator","",""))
      retry=Callback::new(move|()|owned.invoke("retryCreate","",""))
      reload=Callback::new(move|()|{if let Some(host)=provider.with_value(|p|p.host.clone()){content.load_authenticated(content_target,host);}})/>
    <button class="chrome" on:click=move |_|owned.invoke("logout","","")>"Sign out"</button>
    </Show></Show>
    <p role="status" aria-live="polite">{move||state.with(|s|s.message.clone())}</p>
    </section>}>
    <div hidden=move||state.with(|s|s.state=="admitted_awaiting_placement")><crate::native_lot::NativeLot owned=owned/></div>
    <Show when=move||state.with(|s|s.state=="admitted_awaiting_placement")><section class="connected-login-card chrome" aria-label="Awaiting source placement"><h2>"Your Sim is awaiting placement"</h2><p role="status">{move||state.with(|s|s.message.clone())}</p><button class="chrome" on:click=move|_|owned.invoke("retryPlacement","","")>"Check placement"</button><button class="chrome" on:click=move|_|owned.leave()>"Leave property"</button></section></Show>
    <div class="top-tools"><button class="chrome" on:click=move|_|owned.invoke("logout","","")>"Sign out"</button></div>
    </Show>
    </main>}
}

#[derive(Clone, Deserialize)]
struct AssetPage {
    avatar_id: String,
    balance: String,
    outfits: Vec<OwnedOutfit>,
    next: Option<String>,
}
#[derive(Clone, Deserialize)]
struct OwnedOutfit {
    id: String,
    asset_key: String,
    suit_type: u16,
}
#[component]
fn SavedPossessions(
    state: RwSignal<Snapshot>,
    selected: RwSignal<Option<String>>,
    owned: NativeOwnedUi,
) -> impl IntoView {
    let open_panel = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let page = RwSignal::new(None::<AssetPage>);
    let message = RwSignal::new(String::new());
    let serial = RwSignal::new(0u64);
    let allowed = Memo::new(move |_| state.with(|s| s.entry_pending.is_none()
        && matches!(s.state.as_str(), "account_ready" | "account_error")));
    let close = move || {
        serial.update(|n| *n = n.saturating_add(1));
        open_panel.set(false);busy.set(false);page.set(None);message.set(String::new());
    };
    Effect::new(move |_| {selected.track();if !allowed.get() || open_panel.get_untracked(){close();}});
    let read = Callback::new(move |after: String| {
        if !allowed.get_untracked() || busy.get_untracked() {return;}
        let Some(id) = selected.get_untracked() else {return;};
        let Some(host) = owned.provider.with_value(|p|p.host.clone()) else {return;};
        serial.update(|n| *n = n.saturating_add(1));let request = serial.get_untracked();
        open_panel.set(true);busy.set(true);page.set(None);message.set("Reading saved possessions…".into());
        let promise = asset_page(&host,&id,&after);
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(promise).await;
            if serial.try_get_untracked()!=Some(request) || selected.try_get_untracked()!=Some(Some(id.clone())) {return;}
            busy.set(false);
            let value=result.ok().and_then(|v|v.as_string()).and_then(|s|serde_json::from_str::<AssetPage>(&s).ok()).filter(|p|p.avatar_id==id);
            if let Some(value)=value {page.set(Some(value));message.set(String::new());}
            else {message.set("Saved possessions could not be read. Refresh or sign in again.".into());}
        });
    });
    view! {
        <button class="chrome" type="button" disabled=move||!allowed.get()||busy.get()
            on:click=move|_|read.run("0".into())>"Your possessions"</button>
        <Show when=move||open_panel.get()>
            <section class="native-owned-assets" aria-label="Saved possessions">
                <button class="connected-text-button" type="button" on:click=move|_|close()>"Close possessions"</button>
                <h3>"Saved possessions"</h3>
                <p>{move||state.with(|s|s.avatars.iter().find(|a|Some(&a.avatar_id)==selected.get().as_ref()).map(|a|a.name.clone()).unwrap_or_default())}</p>
                <p role="status">{move||message.get()}</p>
                <Show when=move||page.with(|p|p.is_some())>
                    <dl><dt>"Saved balance"</dt><dd>{move||page.with(|p|p.as_ref().map(|p|format!("{} Simoleons",p.balance)).unwrap_or_default())}</dd></dl>
                    <h4>"Owned outfits"</h4>
                    <ul class="native-owned-assets-list">
                        <For each=move||page.with(|p|p.as_ref().map(|p|p.outfits.clone()).unwrap_or_default()) key=|o|o.id.clone() children=move|o|view!{
                            <li data-owned-outfit=o.id.clone()><strong>{match o.suit_type {0=>"Everyday",5=>"Sleepwear",2=>"Swimwear",_=>"Outfit"}}</strong>
                                <span>{format!("Owned instance #{}",o.id)}</span><small>{format!("Asset {}",o.asset_key)}</small></li>
                        }/>
                    </ul>
                    <Show when=move||page.with(|p|p.as_ref().is_some_and(|p|p.outfits.is_empty()))><p>"No owned outfits on this page."</p></Show>
                </Show>
                <div class="native-assets-actions">
                    <button class="chrome" type="button" disabled=move||busy.get() on:click=move|_|read.run("0".into())>"Refresh possessions"</button>
                    <button class="chrome" type="button" disabled=move||busy.get()||page.with(|p|p.as_ref().and_then(|p|p.next.as_ref()).is_none())
                        on:click=move|_|if let Some(next)=page.with_untracked(|p|p.as_ref().and_then(|p|p.next.clone())){read.run(next)}>"Next outfits"</button>
                </div>
            </section>
        </Show>
    }
}

/// Browse only the authenticated destination projection. Entry and departure
/// authority stay in the existing account provider and native runtime.
#[component]
fn PropertyDirectory(
    state: RwSignal<Snapshot>,
    selected: RwSignal<Option<String>>,
    owned: NativeOwnedUi,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let chosen = RwSignal::new(None::<String>);
    let pending = Memo::new(move |_| state.with(|s| s.entry_pending.clone()));
    let busy = Memo::new(move |_| {
        state.with(|s| !matches!(s.state.as_str(), "account_ready" | "account_error"))
    });
    let locked = move || busy.get() || pending.get().is_some();
    Effect::new(move |_| {
        selected.track();
        query.set(String::new());
        chosen.set(None);
    });
    Effect::new(move |_| {
        if let Some(entry) = pending.get() {
            if selected.get_untracked().as_ref() != Some(&entry.avatar_id) {
                selected.set(Some(entry.avatar_id));
            }
            chosen.set(Some(entry.lot_id));
        } else if state.with(|s| s.state == "account_refreshing") {
            chosen.set(None);
        }
    });
    let properties = Memo::new(move |_| {
        let needle = query.get().trim().to_lowercase();
        state.with(|s| {
            s.destinations
                .iter()
                .filter(|p| p.name.to_lowercase().contains(&needle) || p.lot_id == needle)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let property = Memo::new(move |_| {
        chosen.get().and_then(|id| {
            state.with(|s| s.destinations.iter().find(|p| p.lot_id == id).cloned())
        })
    });
    let visit = move |_| {
        if let Some(entry) = pending.get_untracked() {
            owned.invoke("enter", &entry.avatar_id, &entry.lot_id);
        } else if let (Some(avatar), Some(property)) =
            (selected.get_untracked(), property.get_untracked())
        {
            owned.invoke("enter", &avatar, &property.lot_id);
        }
    };
    view! {
        <section class="native-property-directory" aria-label="Property directory">
            <h3>"Property directory"</h3>
            <p>"Find a place to visit with your selected Sim."</p>
            <label for="native-property-search">"Find a property"</label>
            <input id="native-property-search" type="search" maxlength="96" disabled=locked
                prop:value=move||query.get() on:input=move|e|{query.set(event_target_value(&e));chosen.set(None);}/>
            <div class="native-property-list" aria-label="Properties">
                <For each=move||properties.get() key=|p|p.lot_id.clone() children=move|p|{
                    let id=p.lot_id.clone();let pressed=p.lot_id.clone();
                    view!{<button class="connected-directory-row native-property-row" type="button" aria-label=format!("View {}",p.name)
                        aria-pressed=move||(chosen.get().as_ref()==Some(&pressed)).to_string() disabled=locked
                        on:click=move|_|chosen.set(Some(id.clone()))>
                        <span class="connected-row-text"><strong>{p.name.clone()}</strong><small>{format!("Property #{}",p.lot_id)}</small></span>
                    </button>}
                }/>
            </div>
            <Show when=move||properties.get().is_empty()><p role="status">"No properties match your search."</p></Show>
            <Show when=move||property.get().is_some()>
                <aside class="native-property-placard" aria-label="Property details">
                    <button class="connected-text-button" type="button" disabled=locked on:click=move|_|chosen.set(None)>"Back to properties"</button>
                    <h4>{move||property.get().map(|p|p.name).unwrap_or_default()}</h4>
                    <dl><dt>"Property"</dt><dd>{move||property.get().map(|p|format!("#{}",p.lot_id)).unwrap_or_default()}</dd>
                        <dt>"Location"</dt><dd>{move||property.get().map(|p|format!("{}, {}",p.lot_location>>16,p.lot_location&65535)).unwrap_or_default()}</dd></dl>
                    <Show when=move||pending.get().is_some()><p role="status">"Entry is unconfirmed. Retry this same Sim and property before choosing another."</p></Show>
                    <button class="chrome primary" type="button" disabled=move||busy.get() on:click=visit>
                        {move||property.get().map(|p|format!("{} {}",if pending.get().is_some(){"Retry entering"}else{"Enter"},p.name)).unwrap_or_default()}
                    </button>
                </aside>
            </Show>
        </section>
    }
}
