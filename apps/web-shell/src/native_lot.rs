//! Explicit native lot view. Legacy source snapshots never enter this decoder.
use crate::{components::Icon, connected_bridge::ConnectedUi, world_renderer::WorldViewport};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use wonderland_game_runtime::{EntityRef, ObjectId, RuntimeProjection};
use wonderland_game_runtime::sim_core::interactions::InteractionKey;
use wonderland_game_runtime::live_wire::{encode_checkpoint_request, player::{ActionStatus, NativePlayer, PlayerBinding}};
use wonderland_game_services::{SessionState, GatewayOperation};
use wonderland_world_view::{WorldDocument, ViewportControls, WorldPick, WorldPickTarget, WallMode};

#[wasm_bindgen(inline_js = r#"
export async function openNativePlayerSocket(url,ticket,frame,state,origin,resume,signal) {
  const module=await import('/native-socket.mjs');
  if(signal.aborted) return null;
  const host=module.connectNativeSocket(url,ticket,frame,state,{allowedOrigin:origin,resume});
  const abort=()=>host.dispose();signal.addEventListener('abort',abort,{once:true});
  return {ready:()=>host.ready(),send:b=>host.send(b),control:b=>host.control(b),dispose:()=>{signal.removeEventListener('abort',abort);host.dispose();}};
}
export function nativeHostCall(host,method,bytes) {
  if(!host || typeof host[method]!=='function') return false;
  return bytes == null ? host[method]() : host[method](bytes);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name=openNativePlayerSocket)]
    fn open_socket(url:&str,ticket:&str,frame:&js_sys::Function,state:&js_sys::Function,origin:&str,resume:bool,signal:&web_sys::AbortSignal)->js_sys::Promise;
    #[wasm_bindgen(catch,js_name=nativeHostCall)]
    fn call_host(host:&JsValue,method:&str,bytes:&JsValue)->Result<JsValue,JsValue>;
}

#[derive(Clone)]
struct Choice { target:EntityRef, key:InteractionKey, param0:i16, label:String }
#[derive(Default)]
struct Resources {
    generation:u64,
    scope:Option<(u64,PlayerBinding)>,
    player:Option<NativePlayer>,
    host:Option<JsValue>,
    abort:Option<web_sys::AbortController>,
    frame:Option<Closure<dyn FnMut(js_sys::Uint8Array)->JsValue>>,
    state:Option<Closure<dyn FnMut(String)>>,
    last_request:u64,
}
impl Resources {
    fn stop_socket(&mut self) {
        if let Some(abort)=self.abort.take(){abort.abort();}
        if let Some(host)=self.host.take(){let _=call_host(&host,"dispose",&JsValue::NULL);}
        self.frame=None;self.state=None;
    }
    fn close(&mut self){self.stop_socket();if let Some(player)=self.player.as_mut(){player.close();}self.player=None;self.scope=None;}
}
#[derive(Clone,Copy)]
struct Controller {
    ui:ConnectedUi,
    resources:StoredValue<Resources,LocalStorage>,
    world:RwSignal<Option<Arc<WorldDocument>>>,
    projection:RwSignal<Option<Arc<RuntimeProjection>>>,
    choices:RwSignal<Vec<Choice>>,
    live:RwSignal<bool>,
    status:RwSignal<ActionStatus>,
    notice:RwSignal<String>,
    wake:RwSignal<u64>,
}
fn scope(ui:ConnectedUi)->Option<(u64,PlayerBinding)> {
    ui.state.try_with_untracked(|state| {
        let session=state.session.as_ref()?;
        if !state.ledger.authenticated || !state.ledger.transport_ready || session.state!=SessionState::LotReady{return None;}
        Some((state.ledger.epoch,PlayerBinding{source_epoch:session.epoch,lot_incarnation:session.lot_incarnation?,lot_location:session.lot_location?,avatar_id:session.avatar_id?}))
    }).flatten()
}
impl Controller {
    fn current(self,generation:u64)->bool {
        self.resources.try_with_value(|r|r.generation==generation && r.scope.is_some() && r.scope==scope(self.ui)).unwrap_or(false)
    }
    fn host(self,method:&str,bytes:Option<&[u8]>)->bool {
        let value=bytes.map(|v|JsValue::from(js_sys::Uint8Array::from(v))).unwrap_or(JsValue::NULL);
        self.resources.try_with_value(|r|r.host.clone()).flatten().is_some_and(|host|call_host(&host,method,&value).ok().and_then(|v|v.as_bool()).unwrap_or(false))
    }
    fn fail(self,message:&str) {
        self.live.try_set(false);self.notice.try_set(message.into());self.choices.try_set(Vec::new());
        self.resources.try_update_value(|r|{if let Some(player)=r.player.as_mut(){player.disconnect();self.status.try_set(player.status());}});
    }
    fn begin(self,new_scope:Option<(u64,PlayerBinding)>) {
        let Some((browser_epoch,binding))=new_scope else {
            self.resources.try_update_value(Resources::close);self.live.set(false);self.world.set(None);self.projection.set(None);self.choices.set(Vec::new());return;
        };
        let Some((generation,resume))=self.resources.try_update_value(|r| {
            r.stop_socket();
            r.generation=r.generation.checked_add(1).unwrap_or(u64::MAX);
            if r.scope!=new_scope{r.player=None;r.last_request=0;}
            let resume=r.player.is_some();
            if let Some(player)=r.player.as_mut(){player.disconnect();if player.reconnect().is_err(){r.player=None;}}
            r.scope=new_scope;r.last_request=0;(r.generation,resume)
        }) else{return;};
        self.live.set(false);self.choices.set(Vec::new());self.notice.set("Connecting to this property’s native runtime…".into());
        spawn_local(async move {
            let result=self.ui.admit_native_lot(browser_epoch,binding).await;
            if !self.current(generation){return;}
            let (url,origin,ticket)=match result{Ok(v)=>v,Err(message)=>{self.fail(&message);return;}};
            let Ok(abort)=web_sys::AbortController::new() else{self.fail("This browser could not create the native connection.");return;};
            let on_frame=Closure::wrap(Box::new(move |bytes:js_sys::Uint8Array|->JsValue {
                if !self.current(generation){return JsValue::FALSE;}
                let payload=bytes.to_vec();
                let processed=self.resources.try_update_value(|r|->Result<(Option<Vec<u8>>,bool),&'static str>{
                    if let Some(player)=r.player.as_mut(){player.receive(&payload)?;}else{r.player=Some(NativePlayer::open(&payload,binding,browser_epoch)?);}
                    let player=r.player.as_ref().ok_or("Native player is unavailable")?;
                    self.status.try_set(player.status());
                    if let Some(request)=player.checkpoint_request(){
                        if r.last_request!=request.id {r.last_request=request.id;return Ok((Some(encode_checkpoint_request(request).map_err(|_|"Recovery request failed")?),false));}
                        return Ok((None,false));
                    }
                    Ok((None,true))
                });
                match processed {
                    Some(Ok((reply,ready)))=>{
                        if ready{self.live.try_set(true);self.notice.try_set(String::new());self.host("ready",None);self.wake.try_update(|n|*n=n.saturating_add(1));}
                        reply.map(|v|JsValue::from(js_sys::Uint8Array::from(v.as_slice()))).unwrap_or(JsValue::NULL)
                    },
                    _=>{self.fail("The native stream could not be validated. Reconnect to recover; no action will be retried automatically.");JsValue::FALSE}
                }
            }) as Box<dyn FnMut(js_sys::Uint8Array)->JsValue>);
            let on_state=Closure::wrap(Box::new(move |value:String| {
                if !self.current(generation){return;}
                if value=="failed" || value=="disconnected" {self.fail("Connection interrupted. The displayed property is the last accepted state. Reconnect to continue.");}
                if value=="connected" && resume {
                    let request=self.resources.try_with_value(|r|r.player.as_ref().and_then(|p|p.checkpoint_request())).flatten();
                    if let Some(request)=request {
                        if let Ok(bytes)=encode_checkpoint_request(request){if !self.host("control",Some(&bytes)){self.fail("The recovery request could not be sent.");}}
                    }
                }
            }) as Box<dyn FnMut(String)>);
            let promise=open_socket(&url,&ticket,on_frame.as_ref().unchecked_ref(),on_state.as_ref().unchecked_ref(),&origin,resume,&abort.signal());
            self.resources.update_value(|r|{r.abort=Some(abort);r.frame=Some(on_frame);r.state=Some(on_state);});
            match JsFuture::from(promise).await {
                Ok(host) if self.current(generation) && !host.is_null()=>{self.resources.update_value(|r|r.host=Some(host));},
                Ok(host)=>{if !host.is_null(){let _=call_host(&host,"dispose",&JsValue::NULL);}},
                Err(_) if self.current(generation)=>self.fail("The native browser connection could not start."),
                Err(_)=>{},
            }
        });
    }
    fn select(self,target:EntityRef) {
        if !self.live.get_untracked(){return;}
        let result=self.resources.with_value(|r|r.player.as_ref().ok_or("Native player is unavailable").and_then(|p|p.offers(target)));
        match result{Ok(batch)=>self.choices.set(batch.offers.into_iter().map(|offer|Choice{target,key:offer.interaction,param0:offer.param0,label:offer.label}).collect()),Err(message)=>self.notice.set(message.into())}
    }
    fn pick(self,pick:WorldPick) {
        if !self.live.get_untracked(){return;}
        let valid=self.world.with_untracked(|w|w.as_ref().is_some_and(|w|w.revision.lot_id==pick.revision.lot_id && w.revision.epoch==pick.revision.epoch && w.revision.content==pick.revision.content));
        if !valid{return;}
        if let WorldPickTarget::Object{entity:Some(entity),source_guid,..}=pick.target {
            let Ok(id)=i16::try_from(entity.object_id) else{return;};
            let target=EntityRef{object_id:ObjectId(id),generation:entity.generation};
            let exists=self.projection.with_untracked(|p|p.as_ref().is_some_and(|p|p.entities.iter().any(|e|e.reference==target && e.guid==source_guid)));
            if exists{self.select(target);}
        }
    }
    fn submit(self,choice:Option<Choice>,cancel:Option<u64>) {
        if !self.live.get_untracked(){return;}
        let result=self.resources.try_update_value(|r| {
            let player=r.player.as_mut().ok_or("Native player is unavailable")?;
            let bytes=if let Some(choice)=choice{player.prepare(choice.target,choice.key,choice.param0)?}else{player.prepare_cancel(cancel.ok_or("No selected action")?)?};
            self.status.set(player.status());Ok::<_,&'static str>(bytes)
        });
        match result {Some(Ok(bytes))=>{if !self.host("send",Some(&bytes)){self.fail("This action was not confirmed. Reconnect before continuing; it will not be retried.");}},Some(Err(message))=>self.notice.set(message.into()),None=>{}}
    }
}

#[component]
pub fn NativeLot()->impl IntoView {
    let ui=expect_context::<ConnectedUi>();
    let ctl=Controller{ui,resources:StoredValue::new_local(Resources::default()),world:RwSignal::new(None),projection:RwSignal::new(None),choices:RwSignal::new(Vec::new()),live:RwSignal::new(false),status:RwSignal::new(ActionStatus::Idle),notice:RwSignal::new(String::new()),wake:RwSignal::new(0)};
    let controls=RwSignal::new(ViewportControls::default());
    let needs_open=RwSignal::new(false);
    Effect::new(move |_|{ui.state.track();let current=scope(ui);if ctl.resources.with_value(|r|r.scope!=current){ctl.begin(current);}});
    Effect::new(move |_|{
        ctl.wake.track();
        if !ctl.live.get_untracked(){return;}
        let result=ctl.resources.with_value(|r|{
            let p=r.player.as_ref().ok_or("Native player is unavailable")?;
            Ok::<_,&'static str>((Arc::new(p.world()?),Arc::new(p.projection()?)))
        });
        match result{Ok((world,projection))=>{ctl.world.set(Some(world));ctl.projection.set(Some(projection));},Err(message)=>ctl.fail(message)}
    });
    on_cleanup(move ||{ctl.resources.try_update_value(Resources::close);});
    let on_pick=Callback::new(move |pick|ctl.pick(pick));
    view!{
        <section class="source-world-screen connected-world native-lot" aria-label="Native connected property" data-native-live=move ||ctl.live.get().to_string()>
            <Show when=move ||ctl.world.with(|w|w.is_some())><NativeScene world=ctl.world controls on_pick/></Show>
            <header class="source-world-header chrome"><button class="chrome round small" aria-label="Return to city" on:click=move |_|ui.send(GatewayOperation::LeaveLot,"Leave property",None)><Icon name="chevron-left"/></button><div><span class="eyebrow">"CONNECTED PROPERTY"</span><h1>"Property"</h1><p id="native-tick">{move ||ctl.projection.with(|p|p.as_ref().map(|p|format!("{} · Tick {}",if ctl.live.get(){"Live native simulation"}else{"Last accepted state"},p.tick)).unwrap_or_else(||"Waiting for native admission".into()))}</p></div><button class="chrome" on:click=move |_|ctl.begin(scope(ui))><Icon name="refresh"/>"Reconnect"</button></header>
            <Show when=move ||!ctl.notice.get().is_empty()><p class="source-world-notice chrome" role="status">{move ||ctl.notice.get()}</p></Show>
            <div class="connected-world-queue" aria-label="Your action queue"><For each=move ||ctl.projection.with(|p|p.as_ref().map(|p|p.queues.iter().flat_map(|q|q.entries.clone()).collect::<Vec<_>>()).unwrap_or_default()) key=|item|item.id children=move |item|{
                let id=item.id;view!{<div class="connected-world-action chrome"><span>{item.label.unwrap_or_else(||"Source action".into())}</span><button class="chrome round small" aria-label="Cancel this action" disabled=move ||!ctl.live.get()||ctl.status.get()==ActionStatus::Pending on:click=move |_|ctl.submit(None,Some(id))><Icon name="x"/></button></div>}
            }/></div>
            <nav class="source-world-tools chrome" aria-label="Property controls">
                <div class="source-control-group"><button class="chrome" on:click=move |_|controls.update(|v|v.yaw_radians-=std::f32::consts::FRAC_PI_4)>"Rotate left"</button><button class="chrome" on:click=move |_|controls.update(|v|v.yaw_radians+=std::f32::consts::FRAC_PI_4)>"Rotate right"</button><button class="chrome" on:click=move |_|controls.update(|v|v.zoom=(v.zoom/1.2).max(0.25))>"−"</button><button class="chrome" on:click=move |_|controls.update(|v|v.zoom=(v.zoom*1.2).min(8.))>"+"</button></div>
                <div class="source-control-group"><button class="chrome" disabled=move ||controls.get().visible_level<=1 on:click=move |_|controls.update(|v|v.visible_level=v.visible_level.saturating_sub(1).max(1))>"Floor down"</button><span>{move ||controls.get().visible_level}</span><button class="chrome" disabled=move ||ctl.world.with(|w|w.as_ref().is_none_or(|w|controls.get().visible_level>=w.lot.levels)) on:click=move |_|{let n=ctl.world.with_untracked(|w|w.as_ref().map(|w|w.lot.levels).unwrap_or(1));controls.update(|v|v.visible_level=v.visible_level.saturating_add(1).min(n));}>"Floor up"</button></div>
                <div class="source-control-group">{[(WallMode::Down,"Walls down"),(WallMode::Cutaway,"Cutaway"),(WallMode::Up,"Walls up")].into_iter().map(move |(mode,label)|view!{<button class="chrome" aria-pressed=move ||(controls.get().walls==mode).to_string() on:click=move |_|controls.update(|v|v.walls=mode)>{label}</button>}).collect_view()}<button class="chrome" on:click=move |_|controls.update(|v|v.show_roofs=!v.show_roofs)>"Roof"</button></div>
                <div class="source-control-group"><button class="chrome" disabled=move ||!ctl.live.get() on:click=move |_|{if let Some(actor)=ctl.resources.with_value(|r|r.player.as_ref().map(|p|p.actor())){ctl.select(actor);}}>"Your Sim"</button><button class="chrome" on:click=move |_|needs_open.update(|v|*v=!*v)>"Needs"</button><button class="chrome" disabled=true title="Native construction provider is not connected">"Buy / Build"</button></div>
            </nav>
            <Show when=move ||!ctl.choices.get().is_empty()><aside class="connected-world-authoring chrome" aria-label="Source actions"><header><h2>"Actions"</h2><button class="chrome round small" aria-label="Close source actions" on:click=move |_|ctl.choices.set(Vec::new())><Icon name="x"/></button></header>{move ||ctl.choices.get().into_iter().map(move |choice|{let label=choice.label.clone();view!{<button class="chrome native-source-action" disabled=move ||!ctl.live.get()||matches!(ctl.status.get(),ActionStatus::Pending|ActionStatus::Unknown) on:click=move |_|ctl.submit(Some(choice.clone()),None)>{label}</button>}}).collect_view()}<p role="status">{move ||match ctl.status.get(){ActionStatus::Idle=>"Choose a source action.",ActionStatus::Pending=>"Sent · awaiting server acceptance",ActionStatus::Accepted=>"Accepted by the server",ActionStatus::Rejected=>"The server rejected this action",ActionStatus::Unknown=>"Previous action result unknown · not retried"}}</p><Show when=move ||ctl.live.get()&&ctl.status.get()==ActionStatus::Unknown><button class="chrome" on:click=move |_|{ctl.resources.update_value(|r|{if let Some(p)=r.player.as_mut(){if p.dismiss_unknown().is_ok(){ctl.status.set(p.status());}}});}>"Dismiss unknown result without retrying"</button></Show></aside></Show>
            <Show when=move ||needs_open.get()><aside class="connected-world-needs chrome" aria-label="Live native needs"><header><h2>"Your Sim"</h2><button class="chrome round small" aria-label="Close needs" on:click=move |_|needs_open.set(false)><Icon name="x"/></button></header><div class="connected-source-needs">{move ||ctl.projection.with(|p|p.as_ref().and_then(|p|p.entities.iter().find(|e|e.persistent_id==scope(ui).map(|s|s.1.avatar_id).unwrap_or(0)).and_then(|e|e.needs.clone()))).map(|n|{
                [("Energy",Some(n.energy)),("Comfort",Some(n.comfort)),("Hunger",Some(n.hunger)),("Hygiene",Some(n.hygiene)),("Bladder",Some(n.bladder)),("Social",Some(n.social)),("Fun",Some(n.fun)),("Room",n.room)].into_iter().map(|(name,value)|view!{<label><span>{name}</span>{value.map(|value|view!{<progress max="100" value=value>{value}</progress>}.into_any()).unwrap_or_else(||view!{<span>"Unavailable"</span>}.into_any())}</label>}).collect_view()
            })}</div></aside></Show>
        </section>
    }
}
#[component]
fn NativeScene(world:RwSignal<Option<Arc<WorldDocument>>>,controls:RwSignal<ViewportControls>,on_pick:Callback<WorldPick>)->impl IntoView {
    let fallback=world.get_untracked().expect("NativeScene mounts only after a valid world projection");
    view!{<WorldViewport world=Signal::derive(move ||world.get().unwrap_or_else(||Arc::clone(&fallback))) controls on_pick/>}
}
