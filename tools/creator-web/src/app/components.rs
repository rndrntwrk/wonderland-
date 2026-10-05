// SPDX-License-Identifier: MPL-2.0
use super::Ui;
use crate::session::Inspection;
use leptos::prelude::*;
use serde_json::Value;

#[component]
pub fn ResourceIcon() -> impl IntoView {
    view! {<svg class="resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" aria-hidden="true"><path d="M5 2.5h9l5 5v14H5z"/><path d="M14 2.5v5h5"/></svg>}
}

#[component]
pub fn Details(inspection: Inspection) -> impl IntoView {
    let source = inspection.guard.source_hash.clone();
    let resource = inspection.guard.resource_hash.clone();
    view! {
        <h2>"Resource details"</h2>
        <dl class="resource-details">
            <dt>"Type"</dt><dd>{inspection.row.kind}</dd>
            <dt>"ID"</dt><dd>{inspection.row.key.id}</dd>
            <dt>"Format version"</dt><dd>{inspection.details.get("string_format").and_then(Value::as_i64).map(|v|v.to_string()).or_else(||inspection.format_version.map(|v|v.to_string())).unwrap_or_else(||if inspection.error.is_some(){"Unavailable".into()}else{"Unversioned".into()})}</dd>
            <dt>"Bytes"</dt><dd>{inspection.row.bytes}</dd>
            <dt>"Flags"</dt><dd>{format!("0x{:04x}",inspection.row.flags)}</dd>
            <dt>"Source hash"</dt><dd><code title=source.clone()>{short_hash(&source)}</code></dd>
            <dt>"Resource hash"</dt><dd><code title=resource.clone()>{short_hash(&resource)}</code></dd>
        </dl>
        <div class="validation-note"><svg width="23" height="23" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v1"/></svg><p>"Edits validate before export."</p></div>
        <details class="hashes"><summary>"Full hashes"</summary><h3>"Source SHA-256"</h3><code>{source}</code><h3>"Resource SHA-256"</h3><code>{resource}</code></details>
    }
}
fn short_hash(value: &str) -> String {
    format!("{}…", &value[..value.len().min(12)])
}

#[component]
pub fn Inspector(inspection: Inspection) -> impl IntoView {
    let data = inspection.details;
    let body = if let Some(instructions) = data.get("instructions").and_then(Value::as_array) {
        let rows = instructions
            .iter()
            .map(|v| {
                vec![
                    text(v, "index"),
                    format!("0x{:04x}", v["opcode"].as_u64().unwrap_or(0)),
                    branch(v["true"].as_u64().unwrap_or(0)),
                    branch(v["false"].as_u64().unwrap_or(0)),
                    text(v, "operand_hex"),
                ]
            })
            .collect();
        let instructions = instructions.clone();
        view!{<DataTable title="Instructions" columns=vec!["Index","Opcode","True","False","Operand"] rows/><ControlFlow instructions/>}.into_any()
    } else if let Some(sets) = data.get("sets").and_then(Value::as_array) {
        let mut rows: Vec<Vec<String>> = sets
            .iter()
            .enumerate()
            .flat_map(|(s, set)| {
                set.as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(move |(i, v)| {
                        vec![
                            s.to_string(),
                            i.to_string(),
                            text(v, "language"),
                            text(v, "value"),
                            text(v, "comment"),
                        ]
                    })
            })
            .collect();
        let unassigned = data.get("unassigned").and_then(Value::as_array);
        let has_unassigned = unassigned.is_some_and(|entries| !entries.is_empty());
        if let Some(entries) = unassigned {
            rows.extend(entries.iter().enumerate().map(|(i, v)| {
                vec![
                    "Unassigned".into(),
                    i.to_string(),
                    text(v, "language"),
                    text(v, "value"),
                    text(v, "comment"),
                ]
            }));
        }
        view!{<DataTable title="Strings" columns=vec!["Set","Index","Language","Value","Comment"] rows/>{has_unassigned.then(||view!{<p class="quiet">"Unassigned source language entries are preserved in their original order. Typed editing addresses the recognized string sets."</p>})}}.into_any()
    } else if let Some(constants) = data.get("constants").and_then(Value::as_array) {
        let rows = constants
            .iter()
            .enumerate()
            .map(|(i, v)| {
                vec![
                    i.to_string(),
                    v.to_string(),
                    format!("0x{:04x}", v.as_u64().unwrap_or(0)),
                ]
            })
            .collect();
        view! {<DataTable title="Constants" columns=vec!["Index","Value","Hex"] rows/>}.into_any()
    } else if let Some(colors) = data.get("colors").and_then(Value::as_array) {
        let rows = colors
            .iter()
            .enumerate()
            .map(|(i, v)| {
                vec![
                    i.to_string(),
                    v[0].to_string(),
                    v[1].to_string(),
                    v[2].to_string(),
                ]
            })
            .collect();
        view! {<DataTable title="Palette colors" columns=vec!["Index","Red","Green","Blue"] rows/>}
            .into_any()
    } else if let Some(slots) = data.get("slots").and_then(Value::as_array) {
        let rows = slots
            .iter()
            .enumerate()
            .map(|(i, v)| {
                vec![
                    i.to_string(),
                    text(v, "type_id"),
                    v["offset"].to_string(),
                    text(v, "standing"),
                    text(v, "sitting"),
                    text(v, "ground"),
                ]
            })
            .collect();
        view!{<DataTable title="Routing slots" columns=vec!["Index","Type","Offset bits","Standing","Sitting","Ground"] rows/>}.into_any()
    } else if data.get("sprite_package").is_some() {
        let ui = expect_context::<Ui>();
        view!{<h2>"Sprite authoring"</h2><p>"Export the source-bound sprite package to inspect or edit indexed, alpha, and depth planes. Import the edited package in the Edit tab."</p><button class="button secondary" on:click=move|_|{
            let result=ui.session.with_value(|s|s.sprite_json().and_then(|json|super::download("sprite-edit.json",json.as_bytes(),"application/json")));
            match result{Ok(())=>ui.status.set("Exported source-bound sprite package.".into()),Err(e)=>ui.error.set(Some(e))}
        }>"Export sprite package"</button>}.into_any()
    } else {
        let hex = data["preview_hex"].as_str().unwrap_or("");
        let lines = hex
            .as_bytes()
            .chunks(32)
            .enumerate()
            .map(|(i, line)| format!("{:08x}  {}", i * 16, String::from_utf8_lossy(line)))
            .collect::<Vec<_>>()
            .join("\n");
        let truncated = data["preview_truncated"].as_bool().unwrap_or(false);
        view!{<h2>"Resource bytes"</h2><pre class="byte-preview" tabindex="0">{lines}</pre>{truncated.then(||view!{<p class="quiet">"Preview shows the first 2,048 bytes. Export preserves the full resource."</p>})}}.into_any()
    };
    view! { {inspection.error.map(|e|view!{<div class="error-banner"><strong>"This payload needs attention"</strong><p>{e}</p></div>})} {body} }
}

pub fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default()
}
fn branch(value: u64) -> String {
    match value {
        253 => "253 · Alternate / error".into(),
        254 => "254 · Return true".into(),
        255 => "255 · Return false".into(),
        _ => value.to_string(),
    }
}

#[component]
fn DataTable(
    title: &'static str,
    columns: Vec<&'static str>,
    rows: Vec<Vec<String>>,
) -> impl IntoView {
    let page = RwSignal::new(0usize);
    let total = rows.len();
    let rows = StoredValue::new(rows);
    view! {
        <h2>{title}</h2>
        <div class="table-scroll" tabindex="0" role="region" aria-label=title><table><thead><tr>{columns.into_iter().map(|c|view!{<th scope="col">{c}</th>}).collect_view()}</tr></thead><tbody>
            {move||rows.with_value(|rows|rows.iter().skip(page.get()*100).take(100).map(|r|view!{<tr>{r.iter().cloned().map(|c|view!{<td>{c}</td>}).collect_view()}</tr>}).collect_view())}
        </tbody></table></div>
        <Show when=move||{total>100}><div class="table-pagination"><button class="button secondary" disabled=move||page.get()==0 on:click=move|_|page.update(|n|*n-=1)>"Previous rows"</button><span>{move||format!("{}–{} of {}",page.get()*100+1,((page.get()+1)*100).min(total),total)}</span><button class="button secondary" disabled=move||(page.get()+1)*100>=total on:click=move|_|page.update(|n|*n+=1)>"Next rows"</button></div></Show>
        { (total==0).then(||view!{<p class="quiet">"This resource has no entries."</p>}) }
    }
}

#[component]
fn ControlFlow(instructions: Vec<Value>) -> impl IntoView {
    let shown = instructions.len().min(32);
    let height = shown * 92 + 144;
    let exit_y = shown * 92 + 50;
    let mut edges = Vec::new();
    for (i, inst) in instructions.iter().take(shown).enumerate() {
        for (side, label, color) in [("true", "T", "#277b42"), ("false", "F", "#a74333")] {
            let destination = inst[side].as_u64().unwrap_or(255) as usize;
            let start_y = i * 92 + 48;
            let (end_x, end_y) = match destination {
                254 => (115, exit_y),
                253 => (340, exit_y),
                255 => (565, exit_y),
                p if p < shown => (340, p * 92 + 16),
                _ => continue,
            };
            let bend = if side == "true" {
                85 + (i % 3) * 15
            } else {
                570 + (i % 3) * 15
            };
            let start_x = if side == "true" { 230 } else { 450 };
            let path = format!(
                "M {start_x} {start_y} H {bend} V {} H {end_x} V {end_y}",
                end_y.saturating_sub(12)
            );
            edges.push(view!{<g><path d=path fill="none" stroke=color stroke-width="1.4" marker-end="url(#cfg-arrow)"/><text x=if side=="true"{"207"}else{"458"} y=(start_y-7).to_string() fill=color class="edge-label">{label}</text></g>});
        }
    }
    let nodes=instructions.into_iter().take(shown).enumerate().map(|(i,inst)|{
        let y=i*92+16;view!{<g><rect x="230" y=y.to_string() width="220" height="62" rx="7" class="cfg-node"/><text x="340" y=(y+24).to_string() text-anchor="middle" class="node-title">{format!("{} · Opcode 0x{:04x}",i,inst["opcode"].as_u64().unwrap_or(0))}</text><text x="340" y=(y+45).to_string() text-anchor="middle" class="node-operand">{text(&inst,"operand_hex")}</text></g>}
    }).collect_view();
    view! {
        <h2 class="flow-title">"Control flow"</h2>
        <div class="flow-scroll" tabindex="0"><svg class="control-flow" viewBox=format!("0 0 680 {height}") style=format!("min-height:{}px",height.min(440)) role="img" aria-label="Decoded BHAV branch graph. The instruction table contains all exact destinations.">
            <defs><marker id="cfg-arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M 0 0 L 7 3.5 L 0 7 z" fill="#566075"/></marker></defs>
            {edges.collect_view()}{nodes}
            {[(30,"Return true"),(255,"Alternate / error"),(480,"Return false")].into_iter().map(|(x,label)|view!{<g><rect x=x.to_string() y=exit_y.to_string() width="170" height="54" rx="7" class="cfg-exit"/><text x=(x+85).to_string() y=(exit_y+32).to_string() text-anchor="middle" class="exit-label">{label}</text></g>}).collect_view()}
        </svg></div>
        <p class="quiet">{if shown==32{"Graph preview is limited to 32 instructions. Use the table for all branch destinations."}else{"T and F show the stored true and false branch destinations."}}</p>
    }
}

#[component]
pub fn History() -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {
        <h2>"History"</h2><div class="history-actions">
            <button class="button secondary" disabled=move||{ui.revision.get();ui.session.with_value(|s|s.undo_len()==0)} on:click=move|_|{let mut result=Ok(false);ui.session.update_value(|s|result=s.undo());ui.report(result,"Restored previous document.");}>"Undo"</button>
            <button class="button secondary" disabled=move||{ui.revision.get();ui.session.with_value(|s|s.redo_len()==0)} on:click=move|_|{let mut result=Ok(false);ui.session.update_value(|s|result=s.redo());ui.report(result,"Restored next document.");}>"Redo"</button>
            <span class="quiet">{move||{ui.revision.get();ui.session.with_value(|s|format!("{} KiB retained",s.history_bytes().div_ceil(1024)))}}</span>
        </div>
        <p class="quiet">"Up to 64 changes are retained within a 16 MiB history budget. Opening a different file starts a new history."</p>
        <ol class="change-list">{move||{ui.revision.get();ui.session.with_value(|s|s.changes()).into_iter().map(|c|view!{<li><strong>{c.label}</strong><dl><dt>"Before"</dt><dd><code>{c.before_sha256}</code></dd><dt>"After"</dt><dd><code>{c.after_sha256}</code></dd></dl></li>}).collect_view()}}</ol>
        <Show when=move||{ui.revision.get();ui.session.with_value(|s|s.undo_len()==0)}><p>"No earlier changes are retained."</p></Show>
    }
}
