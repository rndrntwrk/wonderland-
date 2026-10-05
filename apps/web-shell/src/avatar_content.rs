//! Original resource bytes are session-owned and never included in saved metadata.
use crate::{authoring_bridge::AuthorUi, avatar_renderer::*, bridge::Ui};
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen::{JsCast, JsValue};
use wonderland_avatar_content::{self as content, CollectionRole, ImportedContent};
use wonderland_contracts::{Availability, authoring::*};

type SourceFiles = Arc<Vec<(String, Vec<u8>)>>;

/// A content consumer supplies its own busy/revision guard. Connected accounts
/// never need a preview provider to import or render original resources.
#[derive(Clone, Copy)]
pub struct ContentTarget {
    pub busy: Signal<bool>,
    pub state_revision: Signal<u64>,
    pub content_revision: Signal<u64>,
    pub install: Callback<AppearanceContent, Result<(), String>>,
}

impl ContentTarget {
    fn preview(author: AuthorUi) -> Self {
        Self {
            busy: Signal::derive(move || author.busy()),
            state_revision: Signal::derive(move || author.state.with(|s| s.projection().revision)),
            content_revision: Signal::derive(move || {
                author
                    .state
                    .with(|s| s.projection().appearance_content.revision)
            }),
            install: Callback::new(move |content| {
                author.install_content(content).map_err(|e| e.to_string())
            }),
        }
    }
}

#[derive(Clone, Copy)]
pub struct AvatarMotion(pub Signal<bool>);

#[derive(Clone, Copy)]
pub struct ContentUi {
    pub imported: RwSignal<Option<Arc<ImportedContent>>>,
    pub choices: RwSignal<Option<AppearanceContent>>,
    files: StoredValue<SourceFiles>,
    generation: RwSignal<u64>,
    pub loading: RwSignal<bool>,
    pub notice: RwSignal<String>,
    skeleton: RwSignal<String>,
    head: RwSignal<String>,
    body: RwSignal<String>,
    inventory: RwSignal<Vec<String>>,
}
impl ContentUi {
    pub fn new() -> Self {
        let s = Self {
            imported: RwSignal::new(None),
            choices: RwSignal::new(None),
            files: StoredValue::new(Arc::new(vec![])),
            generation: RwSignal::new(0),
            loading: RwSignal::new(false),
            notice: RwSignal::new("Load original game files to compose your character.".into()),
            skeleton: RwSignal::new("adult.skel".into()),
            head: RwSignal::new("ea_male_heads.col, ea_female_heads.col".into()),
            body: RwSignal::new("ea_male.col, ea_female.col".into()),
            inventory: RwSignal::new(vec![]),
        };
        on_cleanup(move || {
            if let Some(v) = s.generation.try_get_untracked() {
                s.generation.try_set(v.wrapping_add(1));
            }
        });
        s
    }
    fn load(self, event: web_sys::Event, target: ContentTarget) {
        let Some(input) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        if target.busy.get_untracked() {
            self.notice
                .set("Finish the pending save before loading content.".into());
            return;
        }
        let limits = browser_limits();
        let mut total = 0.;
        let mut selected = vec![];
        for i in 0..files.length() {
            if let Some(file) = files.item(i) {
                total += file.size();
                selected.push(file);
            }
        }
        input.set_value("");
        if selected.len() > limits.max_files || total > limits.max_total_input_bytes as f64 {
            self.notice.set("This selection exceeds the browser import memory budget. Load a smaller original resource set.".into());
            return;
        }
        let generation = self.generation.get_untracked().wrapping_add(1);
        self.generation.set(generation);
        self.loading.set(true);
        let revision = target.state_revision.get_untracked();
        wasm_bindgen_futures::spawn_local(async move {
            let mut bytes = vec![];
            for file in selected {
                if self.generation.try_get_untracked() != Some(generation) {
                    return;
                }
                match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buffer) => {
                        let relative = js_sys::Reflect::get(
                            file.as_ref(),
                            &JsValue::from_str("webkitRelativePath"),
                        )
                        .ok()
                        .and_then(|v| v.as_string())
                        .filter(|v| !v.is_empty())
                        .unwrap_or_else(|| file.name());
                        bytes.push((relative, js_sys::Uint8Array::new(&buffer).to_vec()));
                    }
                    Err(_) => {
                        if self.generation.try_get_untracked() != Some(generation) {
                            return;
                        }
                        self.loading.try_set(false);
                        self.notice.try_set(format!(
                            "Could not read {}. Previous content is preserved.",
                            file.name()
                        ));
                        return;
                    }
                }
            }
            if self.generation.try_get_untracked() != Some(generation) {
                return;
            }
            self.loading.set(false);
            if target.busy.get_untracked() || target.state_revision.get_untracked() != revision {
                self.notice.set("The game changed during import. Select the files again after the pending operation finishes.".into());
                return;
            }
            let descriptors = content::inventory(
                &bytes
                    .iter()
                    .map(|(name, bytes)| content::NamedBytes {
                        name,
                        bytes,
                        key: resource_key(name),
                    })
                    .collect::<Vec<_>>(),
                &browser_limits(),
            );
            if let Ok(descriptors) = descriptors {
                if let Some(skeleton) = descriptors.iter().find(|d| {
                    d.kind == content::ResourceKind::Skeleton
                        && d.name.rsplit('/').next() == Some("adult.skel")
                }) {
                    self.skeleton.set(skeleton.name.clone());
                }
                let names = descriptors
                    .iter()
                    .filter(|d| d.kind == content::ResourceKind::Collection)
                    .map(|d| d.name.clone())
                    .collect::<Vec<_>>();
                for (signal, defaults) in [
                    (self.head, ["ea_male_heads.col", "ea_female_heads.col"]),
                    (self.body, ["ea_male.col", "ea_female.col"]),
                ] {
                    let found = names
                        .iter()
                        .filter(|n| defaults.contains(&n.as_str()))
                        .cloned()
                        .collect::<Vec<_>>();
                    if !found.is_empty() {
                        signal.set(found.join(", "));
                    }
                }
                self.inventory.set(names);
            }
            let count = bytes.len();
            self.files.set_value(Arc::new(bytes));
            self.notice.set(format!(
                "{count} original files read. Check the collection names, then choose Apply files."
            ));
        });
    }
    fn apply(self, target: ContentTarget) {
        if self.loading.get_untracked() || target.busy.get_untracked() {
            return;
        }
        let files = self.files.get_value();
        if files.is_empty() {
            self.notice
                .set("Choose original files or a game folder first.".into());
            return;
        }
        let skeleton = self.skeleton.get_untracked();
        let mut collections = vec![];
        for (name, role) in [
            (self.head.get_untracked(), CollectionRole::Head),
            (self.body.get_untracked(), CollectionRole::Body),
        ] {
            for name in name.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                collections.push(content::CollectionSpec {
                    name: name.into(),
                    role,
                });
            }
        }
        let result = content::import(
            content::ImportRequest {
                files: files
                    .iter()
                    .map(|(name, bytes)| content::NamedBytes {
                        name,
                        bytes,
                        key: resource_key(name),
                    })
                    .collect(),
                skeleton_name: &skeleton,
                collections,
            },
            &browser_limits(),
        );
        match result {
            Err(e) => self.notice.set(e.to_string()),
            Ok(imported) => {
                let metadata = metadata(
                    &imported,
                    target.content_revision.get_untracked().saturating_add(1),
                );
                let heads = metadata
                    .heads
                    .iter()
                    .filter(|o| o.availability.is_available())
                    .count();
                let bodies = metadata
                    .bodies
                    .iter()
                    .filter(|o| o.availability.is_available())
                    .count();
                match target.install.run(metadata.clone()) {
                    Ok(()) => {
                        self.generation.update(|g| *g = g.wrapping_add(1));
                        self.imported.set(Some(Arc::new(imported)));
                        self.choices.set(Some(metadata));
                        self.notice
                            .set(format!("{heads} heads · {bodies} bodies ready"));
                    }
                    Err(e) => self.notice.set(e.to_string()),
                }
            }
        }
    }
}
impl Default for ContentUi {
    fn default() -> Self {
        Self::new()
    }
}
fn browser_limits() -> content::ImportLimits {
    content::ImportLimits {
        max_files: 100_000,
        max_total_input_bytes: 256 * 1024 * 1024,
        max_total_resource_bytes: 256 * 1024 * 1024,
        ..Default::default()
    }
}
fn resource_key(name: &str) -> Option<content::ResourceKey> {
    let (file_id, type_id) = crate::source_identity::original_file_key(name)?;
    Some(content::ResourceKey {
        group_id: 0,
        file_id,
        type_id,
    })
}
fn metadata(imported: &ImportedContent, revision: u64) -> AppearanceContent {
    let skin_keys = ["light", "medium", "dark"];
    let mut heads = std::collections::BTreeMap::<ContentKey, AppearanceOption>::new();
    let mut bodies = heads.clone();
    let mut genders = std::collections::BTreeSet::new();
    for choice in &imported.choices {
        // Native creator derives gender from its selected collection, not .po's
        // raw Gender field (female head purchasables may also contain zero).
        let collection_gender = match choice.collection.as_str() {
            "ea_male_heads.col" | "ea_male.col" => Some(0),
            "ea_female_heads.col" | "ea_female.col" => Some(1),
            _ => None,
        };
        if let Some(g) = collection_gender {
            genders.insert(g);
        }
        let Some(outfit) = choice.outfit else {
            continue;
        };
        let ready = choice.skins.iter().any(|s| s.ready);
        let reason = choice
            .skins
            .iter()
            .flat_map(|s| &s.issues)
            .next()
            .map(ToString::to_string)
            .unwrap_or("Required source resources are missing.".into());
        let reason = reason.chars().take(240).collect::<String>();
        let key: ContentKey = content::file_content_key(outfit).into();
        let option = AppearanceOption {
            key: key.clone(),
            label: format!(
                "{}{} {}",
                match collection_gender {
                    Some(0) => "Male ",
                    Some(1) => "Female ",
                    _ => "",
                },
                match choice.role {
                    CollectionRole::Head => "Head",
                    CollectionRole::Body => "Body",
                },
                i64::from(choice.index) + 1
            ),
            thumbnail: None,
            availability: if ready {
                Availability::Available
            } else {
                Availability::Unavailable { reason }
            },
            genders: collection_gender
                .map(|g| vec![format!("vitaboy:gender:{g}").into()])
                .unwrap_or_default(),
            skin_tones: choice
                .skins
                .iter()
                .enumerate()
                .filter(|(_, s)| s.ready)
                .map(|(i, _)| format!("vitaboy:skin:{}", skin_keys[i]).into())
                .collect(),
        };
        let map = match choice.role {
            CollectionRole::Head => &mut heads,
            CollectionRole::Body => &mut bodies,
        };
        if let Some(existing) = map.get_mut(&key) {
            if option.availability.is_available() {
                existing.availability = Availability::Available;
            }
            if existing.genders.is_empty() || option.genders.is_empty() {
                existing.genders.clear();
            } else {
                existing.genders.extend(option.genders);
                existing.genders.sort();
                existing.genders.dedup();
            }
            existing.skin_tones.extend(option.skin_tones);
            existing.skin_tones.sort();
            existing.skin_tones.dedup();
        } else {
            map.insert(key, option);
        }
    }
    let option = |key: String, label: String| AppearanceOption {
        key: key.into(),
        label,
        thumbnail: None,
        availability: Availability::Available,
        genders: vec![],
        skin_tones: vec![],
    };
    AppearanceContent {
        source: "original-vitaboy".into(),
        revision,
        heads: heads.into_values().collect(),
        bodies: bodies.into_values().collect(),
        skin_tones: skin_keys
            .into_iter()
            .map(|s| option(format!("vitaboy:skin:{s}"), s.into()))
            .collect(),
        genders: genders
            .into_iter()
            .map(|g| {
                option(
                    format!("vitaboy:gender:{g}"),
                    match g {
                        0 => "Male".into(),
                        1 => "Female".into(),
                        _ => format!("Source gender {g}"),
                    },
                )
            })
            .collect(),
        requirements: AppearanceRequirements {
            head: true,
            body: true,
            skin_tone: true,
            gender: false,
        },
        rendering: if imported.rig.is_some() {
            Availability::Available
        } else {
            Availability::Unavailable {
                reason: "Original skeleton not found. Check its exact archive name.".into(),
            }
        },
    }
}
#[component]
pub fn ContentLoader() -> impl IntoView {
    let content = expect_context::<ContentUi>();
    let target = use_context::<ContentTarget>()
        .unwrap_or_else(|| ContentTarget::preview(expect_context::<AuthorUi>()));
    let folder = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = folder.get() {
            let _ = input.set_attribute("webkitdirectory", "");
        }
    });
    view! {<details class="content-loader game-content" open=move ||content.imported.get().is_none()><summary>"Game content"</summary>
        <div class="content-load-actions"><label class="chrome load-files">"Load game files"<input type="file" multiple=true disabled=move ||target.busy.get() on:change=move |e|content.load(e,target)/></label><label class="chrome load-files">"Load folder"<input type="file" multiple=true node_ref=folder disabled=move ||target.busy.get() on:change=move |e|content.load(e,target)/></label></div>
        <details><summary>"Resource names and collection roles"</summary><label>"Skeleton"<input prop:value=move ||content.skeleton.get() on:input=move |e|content.skeleton.set(event_target_value(&e))/></label><label>"Head collections"<input list="source-collections" prop:value=move ||content.head.get() on:input=move |e|content.head.set(event_target_value(&e))/></label><label>"Body collections"<input list="source-collections" prop:value=move ||content.body.get() on:input=move |e|content.body.set(event_target_value(&e))/></label><datalist id="source-collections"><For each=move ||content.inventory.get() key=|n|n.clone() children=move |name|view!{<option value=name/>}/></datalist><Show when=move ||content.imported.with(|bank|bank.as_ref().is_some_and(|bank|!bank.issues.is_empty()))><details class="content-diagnostics"><summary>"Other resource diagnostics"</summary><For each=move ||content.imported.with(|bank|bank.as_ref().map(|bank|bank.issues.iter().take(20).map(ToString::to_string).collect::<Vec<_>>()).unwrap_or_default()) key=|issue|issue.clone() children=move |issue|view!{<p>{issue}</p>}/></details></Show><p>"Use exact archive names; separate multiple collections with commas.  Standalone numeric resources must retain their original 16-digit packed IDs."</p></details>
        <button class="chrome" disabled=move ||target.busy.get()||content.loading.get() on:click=move |_|content.apply(target)>"Apply files"</button><p role="status">{move ||if content.loading.get(){"Reading original files…".into()}else{content.notice.get()}}</p>
    </details>}
}
fn selection(imported: &ImportedContent, a: &AppearanceSelection) -> content::AppearanceSelection {
    let lookup = |key: &Option<ContentKey>| {
        key.as_ref()
            .and_then(|key| crate::source_identity::outfit_key(key.as_ref()))
            .map(|(file_id, type_id)| content::FileKey { file_id, type_id })
    };
    let _ = imported;
    content::AppearanceSelection {
        head: lookup(&a.head),
        body: lookup(&a.body),
        skin: match a.skin_tone.as_ref().map(|s| s.as_ref()) {
            Some("vitaboy:skin:medium") => content::Skin::Medium,
            Some("vitaboy:skin:dark") => content::Skin::Dark,
            _ => content::Skin::Light,
        },
        ..Default::default()
    }
}
fn mesh_array(imported: &ImportedContent, parts: Vec<content::RenderablePart>) -> js_sys::Array {
    let array = js_sys::Array::new();
    for part in parts {
        let Some(texture) = imported.textures.get(&part.texture) else {
            continue;
        };
        let object = js_sys::Object::new();
        for (name, value) in [
            (
                "positions",
                js_sys::Float32Array::from(
                    part.mesh
                        .vertices
                        .iter()
                        .flat_map(|v| [v.position.x, v.position.y, v.position.z])
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .into(),
            ),
            (
                "normals",
                js_sys::Float32Array::from(
                    part.mesh
                        .vertices
                        .iter()
                        .flat_map(|v| [v.normal.x, v.normal.y, v.normal.z])
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .into(),
            ),
            (
                "uvs",
                js_sys::Float32Array::from(
                    part.mesh
                        .vertices
                        .iter()
                        .flat_map(|v| [v.uv.x, v.uv.y])
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .into(),
            ),
            (
                "indices",
                js_sys::Uint32Array::from(part.mesh.indices.as_slice()).into(),
            ),
            (
                "bytes",
                js_sys::Uint8Array::from(texture.bytes.as_slice()).into(),
            ),
            ("mime", JsValue::from_str(texture.mime)),
        ] {
            let _ = js_sys::Reflect::set(&object, &JsValue::from_str(name), &value);
        }
        array.push(&object);
    }
    array
}
#[component]
pub fn AvatarStage(
    #[prop(into)] appearance: Signal<AppearanceSelection>,
    #[prop(default = false)] thumbnail: bool,
) -> impl IntoView {
    let content = expect_context::<ContentUi>();
    let reduced_motion = use_context::<AvatarMotion>()
        .map(|m| m.0)
        .or_else(|| use_context::<Ui>().map(|ui| Signal::derive(move || ui.reduced_motion.get())))
        .unwrap_or_else(|| Signal::derive(|| false));
    let appearance = Memo::new(move |_| appearance.get());
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let message = RwSignal::new(String::new());
    let generation = RwSignal::new(0u64);
    Effect::new(move |_| {
        let selected = appearance.get();
        let imported = content.imported.get();
        let reduced = reduced_motion.get() || thumbnail;
        let Some(canvas) = canvas.get() else {
            return;
        };
        drop_avatar(&canvas);
        generation.update(|v| *v = v.wrapping_add(1));
        let expected = generation.get_untracked();
        if !selected.decorations.is_empty() {
            message.set("Your saved decorations are retained. This source needs category-to-renderer role bindings before the complete outfit can be previewed.".into());
            return;
        }
        let Some(imported) = imported else {
            message.set("Original avatar resources are not loaded.".into());
            return;
        };
        let mut selected = selection(&imported, &selected);
        if thumbnail {
            selected.left = content::Gesture::None;
            selected.right = content::Gesture::None;
        }
        if selected.head.is_none() && selected.body.is_none() {
            message.set("Choose source content for the character stage.".into());
            return;
        }
        match imported.compose(&selected) {
            Err(issues) => message.set(
                issues
                    .iter()
                    .take(3)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
            Ok(parts) => {
                message.set("Preparing textured avatar…".into());
                let promise = render_avatar(&canvas, &mesh_array(&imported, parts), reduced);
                wasm_bindgen_futures::spawn_local(async move {
                    let result = wasm_bindgen_futures::JsFuture::from(promise).await;
                    if generation.try_get_untracked() != Some(expected) {
                        return;
                    }
                    match result {
                        Ok(v) if v.as_bool() == Some(true) => message.set(String::new()),
                        Ok(_) => {}
                        Err(e) => {
                            let detail = js_sys::Reflect::get(&e, &JsValue::from_str("message"))
                                .ok()
                                .and_then(|v| v.as_string())
                                .or_else(|| e.as_string())
                                .unwrap_or_else(|| "Unknown graphics error".into());
                            message.set(format!("Avatar preview: {detail}"));
                        }
                    }
                });
            }
        }
    });
    on_cleanup(move || {
        generation.try_update(|v| *v = v.wrapping_add(1));
        if let Some(canvas) = canvas.get_untracked() {
            dispose_avatar(&canvas);
        }
    });
    view! {<div class="avatar-render-stage" class:thumbnail=thumbnail><canvas node_ref=canvas aria-label="Textured original avatar preview"/><p class="graphics-error" role="status"></p><span class="avatar-render-mode"></span><Show when=move ||!message.get().is_empty()><p class="avatar-render-status" role="status">{move ||message.get()}</p></Show><Show when=move ||!thumbnail&&message.get().is_empty()><div class="avatar-turn-controls"><button class="chrome" aria-label="Rotate avatar left" on:click=move |_|if let Some(c)=canvas.get_untracked(){turn_avatar(&c,-30.);}>"↶"</button><span>"3D character"</span><button class="chrome" aria-label="Rotate avatar right" on:click=move |_|if let Some(c)=canvas.get_untracked(){turn_avatar(&c,30.);}>"↷"</button></div></Show></div>}
}

/// Original .apr thumbnail artwork is decoded by the browser. These object URLs
/// are disposable source-bank views, never saved profile portrait references.
#[component]
pub fn OriginalThumbnail(
    content_key: ContentKey,
    #[prop(into)] skin: Signal<Option<ContentKey>>,
) -> impl IntoView {
    let content = expect_context::<ContentUi>();
    let url = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        if let Some(previous) = url.get_untracked() {
            revoke_texture_url(&previous);
        }
        let index = match skin.get().as_ref().map(|s| s.as_ref()) {
            Some("vitaboy:skin:medium") => 1,
            Some("vitaboy:skin:dark") => 2,
            _ => 0,
        };
        let value = content.imported.with(|bank| {
            bank.as_ref().and_then(|bank| {
                bank.choices
                    .iter()
                    .filter(|c| {
                        c.outfit
                            .is_some_and(|o| content::file_content_key(o) == content_key.as_ref())
                    })
                    .find_map(|c| c.thumbnails[index])
                    .and_then(|key| bank.textures.get(&key))
                    .map(|texture| {
                        texture_url(
                            &js_sys::Uint8Array::from(texture.bytes.as_slice()),
                            texture.mime,
                        )
                    })
            })
        });
        url.set(value);
    });
    on_cleanup(move || {
        if let Some(Some(url)) = url.try_get_untracked() {
            revoke_texture_url(&url);
        }
    });
    view! {<Show when=move ||url.get().is_some() fallback=||view!{<span class="source-thumbnail-missing">"No source thumbnail"</span>}><img class="source-thumbnail" src=move ||url.get().unwrap_or_default() alt=""/></Show>}
}
