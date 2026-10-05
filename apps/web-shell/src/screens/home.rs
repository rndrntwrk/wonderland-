use crate::{
    authoring_bridge::AuthorUi,
    authoring_geometry::*,
    bridge::{Ui, focus_later, selected_character},
    components::{
        Icon,
        authoring::{owned_actions::OwnedActions, sprite::Furniture},
    },
    geometry::{Point, Size},
};
use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use wasm_bindgen::JsCast;
use wonderland_contracts::authoring::*;
use wonderland_contracts::{CharacterId, UiIntent};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Live,
    Build,
    Buy,
    Inventory,
}
fn size(mode: Mode, placing: bool) -> Size {
    let w = web_sys::window().unwrap();
    let width = w
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1440.);
    let height = w
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(900.);
    Size {
        width,
        height: (height
            - if placing {
                if width <= 700. { 190. } else { 160. }
            } else if matches!(mode, Mode::Live | Mode::Build) {
                if width <= 700. { 110. } else { 125. }
            } else if width <= 700. {
                260.
            } else {
                225.
            })
        .max(240.),
    }
}
fn reveal_home_focus(camera: &mut HomeCamera, author: AuthorUi) {
    let bounds = author.state.with_untracked(|s| {
        if let Some(AuthoringDraft::Placement(draft)) = s.draft() {
            return placement_bounds(s.projection(), draft);
        }
        let home = s.projection().home(s.selected_home()?)?;
        let instance = home.instance(s.selected_instance()?)?;
        let item = s.projection().catalog_item(&instance.catalog_id)?;
        Some(furniture_bounds(
            item.id.as_ref(),
            item.footprint,
            instance.placement?,
        ))
    });
    if let Some(bounds) = bounds {
        camera.reveal_bounds(
            bounds,
            if camera.viewport.width <= 700. {
                240.
            } else {
                160.
            },
            70.,
        );
    }
}
#[component]
pub fn HomeScreen() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let author = expect_context::<AuthorUi>();
    let owner = ui
        .state
        .with_untracked(|s| s.selected_character.clone())
        .unwrap_or_else(|| CharacterId::from("maya"));
    author.select(owner.clone());
    author.send(AuthoringIntent::OpenHome(owner));
    let mode = RwSignal::new(Mode::Live);
    let camera = RwSignal::new(HomeCamera::new(size(
        mode.get_untracked(),
        author
            .state
            .with_untracked(|s| matches!(s.draft(), Some(AuthoringDraft::Placement(_)))),
    )));
    camera.update(|c| {
        if c.viewport.width <= 700. {
            c.zoom =
                (c.viewport.height / (crate::geometry::WORLD_HEIGHT * c.scale())).clamp(1., 4.);
        }
    });
    let usable = Memo::new(move |_| {
        size(
            mode.get(),
            author
                .state
                .with(|s| matches!(s.draft(), Some(AuthoringDraft::Placement(_)))),
        )
    });
    Effect::new(move |_| {
        let viewport = usable.get();
        camera.update(|c| {
            c.resize(viewport);
            reveal_home_focus(c, author);
        });
    });
    let resize = window_event_listener(ev::resize, move |_| {
        camera.update(|c| {
            c.resize(size(
                mode.get_untracked(),
                author
                    .state
                    .with_untracked(|s| matches!(s.draft(), Some(AuthoringDraft::Placement(_)))),
            ));
            reveal_home_focus(c, author);
        })
    });
    on_cleanup(move || resize.remove());
    let category = RwSignal::new(None::<CatalogCategory>);
    let search = RwSignal::new(String::new());
    let drag = RwSignal::new(None::<Point>);
    let moved = RwSignal::new(false);
    let placement = move || {
        author.state.with(|s| match s.draft() {
            Some(AuthoringDraft::Placement(d)) => Some(d.clone()),
            _ => None,
        })
    };
    let reveal = move || camera.update(|c| reveal_home_focus(c, author));
    let cancel = move || {
        if author.busy() {
            return;
        }
        let target = author
            .state
            .with_untracked(|s| s.selected_instance().map(|id| format!("home-object-{id}")));
        author.send(AuthoringIntent::Cancel);
        focus_later(target.unwrap_or("scene-home".into()));
    };
    let dismiss_owned = move || {
        if author.busy() {
            return;
        }
        let (owner, target) = author.state.with_untracked(|s| {
            (
                s.selected_home().cloned(),
                s.selected_instance().map(|id| format!("home-object-{id}")),
            )
        });
        if let Some(owner) = owner {
            author.send(AuthoringIntent::OpenHome(owner));
        }
        mode.set(Mode::Live);
        camera.update(|c| c.resize(size(Mode::Live, false)));
        focus_later(target.unwrap_or("scene-home".into()));
    };
    let change_mode = move |new: Mode| {
        if author.busy() {
            return;
        }
        author.send(AuthoringIntent::Cancel);
        mode.set(new);
        camera.update(|c| c.resize(size(new, false)));
    };
    view! {
    <section class="scene-screen home-screen" aria-label="Home authoring" aria-busy=move ||author.busy().to_string() on:keydown=move |e:web_sys::KeyboardEvent|{
    let typing=e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()).is_some_and(|el|matches!(el.tag_name().as_str(),"INPUT"|"TEXTAREA"|"SELECT")||el.get_attribute("contenteditable").is_some());if typing||e.is_composing(){return;}
    if e.key()=="Enter" && e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()).and_then(|el|el.closest("button").ok().flatten()).is_some(){return;}
    if author.busy(){return;}
    if placement().is_some(){let intent=match e.key().as_str(){"ArrowLeft"=>Some(AuthoringIntent::MoveCandidate{dx:-1,dy:0}),"ArrowRight"=>Some(AuthoringIntent::MoveCandidate{dx:1,dy:0}),"ArrowUp"=>Some(AuthoringIntent::MoveCandidate{dx:0,dy:-1}),"ArrowDown"=>Some(AuthoringIntent::MoveCandidate{dx:0,dy:1}),"r"|"R"=>Some(AuthoringIntent::RotateCandidate),"Enter"=>Some(AuthoringIntent::ConfirmPlacement),"Escape"=>Some(AuthoringIntent::Cancel),_=>None};if let Some(intent)=intent{e.prevent_default();if matches!(intent,AuthoringIntent::Cancel){cancel();}else{author.send(intent);reveal();}}}else if e.key()=="Escape"{e.prevent_default();dismiss_owned();}else {let direct=e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()).is_some_and(|el|el.id()=="scene-home");if direct {let delta=match e.key().as_str(){"ArrowLeft"=>Some(Point{x:50.,y:0.}),"ArrowRight"=>Some(Point{x:-50.,y:0.}),"ArrowUp"=>Some(Point{x:0.,y:50.}),"ArrowDown"=>Some(Point{x:0.,y:-50.}),_=>None};if let Some(delta)=delta{e.prevent_default();camera.update(|c|c.pan_by(delta));}else if matches!(e.key().as_str(),"+"|"="|"-"|"Home"){e.prevent_default();camera.update(|c|if e.key()=="Home"{c.reset()}else{c.zoom_at(if e.key()=="-"{1./1.2}else{1.2},Point{x:c.viewport.width/2.,y:c.viewport.height/2.})});}}}
    }>
    <div id="scene-home" class="home-viewport" tabindex="0" role="group" aria-label="Home scene. Drag to pan. Arrow keys move a selected placement, R rotates, Enter saves, Escape cancels." style=move ||format!("height:{}px",camera.get().viewport.height)
    on:pointerdown=move |e:web_sys::PointerEvent|{if !e.is_primary()||e.button()!=0{return;}
    if e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()).and_then(|e|e.closest("button").ok().flatten()).is_some(){return;}moved.set(false);drag.set(Some(Point{x:f64::from(e.client_x()),y:f64::from(e.client_y())}));if let Some(el)=e.current_target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()){let _=el.set_pointer_capture(e.pointer_id());}}
    on:pointermove=move |e:web_sys::PointerEvent|{if let Some(prev)=drag.get_untracked(){let p=Point{x:f64::from(e.client_x()),y:f64::from(e.client_y())};let delta=Point{x:p.x-prev.x,y:p.y-prev.y};if delta.x.abs()+delta.y.abs()>2.{moved.set(true);}camera.update(|c|c.pan_by(delta));drag.set(Some(p));}}
    on:pointerup=move |_|drag.set(None) on:pointercancel=move |_|drag.set(None)
    on:click=move |e:web_sys::MouseEvent|{if moved.get_untracked()||author.busy(){return;}let is_button=e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()).and_then(|e|e.closest("button").ok().flatten()).is_some();if !is_button&&placement().is_some()&&let Some(cell)=pick(camera.get_untracked(),Point{x:f64::from(e.client_x()),y:f64::from(e.client_y())}){author.send(AuthoringIntent::SetCell(cell));reveal();}}
    on:wheel=move |e:web_sys::WheelEvent|{e.prevent_default();camera.update(|c|c.zoom_at(if e.delta_y()<0.{1.12}else{1./1.12},Point{x:f64::from(e.client_x()),y:f64::from(e.client_y())}));}>
    <div class="home-plane" style=move ||{let c=camera.get();let o=c.origin();format!("transform:translate({}px,{}px) scale({});--camera-scale:{}",o.x,o.y,c.scale(),c.scale())}>
    <img class="world-art" src="/assets/authoring/home-scene.png" alt="Empty coastal Home with oak floor and windows" draggable="false"/>
    <img class="home-character" src=move ||selected_character(ui).map(|c|author.path(&c.id)).unwrap_or_default() alt=move ||selected_character(ui).map(|c|format!("{} at the Home entrance",c.name)).unwrap_or_default() draggable="false" style={let p=cell_center(GridCell{x:0,y:0});format!("left:{}px;top:{}px",p.x,p.y)}/>
    <div class="entrance-marker" style={let p=cell_center(GridCell{x:0,y:0});format!("left:{}px;top:{}px",p.x,p.y)}>"Entrance"</div>
    <For each=move ||author.state.with(|s|s.selected_home().and_then(|id|s.projection().home(id)).map(|h|h.instances.iter().filter(|i|i.placement.is_some()).cloned().collect::<Vec<_>>()).unwrap_or_default()) key=|i|(i.id.clone(),format!("{:?}",i.placement)) children=move |instance|{
    let pose=instance.placement.unwrap();let id=instance.id.clone();let item=author.state.with_untracked(|s|s.projection().catalog_item(&instance.catalog_id).cloned()).unwrap();let p=footprint_center(item.footprint,pose);let selected=id.clone();let focus=furniture_bounds(item.id.as_ref(),item.footprint,pose);
    view!{<Furniture item=item.clone() pose=pose/><button id=format!("home-object-{id}") class="home-object-pick" data-instance-id=id.to_string() data-catalog-id=instance.catalog_id.to_string() data-cell-x=pose.cell.x data-cell-y=pose.cell.y data-direction=format!("{:?}",pose.direction) aria-label=format!("Select {} at cell {}, {}",item.name,pose.cell.x,pose.cell.y) aria-pressed=move ||author.state.with(|s|s.selected_instance()==Some(&selected)).to_string() disabled=move ||author.busy()||placement().is_some() style=format!("left:{}px;top:{}px;z-index:{}",p.x,p.y,p.y as i32+20) on:focus=move |_|camera.update(|c|c.reveal_bounds(focus,if c.viewport.width<=700.{240.}else{160.},70.)) on:click=move |_|{author.send(AuthoringIntent::SelectOwned(id.clone()));mode.set(Mode::Build);camera.update(|c|c.resize(size(Mode::Build, false)));} ><Icon name="cube"/><span>{item.name.clone()}</span></button>}
    }/>
    <Show when=move ||placement().is_some()>
    <div class="floor-cells">{(0..6).flat_map(|y|(0..8).map(move |x|GridCell{x,y})).map(move |cell|{let p=cell_center(cell);view!{<button class="floor-cell" aria-label=format!("Cell {}, {}{}",cell.x,cell.y,if cell.x==0&&cell.y==0{". Entrance reserved"}else{""}) disabled=move ||author.busy() aria-pressed=move ||placement().is_some_and(|d|d.pose.cell==cell).to_string() style=format!("left:{}px;top:{}px",p.x,p.y) on:focus=move |_|camera.update(|c|c.reveal(p,100.,70.)) on:click=move |_|{author.send(AuthoringIntent::SetCell(cell));reveal();}></button>}}).collect_view()}</div>
    {move ||placement().and_then(|d|author.state.with(|s|{let id=match &d.source{PlacementSource::Catalog(id)=>Some(id.clone()),PlacementSource::Move(id)|PlacementSource::Inventory(id)=>s.projection().home(&d.home_owner_id).and_then(|h|h.instance(id)).map(|i|i.catalog_id.clone())}?;let item=s.projection().catalog_item(&id)?.clone();let points=footprint_points(item.footprint,d.pose);let valid=s.draft_validity().is_ok();Some(view!{<svg class="footprint-overlay" viewBox="0 0 1672 941"><polygon class:invalid=!valid points={points.iter().map(|p|format!("{},{}",p.x,p.y)).collect::<Vec<_>>().join(" ")}/></svg><Furniture item=item pose=d.pose ghost=true/>})}))}
    </Show>
    </div></div>
    <header class="scene-header chrome home-header"><button class="chrome round" aria-label="Back to city" disabled=move ||author.busy() on:click=move |_|{author.send(AuthoringIntent::Close);ui.send(UiIntent::Back);} ><Icon name="chevron-left"/></button><Icon name="home"/><h1 id="screen-title" tabindex="-1">{move ||selected_character(ui).map(|c|format!("{}’s Home",c.name)).unwrap_or("Home".into())}</h1></header>
    <aside class="home-budget chrome"><img src=move ||selected_character(ui).map(|c|author.path(&c.id)).unwrap_or_default() alt=""/><span><strong>{move ||selected_character(ui).map(|c|c.name).unwrap_or_default()}</strong><span>{move ||selected_character(ui).map(|c|format!("${} · preview budget",c.money)).unwrap_or_default()}</span></span></aside>
    <nav class="camera-controls chrome" aria-label="Home camera"><button class="chrome round" aria-label="Zoom out" on:click=move |_|camera.update(|c|c.zoom_at(1./1.2,Point{x:c.viewport.width/2.,y:c.viewport.height/2.}))><Icon name="minus"/></button><button class="chrome round" aria-label="Zoom in" on:click=move |_|camera.update(|c|c.zoom_at(1.2,Point{x:c.viewport.width/2.,y:c.viewport.height/2.}))><Icon name="plus"/></button><button class="chrome round" aria-label="Reset Home view" on:click=move |_|camera.update(|c|c.reset())><Icon name="rotate-clockwise"/></button></nav>
    <Show when=move ||author.state.with(|s|s.selected_instance().is_some())&&placement().is_none()><OwnedActions camera=camera on_dismiss=Callback::new(move |_|dismiss_owned())/></Show>
    <div class="home-drawer chrome" class:placing=move ||placement().is_some() aria-label="Room editing">
    <Show when=move ||placement().is_some() fallback=move ||view!{
    <Show when=move ||matches!(mode.get(),Mode::Buy|Mode::Inventory) fallback=move ||view!{
    <Show when=move ||author.state.with(|s|s.selected_instance().is_none())><p class="room-hint">{move ||if mode.get()==Mode::Build{author.state.with(|s|s.selected_home().and_then(|id|s.projection().home(id)).and_then(|home|crate::components::availability_reason(&home.permissions.arrange)).unwrap_or_else(||"Select furniture to move or store. Room arrangement preview.".into()))}else{"Your coastal room. Choose Buy to furnish it.".into()}}</p></Show>
    }>
    <div class="catalog-filters"><button class="chrome" aria-pressed=move ||category.get().is_none().to_string() on:click=move |_|category.set(None)>"All"</button>{CatalogCategory::ALL.into_iter().map(move |c|view!{<button class="chrome" aria-pressed=move ||(category.get()==Some(c)).to_string() on:click=move |_|category.set(Some(c))>{c.label()}</button>}).collect_view()}<input type="search" aria-label="Search furniture" placeholder="Find furniture" prop:value=move ||search.get() on:input=move |e|search.set(event_target_value(&e))/></div>
    <div class="catalog-row" role="group" aria-label="Furniture catalog">
    {move ||author.state.with(|s|{let inventory=mode.get()==Mode::Inventory;let home=s.selected_home().and_then(|id|s.projection().home(id));let rows=s.projection().catalog.iter().filter(|item|category.get().is_none_or(|c|item.category==c)&&item.name.to_lowercase().contains(&search.get().to_lowercase())).flat_map(|item|if inventory{home.map(|h|h.instances.iter().filter(|i|i.catalog_id==item.id&&i.placement.is_none()).map(|i|(item.clone(),Some(i.id.clone()))).collect::<Vec<_>>()).unwrap_or_default()}else{vec![(item.clone(),None)]}).collect::<Vec<_>>();if rows.is_empty(){view!{<p class="empty-catalog">{if inventory{"No stored furniture. Choose Buy to browse the catalog."}else if s.projection().catalog.is_empty(){"No furniture is available in this preview."}else{"No furniture matches. Try All or clear search."}}</p>}.into_any()}else{rows.into_iter().map(|(item,instance)|{let id=item.id.clone();let name=item.name.clone();let sprite=sprite_layout(id.as_ref(),Direction::North);view!{<button class="catalog-card" data-catalog-id=id.to_string() data-instance-id=instance.as_ref().map(ToString::to_string) disabled=move ||author.busy() aria-label=format!("{} {}",if instance.is_some(){"Place stored"}else{"Buy"},name) on:click=move |_|{author.send(if let Some(id)=instance.clone(){AuthoringIntent::BeginPlace(id)}else{AuthoringIntent::SelectCatalog(id.clone())});reveal();focus_later("scene-home".into());}><img src=sprite.path alt=""/><strong>{item.name}</strong><span>{if instance.is_some(){"Owned".into()}else{format!("${}",item.price)}}</span></button>}}).collect_view().into_any()}})}
    </div></Show>
    }>
    <div class="placement-controls"><div><strong>{move ||placement().and_then(|d|author.state.with(|s|{let id=match &d.source{PlacementSource::Catalog(id)=>Some(id.clone()),PlacementSource::Move(id)|PlacementSource::Inventory(id)=>s.projection().home(&d.home_owner_id).and_then(|h|h.instance(id)).map(|i|i.catalog_id.clone())}?;s.projection().catalog_item(&id).map(|i|if matches!(d.source,PlacementSource::Catalog(_)){format!("{} · ${}",i.name,i.price)}else{format!("{} · owned",i.name)})})).unwrap_or_default()}</strong><p id="placement-validity" role="status">{move ||author.state.with(|s|s.draft_validity().err().map(|e|e.to_string()).unwrap_or("Ready to place · arrows move · R rotates".into()))}</p></div>
    <div class="placement-buttons"><button class="chrome" disabled=move ||author.busy() on:click=move |_|cancel()>"Cancel"</button><Show when=move ||placement().is_some_and(|d|author.state.with(|s|{let id=match &d.source{PlacementSource::Catalog(id)=>Some(id.clone()),PlacementSource::Move(id)|PlacementSource::Inventory(id)=>s.projection().home(&d.home_owner_id).and_then(|h|h.instance(id)).map(|i|i.catalog_id.clone())};id.and_then(|id|s.projection().catalog_item(&id)).is_some_and(|i|i.can_rotate())}))><button class="chrome" disabled=move ||author.busy() on:click=move |_|{author.send(AuthoringIntent::RotateCandidate);reveal();}>"Rotate"</button></Show><button id="confirm-placement" class="chrome primary" aria-describedby="placement-validity" disabled=move ||author.busy()||author.state.with(|s|s.draft_validity().is_err()) on:click=move |_|author.send(AuthoringIntent::ConfirmPlacement)>{move ||if author.busy(){"Saving…"}else if placement().is_some_and(|d|matches!(d.source,PlacementSource::Catalog(_))){"Buy and place"}else{"Place"}}</button></div></div>
    </Show>
    <nav class="home-modes" aria-label="Game mode">{[(Mode::Live,"Live","home"),(Mode::Build,"Build","hammer"),(Mode::Buy,"Buy","shopping-cart"),(Mode::Inventory,"Inventory","stack-2")].into_iter().map(move |(m,label,icon)|view!{<button id=if m==Mode::Inventory{"home-inventory"}else{""} class="chrome" class:primary=move ||mode.get()==m aria-pressed=move ||(mode.get()==m).to_string() disabled=move ||author.busy() on:click=move |_|change_mode(m)><Icon name=icon/>{label}</button>}).collect_view()}</nav>
    </div></section>
    }
}
