// SPDX-License-Identifier: MPL-2.0
//! Browser controls over the same bounded source-authoring APIs as the CLI.
use crate::workbench::{
    workbench_limits, AssetKind, ExchangeFormat, ExportFormat, InputKind, MapLayer,
    NeighborhoodFields, ReadTarget, WorkbenchMode, WorkbenchSession, MAX_WORKBENCH_FILE_BYTES,
};
use leptos::{context::Provider, prelude::*};
use serde_json::{json, Value};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use wonderland_creator::json_support::{self, JsonEdit};

#[derive(Clone, Copy)]
struct WorkbenchUi {
    session: StoredValue<WorkbenchSession>,
    revision: RwSignal<u64>,
    error: RwSignal<Option<String>>,
    status: RwSignal<String>,
    busy: RwSignal<bool>,
}
impl WorkbenchUi {
    fn loaded(self) -> bool {
        self.revision.get();
        self.session.with_value(WorkbenchSession::loaded)
    }
    fn kind(self) -> Option<InputKind> {
        self.revision.get();
        self.session.with_value(WorkbenchSession::kind)
    }
    fn guard(self) -> String {
        self.session.with_value(|s| s.source_sha256().to_owned())
    }
    fn report(self, result: Result<bool, String>, message: &str) {
        match result {
            Ok(changed) => {
                self.error.set(None);
                self.status.set(if changed {
                    message.into()
                } else {
                    "No changes to apply.".into()
                });
                self.revision.update(|v| *v += 1);
            }
            Err(error) => self.error.set(Some(error)),
        }
    }
    fn run(
        self,
        action: impl FnOnce(&mut WorkbenchSession) -> Result<bool, String>,
        message: &str,
    ) {
        let mut result = Err("workbench unavailable".into());
        self.session.update_value(|s| result = action(s));
        self.report(result, message);
    }
    fn export(self, format: ExportFormat) {
        let result = self
            .session
            .with_value(|s| s.export(format))
            .and_then(|file| super::download(&file.name, &file.bytes, file.mime));
        match result {
            Ok(()) => {
                self.error.set(None);
                self.status.set("Exported validated document.".into());
            }
            Err(error) => self.error.set(Some(error)),
        }
    }
    fn edit(self, edits: &[JsonEdit], message: &str) {
        let guard = self.guard();
        self.run(|s| s.apply_json(&guard, edits, message), message);
    }
}
fn number<T: std::str::FromStr>(value: &str, label: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("{label} must be a number in its supported range."))
}
fn short(value: &str, cap: usize) -> String {
    let mut end = cap.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    if end == value.len() {
        value.to_owned()
    } else {
        format!("{}…", &value[..end])
    }
}
fn value_text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|v| match v {
            Value::String(s) => short(s, 256),
            Value::Null => String::new(),
            _ => short(&v.to_string(), 256),
        })
        .unwrap_or_default()
}
fn field_text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            _ => v.to_string(),
        })
        .unwrap_or_default()
}
fn path_value<'a>(mut tree: &'a Value, path: &[String]) -> Option<&'a Value> {
    for part in path {
        tree = if tree.is_array() {
            tree.get(part.parse::<usize>().ok()?)?
        } else {
            tree.get(part)?
        };
    }
    Some(tree)
}
fn set(path: Vec<String>, value: Value) -> JsonEdit {
    JsonEdit {
        path,
        value,
        remove: false,
    }
}

#[component]
fn Field(
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(default = 256)] max: usize,
) -> impl IntoView {
    view! { <label class="field"><span>{label}</span><input type=kind maxlength=max.to_string()
    prop:value=move||value.get() on:input=move|ev|value.set(event_target_value(&ev))/></label> }
}
#[component]
fn TextArea(
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = 65536)] max: usize,
) -> impl IntoView {
    view! { <label class="field wide"><span>{label}</span><textarea rows="6" maxlength=max.to_string()
    spellcheck="false" prop:value=move||value.get() on:input=move|ev|value.set(event_target_value(&ev))></textarea></label> }
}
#[component]
fn Upload(
    label: &'static str,
    accept: &'static str,
    #[prop(into)] target: Signal<ReadTarget>,
) -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let input = NodeRef::<leptos::html::Input>::new();
    view! {
        <input class="visually-hidden" type="file" tabindex="-1" accept=accept aria-label=label aria-hidden="true" node_ref=input
            on:change=move|ev|{
                let element = event_target::<web_sys::HtmlInputElement>(&ev);
                let file = element.files().and_then(|files|files.get(0)); element.set_value("");
                if let Some(file) = file { read_file(ui,target.get_untracked(),file); }
            }/>
        <button class="button secondary" type="button" disabled=move||ui.busy.get()
            on:click=move|_|{if let Some(input)=input.get(){input.click();}}>{label}</button>
    }
}
fn read_file(ui: WorkbenchUi, target: ReadTarget, file: web_sys::File) {
    let size = file.size();
    if !size.is_finite() || size < 0.0 || size > MAX_WORKBENCH_FILE_BYTES as f64 {
        ui.error
            .set(Some("The selected file exceeds the 4 MiB limit.".into()));
        return;
    }
    let mut ticket = Err("workbench unavailable".into());
    ui.session
        .update_value(|s| ticket = s.begin_read(target, size as usize));
    let ticket = match ticket {
        Ok(ticket) => ticket,
        Err(error) => {
            ui.error.set(Some(error));
            return;
        }
    };
    ui.busy.set(true);
    ui.error.set(None);
    spawn_local(async move {
        let result = JsFuture::from(file.array_buffer())
            .await
            .map_err(|_| "Could not read the selected file.".to_owned());
        if !ui.session.with_value(|s| s.is_current_read(ticket)) {
            return;
        }
        let result = result.and_then(|buffer| {
            let array = js_sys::Uint8Array::new(&buffer);
            if array.length() as usize != size as usize {
                return Err("The file changed size while reading.".into());
            }
            let mut bytes = vec![0; size as usize];
            array.copy_to(&mut bytes);
            let mut result = Err("workbench unavailable".into());
            ui.session
                .update_value(|s| result = s.complete_read(ticket, &file.name(), &bytes));
            result
        });
        ui.session.update_value(|s| s.cancel_read(ticket));
        ui.busy.set(false);
        ui.report(result, "File read and validated.");
    });
}

#[component]
pub fn WorkbenchPanel(mode: WorkbenchMode) -> impl IntoView {
    let ui = WorkbenchUi {
        session: StoredValue::new(WorkbenchSession::new(mode)),
        revision: RwSignal::new(0),
        error: RwSignal::new(None),
        status: RwSignal::new("Open a source file to begin.".into()),
        busy: RwSignal::new(false),
    };
    let asset_kind = RwSignal::new("fsom".to_owned());
    let target = Signal::derive(move || {
        ReadTarget::Source(match mode {
            WorkbenchMode::Upgrades => InputKind::Upgrades,
            WorkbenchMode::City => InputKind::City,
            WorkbenchMode::Neighborhood => InputKind::Neighborhood,
            WorkbenchMode::Patches => InputKind::PatchSource,
            WorkbenchMode::Assets => {
                InputKind::Asset(AssetKind::parse(&asset_kind.get()).unwrap_or(AssetKind::Fsom))
            }
        })
    });
    let accept = match mode {
        WorkbenchMode::City => ".png,.bmp",
        WorkbenchMode::Upgrades | WorkbenchMode::Neighborhood => ".json",
        WorkbenchMode::Patches => ".iff,.spf,.otf",
        WorkbenchMode::Assets => {
            ".fsom,.mesh,.anim,.skel,.bnd,.apr,.oft,.po,.col,.hag,.nbhm,application/octet-stream"
        }
    };
    view! {
        <Provider value=ui>
        <section class="workbench-panel" id=format!("workbench-{}",mode.id()) tabindex="-1"
            data-dirty=move||{ui.revision.get();ui.session.with_value(|s|s.dirty().to_string())}
            aria-busy=move||ui.busy.get().to_string()>
            <header class="workbench-heading">
                <div><h1>{mode.title()}</h1><p class="quiet">{move||{
                    ui.revision.get(); ui.session.with_value(|s|if s.loaded(){s.filename().to_owned()}else{"Open a source file. Files stay in this browser.".into()})
                }}</p></div>
                <div class="file-actions">
                    {if mode == WorkbenchMode::Assets { view! {
                        <label class="field"><span>"Source format"</span><select aria-label="Asset source format"
                            prop:value=move||asset_kind.get() on:change=move|ev|asset_kind.set(event_target_value(&ev))>
                            <option value="fsom">"FSOm mesh override"</option><option value="mesh">"Vitaboy mesh"</option>
                            <option value="animation">"Animation"</option><option value="skeleton">"Skeleton"</option>
                            <option value="binding">"Binding"</option><option value="appearance">"Appearance"</option>
                            <option value="outfit">"Outfit"</option><option value="purchasable-outfit">"Purchasable outfit"</option>
                            <option value="collection">"Collection"</option><option value="hand-group">"Hand group"</option>
                            <option value="nbhm">"NBHm house positions"</option>
                        </select></label>
                    }.into_any() } else { ().into_any() }}
                    <Upload label="Open source" accept target/>
                    <button class="button primary" disabled=move||!ui.loaded()||ui.busy.get()
                        on:click=move|_|ui.export(ExportFormat::Source)>"Export source"</button>
                </div>
            </header>
            <Show when=move||ui.error.get().is_some()>
                <div class="error-banner" role="alert"><strong>"Could not apply that action"</strong>
                    <p>{move||ui.error.get().unwrap_or_default()}</p>
                    <button type="button" on:click=move|_|ui.error.set(None)>"Dismiss"</button></div>
            </Show>
            <Show when=move||ui.loaded() fallback=move||view!{
                <div class="empty-state"><h2>"Open your source document"</h2><p>{match mode {
                    WorkbenchMode::Upgrades=>"Inspect upgrade files, groups, substitutions and levels, then apply guarded edits.",
                    WorkbenchMode::City=>"Open a PNG or BMP city layer. Paint pixels, draw roads, and export validated maps.",
                    WorkbenchMode::Neighborhood=>"Edit explicit neighborhood identities, names and city locations.",
                    WorkbenchMode::Assets=>"Choose the actual source format, inspect its metadata and edit stored transforms.",
                    WorkbenchMode::Patches=>"Open the original IFF, then attach ordered official and user PIFF files.",
                }}</p><p class="quiet">"4 MiB per file. Each workbench retains bounded local history."</p></div>
            }>
                <div class="workbench-layout">
                    <aside class="workbench-tools">
                        <fieldset class="editor-controls" disabled=move||ui.busy.get()>
                            {match mode {
                                WorkbenchMode::Upgrades=>view!{<UpgradeTools/>}.into_any(),
                                WorkbenchMode::City=>view!{<CityTools/>}.into_any(),
                                WorkbenchMode::Neighborhood=>view!{<NeighborhoodTools/>}.into_any(),
                                WorkbenchMode::Assets=>view!{<AssetTools/>}.into_any(),
                                WorkbenchMode::Patches=>view!{<PatchTools/>}.into_any(),
                            }}
                            {if matches!(mode,WorkbenchMode::Upgrades|WorkbenchMode::Neighborhood|WorkbenchMode::Assets) {
                                view!{<JsonTools/>}.into_any()
                            } else { ().into_any() }}
                        </fieldset>
                    </aside>
                    <div class="workbench-preview">
                        {match mode {
                            WorkbenchMode::City=>view!{<CityPreview/>}.into_any(),
                            WorkbenchMode::Neighborhood=>view!{<NeighborhoodPreview/>}.into_any(),
                            WorkbenchMode::Assets=>view!{<GeometryPreview/>}.into_any(),
                            _=>().into_any(),
                        }}
                        <SourceTable mode/>
                        <JsonPreview/>
                    </div>
                    <aside class="workbench-details">
                        <h2>"Source revision"</h2><code class="hash">{move||{ui.revision.get();ui.guard()}}</code>
                        <p class="quiet">"Edits are validated against this source revision before publication."</p>
                        <div class="field-row">
                            <button class="button secondary" disabled=move||ui.busy.get()||{ui.revision.get();ui.session.with_value(|s|s.undo_len()==0)}
                                on:click=move|_|ui.run(WorkbenchSession::undo,"Undid the last edit.")>"Undo"</button>
                            <button class="button secondary" disabled=move||ui.busy.get()||{ui.revision.get();ui.session.with_value(|s|s.redo_len()==0)}
                                on:click=move|_|ui.run(WorkbenchSession::redo,"Redid the last edit.")>"Redo"</button>
                        </div>
                        <p class="quiet">{move||{ui.revision.get();ui.session.with_value(|s|format!("{} undo entries · {} KiB history",s.undo_len(),s.history_bytes()/1024))}}</p>
                        <ol class="history-list">{move||{ui.revision.get();ui.session.with_value(|s|s.history()).into_iter().map(|row|view!{
                            <li><strong>{row.label}</strong><code>{short(&row.before_sha256,16)}</code></li>
                        }).collect_view()}}</ol>
                        <button class="button secondary" disabled=move||ui.busy.get() on:click=move|_|ui.export(ExportFormat::Json)>"Download inspection JSON"</button>
                    </aside>
                </div>
            </Show>
            <footer class="statusbar"><span>"Local authoring"</span><span role="status" aria-live="polite">{move||ui.status.get()}</span></footer>
        </section>
        </Provider>
    }
}

#[component]
fn JsonPreview() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let preview = RwSignal::new(String::new());
    Effect::new(move |_| {
        ui.revision.get();
        let result = ui
            .session
            .with_value(|s| s.document_json())
            .and_then(|data| json_support::encode(&data, &workbench_limits()))
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()));
        preview.set(match result {
            Ok(text) => short(&text, 65_536),
            Err(error) => error,
        });
    });
    view! {<details class="inspection-json"><summary>"Source metadata JSON"</summary>
    <p class="quiet">"Preview is limited to 64 KiB. Download the inspection for the complete metadata."</p>
    <pre>{move||preview.get()}</pre></details>}
}
#[component]
fn JsonTools() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let edits = RwSignal::new("[]".to_owned());
    let draft_guard = RwSignal::new(ui.guard());
    let draft_cleared = RwSignal::new(false);
    Effect::new(move |_| {
        ui.revision.get();
        let current = ui.guard();
        if current != draft_guard.get_untracked() {
            let had_draft = edits.with_untracked(|value| !matches!(value.trim(), "" | "[]"));
            edits.set("[]".into());
            draft_guard.set(current);
            draft_cleared.set(had_draft);
        }
    });
    view! {<details><summary>"Advanced field edits"</summary>
        <p class="quiet">"Apply an array of edits. Each path names source fields or array indices. Unknown fields remain unchanged."</p>
        <p class="quiet" role="status">{move||if draft_cleared.get(){
            "The source revision changed, so the previous field-edit draft was cleared. Inspect this source before drafting new edits."
        }else{"This draft is bound to the current source revision."}}</p>
        <code>{r#"[{"path":["field"],"remove":false,"value":"new value"}]"#}</code>
        <form on:submit=move|ev|{
            ev.prevent_default(); let guard=draft_guard.get_untracked(); let bytes=edits.get();
            ui.run(|s|s.apply_json_text(&guard,bytes.as_bytes(),"Apply field edits"),"Field edits applied.");
        }><TextArea label="Field edit JSON" value=edits/><button class="button primary" type="submit">"Apply field edits"</button></form>
        <p class="quiet">"Binary asset metadata represents stored floats as unsigned IEEE-754 bits. The transform form accepts ordinary decimal values."</p>
    </details>}
}
#[component]
fn SourceTable(mode: WorkbenchMode) -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    view! {<div class="table-scroll">{move||{
        ui.revision.get();
        let data=match ui.session.with_value(|s|s.document_json()){Ok(data)=>data,Err(error)=>return view!{<p class="quiet">{error}</p>}.into_any()};
        let rows:Vec<(String,String,String)> = match mode {
            WorkbenchMode::Upgrades=>data["Files"].as_array().into_iter().flatten().take(100).enumerate().map(|(i,file)|
                (i.to_string(),value_text(file,"Name"),format!("{} groups · {} levels",file["Groups"].as_array().map_or(0,Vec::len),file["Upgrades"].as_array().map_or(0,Vec::len)))).collect(),
            WorkbenchMode::Neighborhood=>data.as_array().into_iter().flatten().take(100).enumerate().map(|(i,entry)|
                (i.to_string(),value_text(entry,"Name"),format!("{} · ({}, {})",value_text(entry,"GUID"),entry["Location"]["X"],entry["Location"]["Y"]))).collect(),
            WorkbenchMode::Patches=>data["resources"].as_array().into_iter().flatten().take(100).map(|entry|
                (value_text(entry,"id"),value_text(entry,"kind_hex"),if entry["changed"] == true {"Changed".into()}else{"Unchanged".into()})).collect(),
            WorkbenchMode::Assets=>data.as_object().into_iter().flatten().take(100).map(|(key,value)|
                (short(key,64),match value {Value::Array(a)=>format!("{} entries",a.len()),Value::Object(o)=>format!("{} fields",o.len()),_=>short(&value.to_string(),256)},String::new())).collect(),
            WorkbenchMode::City=>data.as_object().into_iter().flatten().map(|(key,value)|(key.clone(),short(&value.to_string(),256),String::new())).collect(),
        };
        view!{<table><caption>"Source overview · first 100 rows"</caption><thead><tr><th scope="col">"Index / field"</th><th scope="col">"Name / value"</th><th scope="col">"Details"</th></tr></thead>
            <tbody>{rows.into_iter().map(|(index,name,detail)|view!{<tr><td>{index}</td><td>{name}</td><td>{detail}</td></tr>}).collect_view()}</tbody></table>}.into_any()
    }}</div>}
}

#[component]
fn UpgradeTools() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let file = RwSignal::new("0".to_owned());
    let level = RwSignal::new("0".to_owned());
    let entry = RwSignal::new("0".to_owned());
    let scope = RwSignal::new("default".to_owned());
    let old = RwSignal::new("4096:0".to_owned());
    let new = RwSignal::new("V0".to_owned());
    let level_name = RwSignal::new(String::new());
    let price = RwSignal::new("$0".to_owned());
    Effect::new(move |_| {
        ui.revision.get();
        let file = file.get();
        let level = level.get();
        let entry = entry.get();
        let scope = scope.get();
        let Ok(data) = ui.session.with_value(|s| s.document_json()) else {
            return;
        };
        let Ok(f) = file.parse::<usize>() else {
            return;
        };
        let Ok(e) = entry.parse::<usize>() else {
            return;
        };
        let l = level.parse::<usize>().unwrap_or(0);
        let subs = if scope == "level" {
            &data["Files"][f]["Upgrades"][l]["Subs"]
        } else {
            &data["Files"][f]["Subs"]
        };
        if let Some(value) = subs.get(e) {
            old.set(field_text(value, "Old"));
            new.set(field_text(value, "New"));
        }
        if let Some(value) = data["Files"][f]["Upgrades"].get(l) {
            level_name.set(field_text(value, "Name"));
            price.set(field_text(value, "Price"));
        }
    });
    view! {<h2>"Tuning substitutions"</h2><form on:submit=move|ev|{
        ev.prevent_default();
        let result=(||{
            let f=number::<usize>(&file.get(),"File index")?.to_string(); let e=number::<usize>(&entry.get(),"Substitution index")?.to_string();
            let mut path=vec!["Files".into(),f];if scope.get()=="level"{path.extend(["Upgrades".into(),number::<usize>(&level.get(),"Level index")?.to_string()]);}
            path.extend(["Subs".into(),e]); let data=ui.session.with_value(|s|s.document_json())?;
            let edits=if path_value(&data,&path).is_some(){
                let mut old_path=path.clone();old_path.push("Old".into());path.push("New".into());
                vec![set(old_path,json!(old.get())),set(path,json!(new.get()))]
            }else{vec![set(path,json!({"Old":old.get(),"New":new.get()}))]};
            Ok::<_,String>(edits)
        })();
        match result{Ok(edits)=>ui.edit(&edits,"Update substitution"),Err(error)=>ui.error.set(Some(error))}
    }>
        <Field label="Upgrade file index" value=file kind="number"/>
        <label class="field"><span>"Substitution scope"</span><select prop:value=move||scope.get() on:change=move|ev|scope.set(event_target_value(&ev))>
            <option value="default">"File defaults"</option><option value="level">"Upgrade level"</option></select></label>
        <Field label="Upgrade level index" value=level kind="number"/><Field label="Substitution index" value=entry kind="number"/>
        <Field label="Target (table:index or Ggroup)" value=old/><Field label="Replacement (Vliteral or Ctable:index)" value=new/>
        <button class="button primary" type="submit">"Apply substitution"</button>
    </form><h3>"Level description and price"</h3><form on:submit=move|ev|{
        ev.prevent_default();let result=(||{
            let f=number::<usize>(&file.get(),"File index")?.to_string();let l=number::<usize>(&level.get(),"Level index")?.to_string();
            Ok::<_,String>(vec![set(vec!["Files".into(),f.clone(),"Upgrades".into(),l.clone(),"Name".into()],json!(level_name.get())),
                set(vec!["Files".into(),f,"Upgrades".into(),l,"Price".into()],json!(price.get()))])
        })();match result{Ok(edits)=>ui.edit(&edits,"Update upgrade level"),Err(error)=>ui.error.set(Some(error))}
    }><Field label="Level name" value=level_name/><Field label="Price ($literal, Rrelative or object GUID)" value=price/>
        <button class="button secondary" type="submit">"Apply level fields"</button></form>
        <p class="quiet">"Groups, object configuration and additional fields can be edited together in Advanced field edits."</p>}
}

#[component]
fn NeighborhoodTools() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let index = RwSignal::new("0".to_owned());
    let guid = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let x = RwSignal::new("0".to_owned());
    let y = RwSignal::new("0".to_owned());
    let query_x = RwSignal::new("0".to_owned());
    let query_y = RwSignal::new("0".to_owned());
    let nearest = RwSignal::new(String::new());
    Effect::new(move |_| {
        ui.revision.get();
        let Ok(i) = index.get().parse::<usize>() else {
            return;
        };
        let Ok(data) = ui.session.with_value(|s| s.document_json()) else {
            return;
        };
        let Some(entry) = data.get(i) else {
            return;
        };
        guid.set(field_text(entry, "GUID"));
        name.set(field_text(entry, "Name"));
        description.set(field_text(entry, "Description"));
        x.set(field_text(&entry["Location"], "X"));
        y.set(field_text(&entry["Location"], "Y"));
    });
    view! {<h2>"Neighborhood fields"</h2><form on:submit=move|ev|{
        ev.prevent_default();let result=(||{
            Ok::<_,String>(NeighborhoodFields {index:number::<usize>(&index.get(),"Neighborhood index")?,
                guid:guid.get(),name:name.get(),description:description.get(),
                location:[number::<i32>(&x.get(),"X")?,number::<i32>(&y.get(),"Y")?]})
        })();match result{Ok(fields)=>{let guard=ui.guard();ui.run(|s|s.apply_neighborhood_fields(&guard,&fields),"Update neighborhood");},Err(error)=>ui.error.set(Some(error))}
    }><Field label="Neighborhood index" value=index kind="number"/><Field label="Explicit GUID" value=guid/>
        <Field label="Neighborhood name" value=name/><TextArea label="Description" value=description max=4096/>
        <div class="field-row"><Field label="City X (0–511)" value=x kind="number"/><Field label="City Y (0–511)" value=y kind="number"/></div>
        <button class="button primary" type="submit">"Apply neighborhood"</button></form>
        <h3>"Nearest neighborhood"</h3><form on:submit=move|ev|{
            ev.prevent_default();let result=ui.session.with_value(|s|s.nearest(number(&query_x.get(),"Query X")?,number(&query_y.get(),"Query Y")?));
            match result{Ok(index)=>nearest.set(index.map(|i|format!("Nearest source index: {i}")).unwrap_or("No neighborhoods.".into())),Err(error)=>ui.error.set(Some(error))}
        }><div class="field-row"><Field label="Query X" value=query_x kind="number"/><Field label="Query Y" value=query_y kind="number"/></div>
        <button class="button secondary" type="submit">"Find nearest"</button><p role="status">{move||nearest.get()}</p></form>}
}

#[component]
fn CityTools() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let layer = RwSignal::new("terrain".to_owned());
    let x = RwSignal::new("0".to_owned());
    let y = RwSignal::new("0".to_owned());
    let radius = RwSignal::new("0".to_owned());
    let red = RwSignal::new("0".to_owned());
    let green = RwSignal::new("0".to_owned());
    let blue = RwSignal::new("0".to_owned());
    let length = RwSignal::new("1".to_owned());
    let direction = RwSignal::new("0".to_owned());
    let pixel = RwSignal::new(String::new());
    view! {<h2>"Paint a city layer"</h2><form on:submit=move|ev|{
        ev.prevent_default();let result=(||{
            let layer=MapLayer::parse(&layer.get())?;let x=number(&x.get(),"X")?;let y=number(&y.get(),"Y")?;
            let radius=number(&radius.get(),"Radius")?;let rgb=[number(&red.get(),"Red")?,number(&green.get(),"Green")?,number(&blue.get(),"Blue")?];
            Ok::<_,String>((layer,x,y,radius,rgb))
        })();match result{Ok((layer,x,y,radius,rgb))=>{let guard=ui.guard();ui.run(|s|s.paint(&guard,layer,x,y,radius,rgb),"City paint applied.");},Err(error)=>ui.error.set(Some(error))}
    }><label class="field"><span>"Map layer"</span><select prop:value=move||layer.get() on:change=move|ev|layer.set(event_target_value(&ev))>
        <option value="terrain">"Terrain type"</option><option value="forest-type">"Forest type"</option>
        <option value="elevation">"Elevation"</option><option value="forest-density">"Forest density"</option>
        <option value="road">"Road bits"</option><option value="vertex-color">"Vertex color"</option></select></label>
        <div class="field-row"><Field label="Pixel X" value=x kind="number"/><Field label="Pixel Y" value=y kind="number"/></div>
        <Field label="Brush radius (0 = one pixel)" value=radius kind="number"/>
        <div class="field-row"><Field label="Red" value=red kind="number"/><Field label="Green" value=green kind="number"/><Field label="Blue" value=blue kind="number"/></div>
        <label class="field"><span>"Terrain preset"</span><select aria-label="Terrain palette preset" on:change=move|ev|{
            let values=match event_target_value(&ev).as_str(){"grass"=>[0,255,0],"water"=>[12,0,255],"snow"=>[255,255,255],"rock"=>[255,0,0],"sand"=>[255,255,0],_=>[0,0,0]};
            red.set(values[0].to_string());green.set(values[1].to_string());blue.set(values[2].to_string());
        }><option value="empty">"Empty / black"</option><option value="grass">"Grass"</option><option value="water">"Water"</option><option value="snow">"Snow"</option><option value="rock">"Rock"</option><option value="sand">"Sand"</option></select></label>
        <button class="button primary" type="submit">"Apply paint"</button>
        <button class="button secondary" type="button" on:click=move|_|{
            let result=(||{let image=ui.session.with_value(|s|s.city_image())?;image.pixel(number(&x.get(),"X")?,number(&y.get(),"Y")?)})();
            match result{Ok(p)=>pixel.set(format!("RGBA: {}, {}, {}, {}",p[0],p[1],p[2],p[3])),Err(error)=>ui.error.set(Some(error))}
        }>"Inspect pixel"</button><p role="status">{move||pixel.get()}</p></form>
        <h3>"Road strokes"</h3><p class="quiet">"Draw paired road edges and corners. The entire stroke must fit inside the map."</p>
        <Field label="Road length" value=length kind="number"/>
        <label class="field"><span>"Road direction"</span><select prop:value=move||direction.get() on:change=move|ev|direction.set(event_target_value(&ev))>
            <option value="0">"+X"</option><option value="1">"+Y"</option><option value="2">"−X"</option><option value="3">"−Y"</option></select></label>
        <div class="field-row">{[("Draw road",false),("Erase road",true)].into_iter().map(|(label,erase)|view!{
            <button class="button secondary" type="button" on:click=move|_|{
                let result=(||Ok::<_,String>((number(&x.get(),"X")?,number(&y.get(),"Y")?,number(&length.get(),"Length")?,number(&direction.get(),"Direction")?)))();
                match result{Ok((x,y,length,direction))=>{let guard=ui.guard();ui.run(|s|s.road(&guard,x,y,length,direction,erase),label);},Err(error)=>ui.error.set(Some(error))}
            }>{label}</button>
        }).collect_view()}</div>
        <div class="field-row"><button class="button secondary" on:click=move|_|ui.export(ExportFormat::Png)>"Export PNG"</button>
            <button class="button secondary" on:click=move|_|ui.export(ExportFormat::Bmp)>"Export BMP"</button></div>
        <p class="quiet">"City edits require 512 × 512 opaque maps and valid colors for the selected layer."</p>}
}

#[component]
fn CityPreview() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let url = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        ui.revision.get();
        if let Some(old) = url.get_untracked() {
            let _ = web_sys::Url::revoke_object_url(&old);
        }
        url.set(None);
        let result = ui
            .session
            .with_value(|s| {
                let image = s.city_image()?;
                if image.width() > 512 || image.height() > 512 {
                    return Err("Preview is limited to 512 × 512 pixels.".into());
                }
                image.encode_png(&workbench_limits())
            })
            .and_then(|bytes| {
                let data = js_sys::Uint8Array::from(bytes.as_slice());
                let parts = js_sys::Array::new();
                parts.push(&data);
                let options = web_sys::BlobPropertyBag::new();
                options.set_type("image/png");
                let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
                    .map_err(|_| "Could not allocate preview.".to_owned())?;
                web_sys::Url::create_object_url_with_blob(&blob)
                    .map_err(|_| "Could not create preview URL.".to_owned())
            });
        match result {
            Ok(value) => {
                url.set(Some(value));
                error.set(None);
            }
            Err(value) => error.set(Some(value)),
        }
    });
    on_cleanup(move || {
        if let Some(url) = url.get_untracked() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    });
    view! {<figure class="city-preview"><figcaption>"Current map pixels"</figcaption>
        {move||url.get().map(|url|view!{<img src=url width="512" height="512" alt="Current city map layer, decoded from the loaded source" style="image-rendering:pixelated;max-width:100%;height:auto;"/>})}
        {move||error.get().map(|error|view!{<p class="quiet">{error}</p>})}
    </figure>}
}
#[component]
fn NeighborhoodPreview() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    view! {<figure><figcaption>"Neighborhood locations"</figcaption><svg viewBox="0 0 512 512" role="img" aria-label="Neighborhood origins in the 512 by 512 city">
        <rect width="512" height="512" fill="#eff3f4"/>
        {move||{ui.revision.get();let data=ui.session.with_value(|s|s.document_json()).unwrap_or(Value::Null);
            data.as_array().into_iter().flatten().take(256).enumerate().map(|(i,entry)|{
                let x=entry["Location"]["X"].as_i64().unwrap_or(0);let y=entry["Location"]["Y"].as_i64().unwrap_or(0);
                view!{<g><circle cx=x.to_string() cy=y.to_string() r="5" fill="#176d61"/><text x=(x+8).to_string() y=(y+4).to_string() font-size="12">{format!("{i}: {}",value_text(entry,"Name"))}</text></g>}
            }).collect_view()}}
    </svg><p class="quiet">"Origins are shown in source order. Nearest lookup keeps the first entry on a tie."</p></figure>}
}

#[component]
fn AssetTools() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    // Keep the selected transform control alive while this source kind stays
    // the same. Its own revision effect refreshes values after a transaction.
    let kind = Memo::new(move |_| ui.kind());
    view! {<h2>"Asset authoring"</h2>
    {move||match kind.get(){
        Some(InputKind::Asset(kind @ (AssetKind::Mesh|AssetKind::Fsom|AssetKind::Animation|AssetKind::Skeleton|AssetKind::Nbhm)))=>view!{<TransformTools kind/>}.into_any(),
        _=>view!{<p>"Inspect the reference identifiers below. Use Advanced field edits to change source fields in one validated transaction."</p>}.into_any(),
    }}
    <Show when=move||ui.kind()==Some(InputKind::Asset(AssetKind::Animation))>
        <h3>"Animation skeleton"</h3><Upload label="Attach skeleton" accept=".skel,application/octet-stream" target=Signal::derive(||ReadTarget::Skeleton)/>
        <p class="quiet">{move||{ui.revision.get();ui.session.with_value(|s|match s.skeleton_name(){Some(name)=>format!("{} · {}",name,s.skeleton_sha256().unwrap_or("")),None=>"Attach the actual skeleton to enable glTF exchange.".into()})}}</p>
    </Show>
    <Show when=move||matches!(ui.kind(),Some(InputKind::Asset(AssetKind::Fsom|AssetKind::Animation)))>
        <h3>"Source-bound interchange"</h3><div class="field-row">
            <button class="button secondary" on:click=move|_|ui.export(ExportFormat::Glb)>"Export GLB"</button>
            <button class="button secondary" on:click=move|_|ui.export(ExportFormat::Gltf)>"Export glTF"</button>
        </div><div class="field-row">
            <Upload label="Import edited GLB" accept=".glb" target=Signal::derive(||ReadTarget::Exchange(ExchangeFormat::Glb))/>
            <Upload label="Import edited glTF" accept=".gltf" target=Signal::derive(||ReadTarget::Exchange(ExchangeFormat::Gltf))/>
        </div><p class="quiet">"Import an exchange exported from this source revision. Mesh topology, identities and animation event metadata stay protected."</p>
    </Show>
    <Show when=move||ui.kind()==Some(InputKind::Asset(AssetKind::Fsom))>
        <div class="field-row"><button class="button secondary" on:click=move|_|ui.export(ExportFormat::Obj)>"Export OBJ"</button>
            <button class="button secondary" on:click=move|_|ui.export(ExportFormat::Mtl)>"Export MTL"</button></div>
        <Upload label="Import edited OBJ" accept=".obj" target=Signal::derive(||ReadTarget::Exchange(ExchangeFormat::Obj))/>
    </Show>}
}
#[component]
fn TransformTools(kind: AssetKind) -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let group = RwSignal::new("0".to_owned());
    let geometry = RwSignal::new("0".to_owned());
    let index = RwSignal::new("0".to_owned());
    let field = RwSignal::new(
        match kind {
            AssetKind::Animation => "translations",
            AssetKind::Skeleton => "translation",
            _ => "position",
        }
        .to_owned(),
    );
    let x = RwSignal::new("0".to_owned());
    let y = RwSignal::new("0".to_owned());
    let z = RwSignal::new("0".to_owned());
    let w = RwSignal::new("1".to_owned());
    let path = move || -> Result<Vec<String>, String> {
        let index = number::<usize>(&index.get(), "Element index")?.to_string();
        let field = field.get();
        Ok(match kind {
            AssetKind::Fsom => vec![
                "groups".into(),
                number::<usize>(&group.get(), "Dynamic group")?.to_string(),
                number::<usize>(&geometry.get(), "Geometry index")?.to_string(),
                "vertices".into(),
                index,
                field,
            ],
            AssetKind::Animation => vec![field, index],
            AssetKind::Skeleton => vec!["bones".into(), index, field],
            AssetKind::Nbhm => vec!["houses".into(), index, "position".into()],
            _ => vec!["vertices".into(), index, field],
        })
    };
    Effect::new(move |_| {
        ui.revision.get();
        let Ok(path) = path() else {
            return;
        };
        let Ok(data) = ui.session.with_value(|s| s.document_json()) else {
            return;
        };
        let Some(values) = path_value(&data, &path).and_then(Value::as_array) else {
            return;
        };
        for (signal, value) in [x, y, z, w].into_iter().zip(values) {
            if let Some(bits) = value.as_u64().and_then(|v| u32::try_from(v).ok()) {
                signal.set(f32::from_bits(bits).to_string());
            }
        }
    });
    view! {<form on:submit=move|ev|{
        ev.prevent_default();let result=(||{
            let path=path()?;let data=ui.session.with_value(|s|s.document_json())?;
            let old=path_value(&data,&path).and_then(Value::as_array).ok_or("Selected transform does not exist.")?;
            let mut values=Vec::new();for signal in [x,y,z,w].into_iter().take(old.len()){
                let value=number::<f32>(&signal.get(),"Transform component")?;if !value.is_finite(){return Err("Transforms must be finite.".into());}
                values.push(value);
            }
            if !(2..=4).contains(&values.len()){return Err("Unsupported transform width.".into());}
            Ok::<_,String>((path,values))
        })();match result{Ok((path,values))=>{let guard=ui.guard();ui.run(|s|s.apply_transform(&guard,&path,&values),"Update stored transform");},Err(error)=>ui.error.set(Some(error))}
    }>
        {if kind==AssetKind::Fsom{view!{<div class="field-row"><Field label="Dynamic group" value=group kind="number"/><Field label="Geometry index" value=geometry kind="number"/></div>}.into_any()}else{().into_any()}}
        <Field label="Element index" value=index kind="number"/>
        <label class="field"><span>"Stored transform"</span><select prop:value=move||field.get() on:change=move|ev|field.set(event_target_value(&ev))>
            {match kind{
                AssetKind::Animation=>view!{<option value="translations">"Translation sample"</option><option value="rotations">"Rotation sample"</option>}.into_any(),
                AssetKind::Skeleton=>view!{<option value="translation">"Bone translation"</option><option value="rotation">"Bone rotation"</option>}.into_any(),
                AssetKind::Nbhm=>view!{<option value="position">"House position"</option>}.into_any(),
                _=>view!{<option value="position">"Vertex position"</option><option value="normal">"Vertex normal"</option><option value="texture_coordinate">"UV coordinate"</option>}.into_any(),
            }}</select></label>
        <div class="field-row"><Field label="X / U" value=x/><Field label="Y / V" value=y/></div>
        <div class="field-row"><Field label="Z" value=z/><Field label="W (rotations)" value=w/></div>
        <button class="button primary" type="submit">"Apply transform"</button>
        <p class="quiet">{if kind==AssetKind::Fsom {
            "Values use stored FreeSO coordinates. Position edits also recalculate visible mesh bounds."
        }else{"Values use stored FreeSO coordinates. The selected vector is edited in one transaction."}}</p>
    </form>}
}
#[component]
fn GeometryPreview() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    view! {<figure class="geometry-preview"><figcaption>"Stored geometry projection"</figcaption>{move||{
        ui.revision.get();let Ok(data)=ui.session.with_value(|s|s.document_json()) else{return ().into_any();};
        let mut segments=Vec::<[[f64;2];2]>::new();
        let point=|value:&Value|->Option<[f64;2]>{
            let values=value.as_array()?;let mut p=[0f64;3];for (target,value) in p.iter_mut().zip(values){*target=f32::from_bits(u32::try_from(value.as_u64()?).ok()?) as f64;}
            Some([p[0]-p[2]*0.5,-p[1]-p[2]*0.25])
        };
        let mut add=|vertices:&Value,faces:&Value,flat:bool|{
            let Some(vertices)=vertices.as_array() else{return;};let Some(faces)=faces.as_array() else{return;};
            let face_count=if flat{faces.len()/3}else{faces.len()};
            for face in 0..face_count.min(500){
                let mut points=Vec::new();for corner in 0..3{
                    let index=if flat{&faces[face*3+corner]}else{&faces[face][corner]};
                    let Some(vertex)=index.as_u64().and_then(|v|vertices.get(v as usize)) else{return;};
                    let Some(p)=point(&vertex["position"]) else{return;};points.push(p);
                }
                if segments.len()+3>1500{return;}
                segments.extend([[points[0],points[1]],[points[1],points[2]],[points[2],points[0]]]);
            }
        };
        match ui.kind(){
            Some(InputKind::Asset(AssetKind::Mesh))=>add(&data["vertices"],&data["faces"],false),
            Some(InputKind::Asset(AssetKind::Fsom))=>for group in data["groups"].as_array().into_iter().flatten(){
                for geometry in group.as_array().into_iter().flatten(){add(&geometry["vertices"],&geometry["indices"],true);}
            },
            _=>return view!{<p class="quiet">"Inspect stored transforms and source metadata below."</p>}.into_any(),
        }
        if segments.is_empty(){return view!{<p class="quiet">"No visible triangles in this source."</p>}.into_any();}
        let mut min=[f64::INFINITY;2];let mut max=[f64::NEG_INFINITY;2];
        for point in segments.iter().flatten(){for i in 0..2{min[i]=min[i].min(point[i]);max[i]=max[i].max(point[i]);}}
        let scale=440.0/(max[0]-min[0]).max(max[1]-min[1]).max(0.000001);
        let mut path=String::new();for [a,b] in segments{
            use std::fmt::Write;let _=write!(path,"M{:.2},{:.2}L{:.2},{:.2}",36.0+(a[0]-min[0])*scale,36.0+(a[1]-min[1])*scale,36.0+(b[0]-min[0])*scale,36.0+(b[1]-min[1])*scale);
        }
        view!{<svg viewBox="0 0 512 512" role="img" aria-label="Projection of stored mesh triangles before skinning or material rendering">
            <rect width="512" height="512" fill="#f2f5f5"/><path d=path fill="none" stroke="#176d61" stroke-width="1"/>
        </svg><p class="quiet">"Up to 500 source triangles. This view shows stored vertices before skinning and material rendering."</p>}.into_any()
    }}</figure>}
}

#[component]
fn PatchTools() -> impl IntoView {
    let ui = expect_context::<WorkbenchUi>();
    let source_name = RwSignal::new(String::new());
    Effect::new(move |_| {
        ui.revision.get();
        source_name.set(ui.session.with_value(|s| s.source_name().to_owned()));
    });
    view! {<h2>"Patch inputs"</h2><form on:submit=move|ev|{
        ev.prevent_default();let guard=ui.guard();let name=source_name.get();
        ui.run(|s|s.set_patch_source_name(&guard,&name),"Updated exact source name.");
    }><Field label="Exact source IFF name" value=source_name/><button class="button secondary" type="submit">"Apply source name"</button></form>
        <p class="quiet">"PIFF matching uses this exact source name. User patches suppress official patches using the original resolver's rules."</p>
        <div class="field-row">
            <Upload label="Add official PIFF" accept=".piff,.iff" target=Signal::derive(||ReadTarget::Patch{is_user:false})/>
            <Upload label="Add user PIFF" accept=".piff,.iff" target=Signal::derive(||ReadTarget::Patch{is_user:true})/>
        </div>
        <ol class="patch-inputs">{move||{ui.revision.get();ui.session.with_value(|s|s.attachments()).into_iter().enumerate().map(|(index,patch)|view!{
            <li><strong>{patch.name}</strong><p class="quiet">{format!("{} · {} bytes · {}",if patch.is_user{"User"}else{"Official"},patch.bytes,short(&patch.sha256,16))}</p>
                <div class="field-row"><button class="button secondary" type="button" on:click=move|_|ui.run(|s|s.move_patch(index,true),"Moved patch earlier.")>"Move up"</button>
                    <button class="button secondary" type="button" on:click=move|_|ui.run(|s|s.move_patch(index,false),"Moved patch later.")>"Move down"</button>
                    <button class="button secondary" type="button" on:click=move|_|ui.run(|s|s.remove_patch(index),"Removed patch input.")>"Remove"</button></div>
            </li>
        }).collect_view()}}</ol>
        <button class="button primary" on:click=move|_|ui.export(ExportFormat::EffectiveIff)>"Export effective IFF"</button>
        <p class="quiet">"The original source remains available. Inspection JSON includes applied and suppressed patch provenance."</p>
    }
}
