use crate::{
    bridge::Ui,
    components::{Icon, availability_reason},
    geometry::{Camera, Point, Size, clamp_overlay},
};
use leptos::prelude::*;
use wonderland_contracts::*;

#[component]
pub fn ObjectActions(camera: RwSignal<Camera>, object: SceneObject) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let anchor = object.anchor;
    let target = object.target.clone();
    let revision = object.revision;
    view! {
        <div id="object-actions" class="object-actions" role="group" aria-label=format!("{} actions", object.name) style=move || {
            let camera = camera.get();
            let point = camera.project(anchor);
            let narrow = camera.viewport.width <= 700.0;
            let size = if narrow { Size { width: 286.0, height: 264.0 } } else { Size { width: 310.0, height: 284.0 } };
            let position = clamp_overlay(Point { x: point.x + 114.0 * camera.scale() - size.width / 2.0, y: point.y - 73.0 * camera.scale() - size.height / 2.0 }, size, camera.viewport, if narrow { 145.0 } else { 78.0 }, if narrow { 242.0 } else { 150.0 });
            format!("left:{}px;top:{}px", position.x, position.y)
        } on:keydown=move |event: web_sys::KeyboardEvent| { if event.key() == "Escape" { event.prevent_default(); event.stop_propagation(); ui.dismiss_object(); } }>
            {object.offers.into_iter().map(|offer| {
                let action_id = offer.id.clone(); let pending_id = action_id.clone(); let active_id = action_id.clone();
                let target = target.clone(); let pending_target = target.clone(); let active_target = target.clone();
                let reason = availability_reason(&offer.availability);
                let available = reason.is_none();
                let disabled_reason = reason.clone();
                let (position, icon) = match offer.id.as_ref() { "make-coffee" => ("make primary", "coffee"), "clean" => ("clean", "brush"), _ => ("inspect", "search") };
                let pending = Memo::new(move |_| ui.state.with(|state| state.pending_requests.values().any(|pending| matches!(&pending.request.kind, RequestKind::Interaction { target, action_id, .. } if target == &pending_target && action_id == &pending_id))));
                let active = Memo::new(move |_| ui.state.with(|state| state.queue.iter().any(|queued| queued.target == active_target && queued.action_id == active_id)));
                view! {
                    <button id=format!("action-{}", offer.id) class=format!("action-petal chrome {position}") class:unavailable=!available aria-label=offer.label.clone()
                        aria-disabled=move || (!available || pending.get() || active.get()).to_string() aria-pressed=move || (pending.get() || active.get()).to_string()
                        title=reason.unwrap_or_default()
                        on:click=move |_| {
                            if let Some(reason) = &disabled_reason { ui.explain(reason.clone()); }
                            else if !pending.get_untracked() && !active.get_untracked() { ui.send(UiIntent::TakeOffer { target: target.clone(), action_id: action_id.clone(), expected_revision: revision }); }
                        }>
                        <Icon name=icon/><span>{offer.label.clone()}</span>
                        <Show when=move || pending.get() || active.get()><small>{move || if pending.get() { "Requested…" } else { "Queued" }}</small></Show>
                        {(!available).then(|| view! { <small>"Already clean"</small> })}
                    </button>
                }
            }).collect_view()}
            <div class="action-target chrome" aria-hidden="true"><img src="/assets/art/coffee-machine.png" alt=""/></div>
            <button class="action-close chrome round" aria-label="Close object actions" on:click=move |_| ui.dismiss_object()><Icon name="x"/></button>
        </div>
    }
}
