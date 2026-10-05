use crate::{
    authoring_bridge::AuthorUi,
    authoring_geometry::{HomeCamera, LotArtTransform, owned_action_position, placement_bounds},
    bridge::focus_later,
};
use leptos::prelude::*;
use wonderland_contracts::authoring::*;
#[component]
pub fn OwnedActions(camera: RwSignal<HomeCamera>, on_dismiss: Callback<()>) -> impl IntoView {
    let author = expect_context::<AuthorUi>();
    let selected = move || {
        author.state.with(|s| {
            let home = s.projection().home(s.selected_home()?)?;
            let instance = home.instance(s.selected_instance()?)?;
            let item = s.projection().catalog_item(&instance.catalog_id)?;
            Some((instance.clone(), item.clone()))
        })
    };
    let start = move |rotate: bool| {
        author.send(AuthoringIntent::BeginMove);
        if rotate {
            author.send(AuthoringIntent::RotateCandidate);
        }
        let bounds = author.state.with_untracked(|s| match s.draft() {
            Some(AuthoringDraft::Placement(draft)) => placement_bounds(s.projection(), draft),
            _ => None,
        });
        if let Some(bounds) = bounds {
            camera.update(|c| {
                c.reveal_bounds(
                    bounds,
                    if c.viewport.width <= 700. { 240. } else { 160. },
                    70.,
                )
            });
        }
        focus_later("scene-home".into());
    };
    view! {<div id="home-object-actions" class="home-owned-actions chrome" role="group" aria-label=move ||selected().map(|(_,i)|format!("Arrange {}",i.name)).unwrap_or("Arrange furniture".into()) style=move ||{let c=camera.get();let p=selected().and_then(|(instance,item)|instance.placement.map(|pose|owned_action_position(c,author.state.with(|s|s.selected_home().and_then(|id|s.projection().home(id)).map(|h|LotArtTransform::new(h.lot.bounds).footprint_center(item.footprint,pose))).unwrap_or_default()))).unwrap_or_default();format!("left:{}px;top:{}px",p.x,p.y)}>
    <button class="chrome primary" disabled=move ||author.busy() on:click=move |_|start(false)>"Move"</button><Show when=move ||selected().is_some_and(|(_,i)|i.can_rotate())><button class="chrome" disabled=move ||author.busy() on:click=move |_|start(true)>"Rotate"</button></Show><button class="chrome" disabled=move ||author.busy() on:click=move |_|author.send(AuthoringIntent::StoreSelected)>"Store"</button><button class="chrome" disabled=move ||author.busy() on:click=move |_|on_dismiss.run(())>"Done"</button>
    </div>}
}
