//! Inspect an original or supplied world without assigning it to a saved home.
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use wonderland_world_view::{
    ViewportControls, WallMode, WorldDocument, WorldPick, WorldPickTarget,
};

use crate::{
    components::Icon,
    world_renderer::{WorldCaptureControls, WorldCapturePanel, WorldViewport},
};

#[component]
pub fn SourceWorldScreen(on_close: Callback<()>) -> impl IntoView {
    let original = match WorldDocument::original_empty_lot() {
        Ok(world) => Arc::new(world),
        Err(_) => return view! {
            <section class="source-world-screen"><div class="startup-card"><h1>"This lot couldn’t be opened"</h1>
                <button class="chrome" on:click=move |_| on_close.run(())>"Back"</button>
            </div></section>
        }.into_any(),
    };
    let world = RwSignal::new(original);
    let capture = WorldCaptureControls::default();
    let controls = RwSignal::new(ViewportControls::default());
    let selected = RwSignal::new(None::<WorldPick>);
    let title = RwSignal::new("Original lot".to_string());
    let notice = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let generation = RwSignal::new(0_u64);
    let on_pick = Callback::new(move |pick: WorldPick| {
        if world.with_untracked(|world| world.revision == pick.revision) {
            selected.set(Some(pick));
        }
    });
    let upload = move |event: web_sys::Event| {
        let input = event_target::<web_sys::HtmlInputElement>(&event);
        let file = input.files().and_then(|files| files.get(0));
        input.set_value("");
        let Some(file) = file else {
            return;
        };
        if file.size() > 32. * 1024. * 1024. {
            notice.set("This file is too large to open in the browser.".into());
            return;
        }
        let current = generation.get_untracked().wrapping_add(1);
        generation.set(current);
        loading.set(true);
        notice.set("Opening lot…".into());
        let filename = file.name();
        spawn_local(async move {
            let result = async {
                let value = JsFuture::from(file.text())
                    .await
                    .map_err(|_| "The lot file could not be read.".to_string())?;
                let text = value
                    .as_string()
                    .ok_or_else(|| "The lot file is not readable text.".to_string())?;
                let document = if text
                    .trim_start_matches('\u{feff}')
                    .trim_start()
                    .starts_with('<')
                {
                    WorldDocument::from_blueprint_xml(&text, &filename, "supplied-source")
                        .map_err(|_| "This blueprint is incomplete or unsupported.".to_string())?
                } else {
                    serde_json::from_str::<WorldDocument>(&text).map_err(|_| {
                        "Choose a source blueprint XML or a Wonderland world document.".to_string()
                    })?
                };
                document.validate().map_err(|_| {
                    "This lot contains invalid or unsupported world data.".to_string()
                })?;
                Ok::<_, String>(Arc::new(document))
            }
            .await;
            if generation.try_get_untracked() != Some(current) {
                return;
            }
            loading.try_set(false);
            match result {
                Ok(document) => {
                    selected.try_set(None);
                    controls.try_set(ViewportControls::default());
                    world.try_set(document);
                    title.try_set(filename);
                    notice.try_set("Lot opened. Drag to look around and select a tile or object to inspect it.".into());
                }
                Err(message) => {
                    notice.try_set(message);
                }
            }
        });
    };
    view! {
        <section class="source-world-screen" aria-label="Source lot view">
            <WorldViewport world=Signal::derive(move || world.get()) controls on_pick capture/>
            <header class="source-world-header chrome">
                <button class="chrome round small" aria-label="Back to your Sims" on:click=move |_| on_close.run(())><Icon name="chevron-left"/></button>
                <div><span class="eyebrow">"SOURCE LOT"</span><h1>{move || title.get()}</h1>
                    <p>{move || world.with(|world| format!("{} × {} tiles · {} floors", world.lot.width, world.lot.height, world.lot.levels))}</p>
                </div>
                <label class="chrome source-open-lot" aria-disabled=move || loading.get().to_string()>
                    <Icon name="home"/>"Open lot"
                    <input type="file" accept=".xml,.json,application/json,text/xml,application/xml" disabled=move || loading.get() on:change=upload/>
                </label>
            </header>
            <nav class="source-world-tools chrome" aria-label="World view controls">
                <div class="source-control-group" role="group" aria-label="Camera">
                    <button class="chrome round small" aria-label="Rotate left" on:click=move |_| controls.update(|view| view.yaw_radians -= std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise" class="icon-mirror"/></button>
                    <button class="chrome round small" aria-label="Rotate right" on:click=move |_| controls.update(|view| view.yaw_radians += std::f32::consts::FRAC_PI_4)><Icon name="rotate-clockwise"/></button>
                    <button class="chrome round small" aria-label="Zoom out" on:click=move |_| controls.update(|view| view.zoom = (view.zoom / 1.2).max(0.25))><Icon name="minus"/></button>
                    <button class="chrome round small" aria-label="Zoom in" on:click=move |_| controls.update(|view| view.zoom = (view.zoom * 1.2).min(12.))><Icon name="plus"/></button>
                    <button class="chrome" on:click=move |_| controls.set(ViewportControls::default())>"Reset view"</button>
                </div>
                <div class="source-control-group" role="group" aria-label="Visible floor">
                    <button class="chrome round small" aria-label="Floor down" disabled={move || controls.get().visible_level <= 1} on:click=move |_| controls.update(|view| view.visible_level = view.visible_level.saturating_sub(1).max(1))><Icon name="chevron-down"/></button>
                    <span>{move || format!("Floor {}", controls.get().visible_level)}</span>
                    <button class="chrome round small" aria-label="Floor up" disabled={move || controls.get().visible_level >= world.with(|world| world.lot.levels)} on:click=move |_| { let max = world.with_untracked(|world| world.lot.levels); controls.update(|view| view.visible_level = (view.visible_level + 1).min(max)); }><Icon name="chevron-up"/></button>
                </div>
                <div class="source-control-group" role="group" aria-label="Wall visibility">
                    {[(WallMode::Up,"Walls up"),(WallMode::Cutaway,"Cutaway"),(WallMode::Down,"Walls down")].into_iter().map(move |(mode,label)| view! {
                        <button class="chrome" aria-pressed=move || (controls.get().walls == mode).to_string() on:click=move |_| controls.update(|view| view.walls = mode)>{label}</button>
                    }).collect_view()}
                    <button class="chrome" aria-pressed=move || controls.get().show_roofs.to_string() on:click=move |_| controls.update(|view| view.show_roofs = !view.show_roofs)>"Roof"</button>
                </div>
            </nav>
            <aside class="source-world-inspector chrome" aria-label="Selected world item">
                <Show when=move || selected.get().is_some() fallback=move || view! {<strong>"Explore the lot"</strong><p>"Drag to look around. Select a tile or object to inspect it."</p>}>
                    <strong>{move || selected.with(|pick| pick.as_ref().map(|pick| match &pick.target {
                        WorldPickTarget::Tile {x,y,level,..} => format!("Tile {x}, {y} · Floor {level}"),
                        WorldPickTarget::Object {source_guid,..} => format!("Object {source_guid:08X}"),
                    }).unwrap_or_default())}</strong>
                    <p>{move || selected.with(|pick| pick.as_ref().map(|pick| match &pick.target {
                        WorldPickTarget::Tile {surface,..} => format!("{surface:?}"),
                        WorldPickTarget::Object {source_record,..} => source_record.map(|record| format!("Original object record {record}")).unwrap_or("Source object".into()),
                    }).unwrap_or_default())}</p>
                    <button class="chrome" on:click=move |_| selected.set(None)>"Clear selection"</button>
                </Show>
                <WorldCapturePanel capture/>
                <p class="source-world-mode">"Local source view"</p>
            </aside>
            <Show when=move || !notice.get().is_empty()><p class="source-world-notice chrome" role="status">{move || notice.get()}</p></Show>
        </section>
    }.into_any()
}
