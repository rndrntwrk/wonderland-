//! Disposable facade exports; never a world save, command, or ownership claim.
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use wonderland_world_view::{WorldDocument, WorldFacadeJob};

#[wasm_bindgen(module = "/public/facade-links.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=makeFacadeLinks)]
    fn make_links(bytes: &[u8], metadata: &str, key: &str) -> Result<js_sys::Array, JsValue>;
    #[wasm_bindgen(js_name=releaseFacadeLinks)]
    fn release_links(file: &str, metadata: &str);
    #[wasm_bindgen(js_name=yieldFacade)]
    fn yield_facade() -> js_sys::Promise;
}
#[derive(Clone)]
struct Links {
    file: String,
    metadata: String,
    name: String,
}
impl Links {
    fn release(&self) {
        release_links(&self.file, &self.metadata);
    }
}
#[component]
pub fn WorldFacadePanel(world: Signal<Arc<WorldDocument>>) -> impl IntoView {
    let ready = RwSignal::new(None::<Links>);
    let pending = RwSignal::new(false);
    let notice = RwSignal::new(String::new());
    let generation = RwSignal::new(0_u64);
    let clear = move || {
        if let Some(links) = ready.try_get_untracked().flatten() {
            links.release();
        }
        ready.try_set(None);
    };
    // Track only the source, not the camera: this is a complete source facade.
    Effect::new(move |_| {
        let _ = world.get();
        generation.update(|v| *v = v.saturating_add(1));
        pending.set(false);
        clear();
        notice.set(String::new());
    });
    on_cleanup(move || {
        generation.try_update(|v| *v = v.saturating_add(1));
        clear();
    });
    let build = move |_| {
        if pending.get_untracked() {
            return;
        }
        let Some(serial) = generation
            .get_untracked()
            .checked_add(1)
            .filter(|v| *v < u64::MAX)
        else {
            notice.set("Reopen the lot to start a new export.".into());
            return;
        };
        generation.set(serial);
        clear();
        pending.set(true);
        notice.set("Building facade…".into());
        let document = world.get_untracked();
        spawn_local(async move {
            let current = || {
                generation.try_get_untracked() == Some(serial)
                    && world
                        .try_get_untracked()
                        .is_some_and(|w| Arc::ptr_eq(&w, &document))
            };
            let result = async {
                JsFuture::from(yield_facade())
                    .await
                    .map_err(|_| "Export scheduling failed.".to_string())?;
                if !current() {
                    return Ok(None);
                }
                let mut job = WorldFacadeJob::new(document.clone(), Default::default())
                    .map_err(|e| e.to_string())?;
                loop {
                    if !current() {
                        job.cancel();
                        return Ok(None);
                    }
                    if let Some(output) = job.step(4).map_err(|e| e.to_string())? {
                        let values =
                            make_links(&output.bytes, &output.metadata_json, &output.source_hash)
                                .map_err(|_| {
                                "The facade download could not be prepared.".to_string()
                            })?;
                        let file = values.get(0).as_string().ok_or("Missing facade URL.")?;
                        let metadata = values
                            .get(1)
                            .as_string()
                            .ok_or("Missing facade details URL.")?;
                        return Ok::<_, String>(Some(Links {
                            file,
                            metadata,
                            name: format!("wonderland-facade-{}", output.source_hash),
                        }));
                    }
                    JsFuture::from(yield_facade())
                        .await
                        .map_err(|_| "Export scheduling failed.".to_string())?;
                }
            }
            .await;
            if !current() {
                if let Ok(Some(links)) = result {
                    links.release();
                }
                return;
            }
            pending.try_set(false);
            match result {
                Ok(Some(links)) => {
                    ready.try_set(Some(links));
                    notice.try_set("Facade ready. This is a visual asset, not a game save.".into());
                }
                Ok(None) => {}
                Err(message) => {
                    notice.try_set(message);
                }
            }
        });
    };
    let cancel = move |_| {
        generation.update(|v| *v = v.saturating_add(1));
        pending.set(false);
        clear();
        notice.set("Export cancelled.".into());
    };
    view! {
        <section class="world-facade-export" aria-label="Lot facade export">
            <button class="chrome" disabled=move || pending.get() on:click=build>"Build facade"</button>
            <Show when=move || pending.get()><button class="chrome" on:click=cancel>"Cancel export"</button></Show>
            {move || ready.get().map(|links|view!{
                <div class="source-control-group">
                    <a class="chrome" href=links.file download=format!("{}.fsof",links.name)>"Save FSOf"</a>
                    <a class="chrome" href=links.metadata download=format!("{}.json",links.name)>"Facade details"</a>
                    <button class="chrome" on:click=move |_|{clear();notice.set(String::new());}>"Discard facade"</button>
                </div>
            })}
            <p role="status">{move || notice.get()}</p>
        </section>
    }
}
