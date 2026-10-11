//! Mounted native Buy/Build controller. Prices/grants are never inferred from a
//! local resource pack. Only the admitted replica can commit visible geometry.
use crate::{
    components::Icon,
    connected_bridge::ConnectedUi,
    native_build::{Catalogue, Draft, Item},
    world_draft::WorldDraftOutline,
};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use wonderland_game_runtime::{
    RuntimeProjection,
    live_wire::{
        construction::{
            ConstructionConsent,
            exchange::client::{Client, ClientStage, Prepared},
        },
        player::PlayerBinding,
    },
};
use wonderland_world_view::{ViewportControls, WorldDocument, WorldPick};
#[wasm_bindgen(inline_js = r#"
export async function createNativeConstruction(endpoint,origin,csrf,bearer) {
 const {createConstructionTransport}=await import('/native-construction.mjs');
 return createConstructionTransport(endpoint,{allowedOrigin:origin,csrfToken:csrf,bearerToken:bearer});
}
export function sendNativeConstruction(host,bytes){return host.send(bytes);}
export function closeNativeConstruction(host){host?.dispose();}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=createNativeConstruction)]
    fn create_transport(endpoint: &str, origin: &str, csrf: &str, bearer: &str) -> js_sys::Promise;
    #[wasm_bindgen(catch,js_name=sendNativeConstruction)]
    fn send_transport(
        host: &JsValue,
        bytes: &js_sys::Uint8Array,
    ) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(js_name=closeNativeConstruction)]
    fn close_transport(host: &JsValue);
}
type Scope = (u64, PlayerBinding);
#[derive(Default)]
struct Resources {
    scope: Option<Scope>,
    generation: u64,
    client: Option<Client>,
    catalogue: Option<Catalogue>,
    draft: Draft,
    transport: Option<JsValue>,
    next_request: u64,
    online: bool,
    loading: bool,
    refreshed: Option<u64>,
}
impl Resources {
    fn interrupt(&mut self) {
        self.generation = self.generation.saturating_add(1);
        if let Some(h) = self.transport.take() {
            close_transport(&h);
        }
        if let Some(c) = self.client.as_mut() {
            let _ = c.disconnect();
        }
        self.loading = false;
        self.online = false;
    }
    fn close(&mut self) {
        self.interrupt();
        if let Some(c) = self.client.as_mut() {
            c.close();
        }
        self.client = None;
        self.catalogue = None;
        self.draft = Draft::default();
        self.scope = None;
        self.refreshed = None;
        self.next_request = 0;
    }
    fn stage(&self) -> ClientStage {
        self.client
            .as_ref()
            .map_or(ClientStage::Idle, Client::stage)
    }
    fn mutable(&self) -> bool {
        matches!(
            self.stage(),
            ClientStage::Idle
                | ClientStage::Committed
                | ClientStage::Rejected
                | ClientStage::Cancelled
                | ClientStage::Expired
        ) && !self.loading
    }
}
#[derive(Clone, Copy)]
pub(crate) struct BuildUi {
    ui: ConnectedUi,
    resources: StoredValue<Resources, LocalStorage>,
    pub open: RwSignal<bool>,
    pub revision: RwSignal<u64>,
    pub notice: RwSignal<String>,
    world: RwSignal<Option<Arc<WorldDocument>>>,
    projection: RwSignal<Option<Arc<RuntimeProjection>>>,
    live: RwSignal<bool>,
    browsing: RwSignal<bool>,
    query: RwSignal<String>,
    category: RwSignal<String>,
    page: RwSignal<usize>,
}
impl BuildUi {
    pub fn new(
        ui: ConnectedUi,
        world: RwSignal<Option<Arc<WorldDocument>>>,
        projection: RwSignal<Option<Arc<RuntimeProjection>>>,
        live: RwSignal<bool>,
    ) -> Self {
        let this = Self {
            ui,
            resources: StoredValue::new_local(Resources::default()),
            open: RwSignal::new(false),
            revision: RwSignal::new(0),
            notice: RwSignal::new(String::new()),
            world,
            projection,
            live,
            browsing: RwSignal::new(true),
            query: RwSignal::new(String::new()),
            category: RwSignal::new(String::new()),
            page: RwSignal::new(0),
        };
        on_cleanup(move || {
            this.resources.try_update_value(Resources::close);
        });
        this
    }
    fn touch(self) {
        self.revision.try_update(|v| *v = v.saturating_add(1));
    }
    pub fn sync(self, scope: Option<Scope>) {
        let live = self.live.get_untracked();
        let changed=self.resources.try_update_value(|r|{
   if r.scope!=scope {r.close();r.scope=scope;self.open.try_set(false);self.browsing.try_set(true);return true;}
   if !live && (r.online || r.loading){r.interrupt();return true;}
   if live && r.online && let (Some(c),Some(p))=(r.client.as_mut(),self.projection.get_untracked()){
    match c.observe(c.connection(),&p){Ok(changed)=>return changed,Err(_)=>{r.interrupt();self.notice.try_set("Construction state could not be validated. Check this operation's status before any new purchase.".into());return true;}}
   }
   false
  }).unwrap_or(false);
        if changed {
            self.touch();
        }
        self.refresh_committed();
    }
    // Only a read, once per replica-confirmed commitment. Never resend consent.
    fn refresh_committed(self) {
        let refresh = self
            .resources
            .try_update_value(|r| {
                if !r.online || r.loading {
                    return false;
                }
                let Some(client) = r.client.as_ref() else {
                    return false;
                };
                if client.stage() != ClientStage::Committed {
                    return false;
                }
                let Some(id) = client.quote().map(|q| q.consent.request_id) else {
                    return false;
                };
                if r.refreshed == Some(id) {
                    return false;
                }
                r.refreshed = Some(id);
                r.draft = Draft::default();
                true
            })
            .unwrap_or(false);
        if refresh {
            self.browsing.set(true);
            self.load(false);
        }
    }
    pub fn show(self) {
        self.open.set(true);
        if self
            .resources
            .with_value(|r| r.catalogue.is_none() && !r.loading)
        {
            self.load(false);
        }
    }
    pub fn hide(self) {
        self.open.set(false);
    }
    fn current(self, generation: u64) -> bool {
        self.live.try_get_untracked() == Some(true)
            && self
                .resources
                .try_with_value(|r| {
                    r.generation == generation
                        && r.scope
                            .is_some_and(|(e, b)| self.ui.native_construction_is_current(e, b))
                })
                .unwrap_or(false)
    }
    pub fn load(self, recover: bool) {
        if !self.live.get_untracked() {
            self.notice
                .set("Reconnect to the property before loading construction.".into());
            return;
        }
        let context = self
            .resources
            .try_update_value(|r| {
                if r.loading {
                    return None;
                }
                let scope = r.scope?;
                r.interrupt();
                if r.generation == u64::MAX {
                    return None;
                }
                r.loading = true;
                Some((r.generation, scope))
            })
            .flatten();
        let Some((generation, (epoch, binding))) = context else {
            return;
        };
        self.notice.set("Loading this property's catalogue…".into());
        self.touch();
        spawn_local(async move {
            let access = self.ui.native_construction_access(epoch, binding).await;
            if !self.current(generation) {
                return;
            }
            let result = match access {
                Ok(access) => {
                    let valid = self.world.with_untracked(|w| {
                        w.as_ref()
                            .ok_or("No admitted property")
                            .and_then(|w| access.catalogue.validate(binding, w))
                    });
                    match valid {
                        Err(e) => Err(e.to_owned()),
                        Ok(()) => match JsFuture::from(create_transport(
                            &access.endpoint,
                            &access.origin,
                            &access.catalogue.csrf_token,
                            &access.bearer,
                        ))
                        .await
                        {
                            Ok(host) => Ok((host, access.catalogue)),
                            Err(_) => Err("Construction transport could not start.".into()),
                        },
                    }
                }
                Err(e) => Err(e),
            };
            if !self.current(generation) {
                if let Ok((host, _)) = result {
                    close_transport(&host);
                }
                return;
            }
            let installed=self.resources.try_update_value(|r|{
    r.loading=false;
    match result {
     Ok((host,catalogue))=>{
      let admitted=if let Some(client)=r.client.as_mut(){client.reconnect().map(|_|())}else{Client::new(binding,catalogue.lot_id,catalogue.authority_epoch).map(|c|{r.client=Some(c);})};
      if admitted.is_err(){close_transport(&host);self.notice.try_set("This construction session cannot resume; contact the world operator.".into());return false;}
      r.next_request=r.next_request.max(catalogue.next_request_id);r.catalogue=Some(catalogue);r.transport=Some(host);r.online=true;
      self.notice.try_set(String::new());true
     },Err(e)=>{self.notice.try_set(e);false}
    }
   }).unwrap_or(false);
            self.touch();
            if installed && recover {
                self.poll();
            }
        });
    }
    pub fn pick(self, pick: WorldPick) {
        if !self.open.get_untracked() {
            return;
        }
        let result = self.resources.try_update_value(|r| {
            if !r.mutable() || !r.online {
                return Err("Finish or recover the current quote before changing placement");
            }
            let w = self.world.get_untracked().ok_or("Property unavailable")?;
            r.draft.pick(&pick, &w)
        });
        match result {
            Some(Err(e)) => self.notice.set(e.into()),
            Some(Ok(())) => {
                self.notice.set(String::new());
                self.touch();
            }
            None => {}
        }
    }
    fn choose(self, item: Item) {
        self.resources.update_value(|r| {
            if r.mutable() && r.online {
                r.draft.choose(item);
                self.browsing.set(false);
                self.notice.set(String::new());
            }
        });
        self.touch();
    }
    fn rotate(self) {
        self.resources.update_value(|r| {
            if r.mutable() {
                r.draft.rotate();
            }
        });
        self.touch();
    }
    fn failed(self, generation: u64) {
        if self.current(generation) {
            self.resources.try_update_value(Resources::interrupt);
            self.notice.try_set("Delivery is unknown. Recheck this same operation; it will not be submitted again automatically.".into());
            self.touch();
        }
    }
    fn dispatch(self, prepared: Prepared) {
        let host = self
            .resources
            .with_value(|r| r.transport.clone().map(|h| (r.generation, h)));
        let Some((generation, host)) = host else {
            self.notice
                .set("Reload construction before continuing.".into());
            return;
        };
        self.touch();
        spawn_local(async move {
            let result =
                match send_transport(&host, &js_sys::Uint8Array::from(prepared.bytes.as_slice())) {
                    Ok(p) => JsFuture::from(p).await,
                    Err(e) => Err(e),
                };
            if !self.current(generation) {
                return;
            }
            match result {
                Ok(value) => {
                    let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() else {
                        self.failed(generation);
                        return;
                    };
                    let bytes = bytes.to_vec();
                    let accepted = self.resources.try_update_value(|r| {
                        let c = r.client.as_mut().ok_or(())?;
                        // Accepted replica completion may have arrived before the HTTP reply.
                        if c.waiting_for_reply() {
                            c.accept(prepared.connection, &bytes).map_err(|_| ())?;
                        }
                        if let Some(p) = self.projection.get_untracked() {
                            c.observe(c.connection(), &p).map_err(|_| ())?;
                        }
                        Ok::<(), ()>(())
                    });
                    if !matches!(accepted, Some(Ok(()))) {
                        self.failed(generation);
                    } else {
                        self.notice.try_set(String::new());
                        self.touch();
                        self.refresh_committed();
                    }
                }
                Err(_) => self.failed(generation),
            }
        });
    }
    fn quote(self) {
        let result = self.resources.try_update_value(|r| {
            if !r.online || !r.mutable() {
                return Err("Construction is not ready");
            }
            let w = self.world.get_untracked().ok_or("Property unavailable")?;
            let cap = r.catalogue.as_ref().ok_or("Load the catalogue first")?;
            let next = r
                .next_request
                .checked_add(1)
                .ok_or("Request sequence exhausted")?;
            let request = r.draft.request(cap, &w, r.next_request)?;
            let prepared = r
                .client
                .as_mut()
                .ok_or("Construction unavailable")?
                .begin(request)
                .map_err(|_| "Finish the previous quote first")?;
            r.next_request = next;
            Ok(prepared)
        });
        match result {
            Some(Ok(p)) => self.dispatch(p),
            Some(Err(e)) => self.notice.set(e.into()),
            None => {}
        }
    }
    fn confirm(self, reviewed: ConstructionConsent) {
        let result = self
            .resources
            .try_update_value(|r| {
                if !r.online {
                    return None;
                }
                r.client.as_mut()?.confirm(reviewed).ok()
            })
            .flatten();
        if let Some(p) = result {
            self.dispatch(p);
        }
    }
    fn cancel(self) {
        let result = self
            .resources
            .try_update_value(|r| {
                if !r.online {
                    return None;
                }
                r.client.as_mut()?.cancel().ok()
            })
            .flatten();
        if let Some(p) = result {
            self.dispatch(p);
        }
    }
    fn poll(self) {
        let result = self
            .resources
            .try_update_value(|r| r.client.as_mut()?.poll().ok())
            .flatten();
        if let Some(p) = result {
            self.dispatch(p);
        }
    }
    pub fn outline(self) -> WorldDraftOutline {
        self.revision.track();
        if !self.open.get() {
            return WorldDraftOutline::default();
        }
        self.world.with(|w| {
            w.as_ref()
                .map(|w| self.resources.with_value(|r| r.draft.outline(w)))
                .unwrap_or_default()
        })
    }
    fn stage(self) -> ClientStage {
        self.revision.track();
        self.resources.with_value(Resources::stage)
    }
    fn editable(self) -> bool {
        self.revision.track();
        self.live.get()
            && self.resources.with_value(|r| {
                r.online && r.mutable() && r.catalogue.as_ref().is_some_and(|c| c.can_build)
            })
    }
    fn items(self) -> Vec<Item> {
        self.revision.track();
        let q = self.query.get();
        let cat = self.category.get();
        let page = self.page.get();
        self.resources.with_value(|r| {
            r.catalogue
                .as_ref()
                .map(|c| c.page(&q, &cat, page, 8))
                .unwrap_or_default()
        })
    }
    fn categories(self) -> Vec<String> {
        self.revision.track();
        self.resources.with_value(|r| {
            r.catalogue
                .as_ref()
                .map(|c| {
                    c.items
                        .iter()
                        .map(|i| i.category.clone())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect()
                })
                .unwrap_or_default()
        })
    }
    fn selection(self) -> Option<Item> {
        self.revision.track();
        self.resources.with_value(|r| r.draft.item.clone())
    }
    fn status_text(self) -> &'static str {
        match self.stage() {
            ClientStage::Idle => "Choose an item, then tap its place on the property.",
            ClientStage::Quoting => "Checking placement and current price…",
            ClientStage::Reviewing => "Review the exact price before confirming.",
            ClientStage::Sending => "Confirmation sent. Waiting for the server…",
            ClientStage::Cancelling => "Cancelling the unsubmitted quote…",
            ClientStage::Pending => "Accepted; waiting for the committed property state.",
            ClientStage::Unknown => "Result unknown. Recheck status before another purchase.",
            ClientStage::AwaitingReplica => {
                "Server result received; waiting for matching game state."
            }
            ClientStage::Committed => "Construction committed in the property.",
            ClientStage::Rejected => "Construction rejected. No successful placement is reported.",
            ClientStage::Reconciliation => {
                "The operation needs reconciliation by the world operator."
            }
            ClientStage::Cancelled => "Quote cancelled. Nothing was submitted.",
            ClientStage::Expired => "Quote expired. Review a new price before confirming.",
            ClientStage::Closed => "Construction session closed.",
        }
    }
}
#[component]
pub(crate) fn BuildPanel(ctl: BuildUi, controls: RwSignal<ViewportControls>) -> impl IntoView {
    let build = ctl;
    view! {<Show when=move ||build.open.get()>
    <aside class="native-build-panel" aria-label="Buy and Build" data-build-stage=move ||format!("{:?}",build.stage())>
     <header><div><span class="eyebrow">"MAKE IT YOURS"</span><h2>"Buy / Build"</h2></div><button class="chrome round small" aria-label="Close Buy and Build" on:click=move |_|build.hide()><Icon name="x"/></button></header>
     <div class="native-build-view"><button class="chrome" aria-label="Rotate property view" on:click=move |_|controls.update(|v|v.yaw_radians+=std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise"/>"View"</button><button class="chrome round small" aria-label="Build floor down" disabled=move ||controls.get().visible_level<=1 on:click=move |_|controls.update(|v|v.visible_level=v.visible_level.saturating_sub(1).max(1))>"−"</button><button class="chrome round small" aria-label="Build floor up" disabled=move ||build.world.with(|w|w.as_ref().is_none_or(|w|controls.get().visible_level>=w.lot.levels)) on:click=move |_|{let n=build.world.with_untracked(|w|w.as_ref().map_or(1,|w|w.lot.levels));controls.update(|v|v.visible_level=(v.visible_level+1).min(n));}>"+"</button><span>{move ||format!("Floor {}",controls.get().visible_level)}</span></div>
     <p class="native-build-balance">{move ||{build.revision.track();build.resources.with_value(|r|r.catalogue.as_ref().map(|c|format!("Catalogue balance · ${}",c.balance)).unwrap_or_default())}}</p>
     <Show when=move ||build.browsing.get()>
      <label for="native-build-search">"Search catalogue"</label><input id="native-build-search" type="search" maxlength="160" prop:value=move ||build.query.get() on:input=move |e|{build.query.set(event_target_value(&e));build.page.set(0);}/>
      <div class="native-build-categories" role="group" aria-label="Catalogue categories"><button class="chrome" aria-pressed=move ||build.category.get().is_empty().to_string() on:click=move |_|{build.category.set(String::new());build.page.set(0);}>"All"</button><For each=move ||build.categories() key=|s|s.clone() children=move |name|{let pressed=name.clone();let selected=name.clone();view!{<button class="chrome" aria-pressed=move ||(build.category.get()==pressed).to_string() on:click=move |_|{build.category.set(selected.clone());build.page.set(0);}>{name}</button>}}/></div>
      <div class="native-build-grid"><For each=move ||build.items() key=Item::key children=move |item|{let selected=item.clone();let label=item.label.clone();let thumb=item.thumbnail.clone();view!{<button class="native-build-item" aria-label=format!("Select {}",item.label) disabled=move ||!build.editable() on:click=move |_|build.choose(selected.clone())>{match thumb{Some(src)=>view!{<img src=src alt="" loading="lazy" referrerpolicy="no-referrer"/>}.into_any(),None=>view!{<span class="native-build-no-thumbnail" aria-hidden="true"><Icon name="home"/></span>}.into_any()}}<strong>{label}</strong><span>{format!("${}",item.price)}</span></button>}}/></div>
      <Show when=move ||build.items().is_empty()><p>"No matching catalogue items are available."</p></Show>
      <nav class="native-build-pages" aria-label="Catalogue pages"><button class="chrome" disabled=move ||build.page.get()==0 on:click=move |_|build.page.update(|p|*p=p.saturating_sub(1))>"Previous"</button><span>{move ||format!("Page {}",build.page.get()+1)}</span><button class="chrome" disabled=move ||{build.revision.track();let q=build.query.get();let cat=build.category.get();let next=build.page.get().saturating_add(1);build.resources.with_value(|r|r.catalogue.as_ref().is_none_or(|c|c.page(&q,&cat,next,8).is_empty()))} on:click=move |_|build.page.update(|p|*p=p.saturating_add(1))>"Next"</button></nav>
     </Show>
     <Show when=move ||build.selection().is_some()>
      <div class="native-build-selection"><h3>{move ||build.selection().map(|i|i.label).unwrap_or_default()}</h3><p>{move ||{build.revision.track();build.resources.with_value(|r|r.draft.tile.map(|t|format!("Selected tile {}, {} · Floor {}",t.x+1,t.y+1,t.level)).unwrap_or_else(||"Tap an empty property tile to select placement.".into()))}}</p><div class="native-build-row"><button class="chrome" disabled=move ||!build.editable() on:click=move |_|build.rotate()>"Rotate item"</button><button class="chrome" disabled=move ||!build.editable() on:click=move |_|build.browsing.set(true)>"Change item"</button></div></div>
     </Show>
     <p class="native-build-status" role="status" aria-live="polite">{move ||build.status_text()}</p>
     <Show when=move ||!build.notice.get().is_empty()><p class="native-build-notice" role="status">{move ||build.notice.get()}</p></Show>
     <Show when=move ||build.editable() && build.selection().is_some()><button class="chrome native-build-primary" disabled=move ||{build.revision.track();build.resources.with_value(|r|r.draft.tile.is_none())} on:click=move |_|build.quote()>"Review placement & price"</button></Show>
     {move ||{build.revision.track();let quote=build.resources.with_value(|r|r.client.as_ref().filter(|c|c.stage()==ClientStage::Reviewing).and_then(|c|c.quote()).cloned());quote.map(|q|{let reviewed=q.consent;view!{<section class="native-build-consent" aria-label="Exact construction quote"><strong>{format!("Total · ${}",reviewed.cost)}</strong><p>"Only confirm this exact quote. Placement appears after the game confirms commitment."</p><button class="chrome native-build-primary" disabled=move ||!build.live.get() on:click=move |_|build.confirm(reviewed)>{format!("Confirm ${}",reviewed.cost)}</button><button class="chrome" disabled=move ||!build.live.get() on:click=move |_|build.cancel()>"Cancel quote"</button></section>}})}}
     <Show when=move ||matches!(build.stage(),ClientStage::Unknown|ClientStage::Pending|ClientStage::AwaitingReplica|ClientStage::Reconciliation)><button class="chrome native-build-primary" disabled=move ||!build.live.get() on:click=move |_|build.load(true)>"Recheck operation status"</button></Show>
     <button class="native-build-refresh" disabled=move ||!build.live.get() || {build.revision.track();build.resources.with_value(|r|r.loading||!r.mutable())} on:click=move |_|build.load(false)>"Refresh catalogue and balance"</button>
    </aside></Show>}
}
