// SPDX-License-Identifier: MPL-2.0
use crate::session::{EditorSession, Inspection, MAX_FILE_BYTES, PAGE_SIZE};
use crate::workbench::WorkbenchMode;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use wonderland_creator::{Edit, EditGuard, ResourceTransaction};
use wonderland_legacy_formats::iff::ChunkKey;

mod components;
mod forms;
mod workbench;
use components::{Details, History, Inspector, ResourceIcon};
use forms::{AddResource, EditPanel};
use workbench::WorkbenchPanel;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Inspect,
    Edit,
    History,
}
impl Tab {
    fn id(self) -> &'static str {
        match self {
            Self::Inspect => "tab-inspect",
            Self::Edit => "tab-edit",
            Self::History => "tab-history",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Ui {
    pub session: StoredValue<EditorSession>,
    pub revision: RwSignal<u64>,
    pub tab: RwSignal<Tab>,
    pub status: RwSignal<String>,
    pub error: RwSignal<Option<String>>,
    pub reading: RwSignal<bool>,
}
impl Ui {
    pub fn inspect(self) -> Option<Inspection> {
        self.revision.get();
        self.session.with_value(|s| s.inspect().ok())
    }
    pub fn report(self, result: Result<bool, String>, action: &str) {
        match result {
            Ok(changed) => {
                self.error.set(None);
                self.status.set(if changed {
                    action.to_owned()
                } else {
                    "No changes to apply.".into()
                });
                self.revision.update(|n| *n += 1);
                focus_after_render(self.tab.get_untracked().id());
            }
            Err(error) => {
                self.error.set(Some(error));
            }
        }
    }
    pub fn apply(self, key: ChunkKey, guard: &EditGuard, edit: Edit, label: &str) {
        let mut result = Err("document unavailable".into());
        self.session
            .update_value(|s| result = s.apply(key, guard, edit, label));
        self.report(result, label);
    }
    pub fn transact(self, transaction: &ResourceTransaction) {
        let mut result = Err("document unavailable".into());
        self.session
            .update_value(|s| result = s.transact(transaction, "Change resource metadata"));
        self.report(result, "Resource metadata updated.");
    }
    pub fn export(self) {
        let result = self.session.with_value(|s| {
            s.export()
                .and_then(|bytes| download(&s.export_name(), &bytes, "application/octet-stream"))
        });
        match result {
            Ok(()) => {
                self.status.set("Exported the current document.".into());
                self.error.set(None);
            }
            Err(error) => self.error.set(Some(error)),
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let ui = Ui {
        session: StoredValue::new(EditorSession::default()),
        revision: RwSignal::new(0),
        tab: RwSignal::new(Tab::Inspect),
        status: RwSignal::new("Open an IFF file to inspect and edit its resources.".into()),
        error: RwSignal::new(None),
        reading: RwSignal::new(false),
    };
    provide_context(ui);
    let mode = RwSignal::new(None::<WorkbenchMode>);
    let filter = RwSignal::new(String::new());
    let page = RwSignal::new(0usize);
    let file_input = NodeRef::<leptos::html::Input>::new();
    let opened = move || {
        ui.revision.get();
        ui.session.with_value(|s| s.loaded())
    };
    let rows = move || {
        ui.revision.get();
        ui.session.with_value(|s| {
            s.rows(&filter.get(), page.get() * PAGE_SIZE, PAGE_SIZE + 1)
                .unwrap_or_default()
        })
    };
    let dirty = move || {
        ui.revision.get();
        ui.session.with_value(|s| s.is_dirty())
    };
    view! {
        <a class="skip-link" href=move||mode.get().map(|m|format!("#workbench-{}",m.id())).unwrap_or("#resource-editor".into())>"Skip to editor"</a>
        <div class="creator-shell" data-dirty=move||dirty().to_string()>
            <header class="topbar">
                <a class="brand" href="#resource-editor" on:click=move|_|mode.set(None)><ResourceIcon/>"Wonderland Creator"</a>
                <div class="filename" aria-label="Current file">{move||{ui.revision.get();if let Some(m)=mode.get(){m.title().into()}else{ui.session.with_value(|s|if s.loaded(){format!("{}{}",s.filename(),if s.is_dirty(){" •"}else{""})}else{"Open a resource file".into()})}}}</div>
                <div class="file-actions" hidden=move||mode.get().is_some()>
                    <input class="visually-hidden" type="file" tabindex="-1" aria-hidden="true" accept=".iff,.otf,.spf,.flr,.wll,application/octet-stream" aria-label="Choose an IFF resource file" node_ref=file_input
                        on:change=move|event|{
                            let input=event_target::<web_sys::HtmlInputElement>(&event);
                            let file=input.files().and_then(|files|files.get(0)); input.set_value("");
                            if let Some(file)=file {open_file(ui,file);page.set(0);filter.set(String::new());}
                        }/>
                    <button class="button secondary" disabled=move||ui.reading.get() on:click=move|_|{if let Some(input)=file_input.get(){input.click();}}> {move||if ui.reading.get(){"Reading IFF…"}else{"Open IFF"}} </button>
                    <button class="button primary" disabled=move||!opened()||ui.reading.get() on:click=move|_|ui.export()>"Export IFF"</button>
                </div>
            </header>
            <nav class="workbench-nav" aria-label="Creator tools">
                {[(None,"IFF resources"),(Some(WorkbenchMode::Upgrades),"Upgrades"),(Some(WorkbenchMode::City),"City painter"),(Some(WorkbenchMode::Neighborhood),"Neighborhoods"),(Some(WorkbenchMode::Assets),"Assets"),(Some(WorkbenchMode::Patches),"Patches")].into_iter().map(|(target,label)|view!{
                    <button type="button" class:active=move||mode.get()==target aria-pressed=move||(mode.get()==target).to_string()
                        on:click=move|_|{mode.set(target);focus_after_render(match target{None=>"resource-editor",Some(WorkbenchMode::Upgrades)=>"workbench-upgrades",Some(WorkbenchMode::City)=>"workbench-city",Some(WorkbenchMode::Neighborhood)=>"workbench-neighborhood",Some(WorkbenchMode::Assets)=>"workbench-assets",Some(WorkbenchMode::Patches)=>"workbench-patches"});}>{label}</button>
                }).collect_view()}
            </nav>
            <div class="workspace" hidden=move||mode.get().is_some()>
                <aside class="resource-rail" aria-label="Resource browser">
                    <h2>"Resources"</h2>
                    <label class="visually-hidden" for="resource-filter">"Filter resources"</label>
                    <input id="resource-filter" class="resource-filter" placeholder="Filter resources" type="search" maxlength="256" prop:value=move||filter.get() on:input=move|ev|{filter.set(event_target_value(&ev));page.set(0);}/>
                    <nav class="resource-list" aria-label="IFF resources">
                        {move||rows().into_iter().take(PAGE_SIZE).map(|row|{
                            let key=row.key;
                            view! {<button class="resource-row" class:selected=move||{ui.revision.get();ui.session.with_value(|s|s.selected()==Some(key))}
                                aria-current=move||{ui.revision.get();ui.session.with_value(|s|if s.selected()==Some(key){"true"}else{"false"})}
                                on:click=move|_|{ui.session.update_value(|s|{let _=s.select(key);});ui.tab.set(Tab::Inspect);ui.error.set(None);ui.revision.update(|n|*n+=1);focus_after_render("resource-editor");}>
                                <ResourceIcon/><span><strong>{format!("{} {}",row.kind,key.id)}</strong><span>{if row.label.is_empty(){"Untitled resource".into()}else{row.label}}</span></span>
                            </button>}
                        }).collect_view()}
                        <Show when=move||rows().is_empty()><p class="rail-empty">{move||if opened(){"No matching resources."}else{"Your imported resources will appear here."}}</p></Show>
                    </nav>
                    <div class="pagination">
                        <button disabled=move||page.get()==0 on:click=move|_|page.update(|n|*n=n.saturating_sub(1))>"Previous"</button>
                        <span>{move||format!("Page {}",page.get()+1)}</span>
                        <button disabled=move||rows().len()<=PAGE_SIZE on:click=move|_|page.update(|n|*n+=1)>"Next"</button>
                    </div>
                    <p class="rail-count">{move||{ui.revision.get();ui.session.with_value(|s|format!("{} resources",s.resource_count()))}}</p>
                </aside>
                <main id="resource-editor" class="editor" tabindex="-1" aria-busy=move||ui.reading.get().to_string()>
                    <Show when=move||ui.error.get().is_some()><div class="error-banner" role="alert"><strong>"Could not apply that action"</strong><p>{move||ui.error.get().unwrap_or_default()}</p><button on:click=move|_|ui.error.set(None)>"Dismiss"</button></div></Show>
                    {move||match ui.inspect(){
                        Some(inspection)=>{
                            let title=if inspection.row.label.is_empty(){"Untitled resource".into()}else{inspection.row.label.clone()};
                            view!{
                                <div class="resource-heading"><span>{format!("{} {}",inspection.row.kind,inspection.row.key.id)}</span><h1>{title}</h1></div>
                                <div class="tabs" role="tablist" aria-label="Resource tools">
                                    <TabButton tab=Tab::Inspect label="Inspect"/>
                                    <TabButton tab=Tab::Edit label="Edit"/>
                                    <TabButton tab=Tab::History label="History"/>
                                </div>
                                <section id="current-resource-panel" class="tab-panel" role="tabpanel" aria-labelledby=move||ui.tab.get().id()>
                                    <fieldset class="editor-controls" disabled=move||ui.reading.get()>
                                    {move||match ui.tab.get(){
                                        Tab::Inspect=>view!{<Inspector inspection=inspection.clone()/>}.into_any(),
                                        Tab::Edit=>view!{<EditPanel inspection=inspection.clone()/>}.into_any(),
                                        Tab::History=>view!{<History/>}.into_any(),
                                    }}
                                    </fieldset>
                                </section>
                            }.into_any()
                        },
                        None=>if opened(){
                            view!{<section class="empty-state"><ResourceIcon/><h1>"Resource file is empty"</h1><p>"Add a resource to begin editing this IFF."</p><fieldset class="editor-controls" disabled=move||ui.reading.get()><AddResource/><Show when=move||{ui.revision.get();ui.session.with_value(|s|s.undo_len()+s.redo_len()>0)}><History/></Show></fieldset></section>}.into_any()
                        }else{
                            view!{<section class="empty-state"><ResourceIcon/><h1>"Open your workspace"</h1><p>"Inspect IFF resources, edit their fields, and export your changes."</p><button class="button primary" disabled=move||ui.reading.get() on:click=move|_|{if let Some(input)=file_input.get(){input.click();}}>"Open IFF"</button><p class="quiet">"Files stay in this browser. Maximum import size: 8 MiB."</p></section>}.into_any()
                        }
                    }}
                </main>
                <aside class="details-pane" aria-label="Selected resource details">{move||ui.inspect().map(|inspection|view!{<Details inspection/>})}</aside>
            </div>
            {[WorkbenchMode::Upgrades,WorkbenchMode::City,WorkbenchMode::Neighborhood,WorkbenchMode::Assets,WorkbenchMode::Patches].into_iter().map(|target|view!{
                <div class="workbench-host" hidden=move||mode.get()!=Some(target)><WorkbenchPanel mode=target/></div>
            }).collect_view()}
            <footer class="statusbar" hidden=move||mode.get().is_some()><span class="local-label"><ResourceIcon/>"Local workspace"</span><span role="status" aria-live="polite">{move||ui.status.get()}</span></footer>
        </div>
    }
}

#[component]
fn TabButton(tab: Tab, label: &'static str) -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {<button id=tab.id() type="button" role="tab" class:active=move||ui.tab.get()==tab aria-selected=move||(ui.tab.get()==tab).to_string()
    tabindex=move||if ui.tab.get()==tab{"0"}else{"-1"} aria-controls="current-resource-panel"
    on:keydown=move|event|{
        let next=match event.key().as_str(){
            "ArrowRight"=>match tab{Tab::Inspect=>Tab::Edit,Tab::Edit=>Tab::History,Tab::History=>Tab::Inspect},
            "ArrowLeft"=>match tab{Tab::Inspect=>Tab::History,Tab::Edit=>Tab::Inspect,Tab::History=>Tab::Edit},
            "Home"=>Tab::Inspect,"End"=>Tab::History,_=>return,
        };
        event.prevent_default();ui.tab.set(next);focus_after_render(next.id());
    }
    on:click=move|_|ui.tab.set(tab)>{label}</button>}
}

fn focus_after_render(id: &'static str) {
    let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn open_file(ui: Ui, file: web_sys::File) {
    if ui.reading.get_untracked() {
        return;
    }
    let size = file.size();
    if !size.is_finite() || size < 0.0 || size > MAX_FILE_BYTES as f64 || size.fract() != 0.0 {
        ui.error
            .set(Some("File exceeds the 8 MiB import limit.".into()));
        return;
    }
    let mut ticket = Err("document unavailable".into());
    ui.session
        .update_value(|s| ticket = s.begin_open(size as usize));
    let ticket = match ticket {
        Ok(value) => value,
        Err(error) => {
            ui.error.set(Some(error));
            return;
        }
    };
    ui.reading.set(true);
    ui.error.set(None);
    ui.status.set("Reading resource file…".into());
    spawn_local(async move {
        let result = JsFuture::from(file.array_buffer())
            .await
            .map_err(|_| "The browser could not read that file.".to_string())
            .and_then(|buffer| {
                let array = js_sys::Uint8Array::new(&buffer);
                if array.length() as usize != size as usize {
                    return Err("File size changed while being read.".into());
                }
                if !ui.session.with_value(|s| s.is_current_read(ticket)) {
                    return Err("File read was superseded.".into());
                }
                let bytes = array.to_vec();
                let mut outcome = Err("document unavailable".into());
                ui.session
                    .update_value(|s| outcome = s.complete_open(ticket, &file.name(), &bytes));
                outcome
            });
        ui.reading.set(false);
        match result {
            Ok(()) => {
                ui.tab.set(Tab::Inspect);
                ui.report(
                    Ok(true),
                    "Select a resource to inspect its bytes and editable fields.",
                );
            }
            Err(error) => ui.error.set(Some(error)),
        }
    });
}

pub fn download(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    let data = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&data);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|_| "Could not allocate download.")?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "Could not create download URL.")?;
    let result = (|| {
        let window = web_sys::window().ok_or("Browser window unavailable.")?;
        let document = window.document().ok_or("Browser document unavailable.")?;
        let link = document
            .create_element("a")
            .map_err(|_| "Could not create download link.")?
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .map_err(|_| "Download link unsupported.")?;
        link.set_href(&url);
        link.set_download(name);
        link.set_hidden(true);
        document
            .body()
            .ok_or("Browser body unavailable.")?
            .append_child(&link)
            .map_err(|_| "Could not attach download link.")?;
        link.click();
        link.remove();
        Ok::<_, String>(())
    })();
    if result.is_ok() {
        let delayed_url = url.clone();
        let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&delayed_url);
        });
        if web_sys::window()
            .and_then(|w| {
                w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.unchecked_ref(),
                    1000,
                )
                .ok()
            })
            .is_none()
        {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    } else {
        let _ = web_sys::Url::revoke_object_url(&url);
    }
    result
}
