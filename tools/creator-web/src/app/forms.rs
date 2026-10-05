// SPDX-License-Identifier: MPL-2.0
use super::{components::text, Ui};
use crate::session::Inspection;
use leptos::prelude::*;
use wonderland_creator::{decode_hex, Edit, ResourceGuard, ResourceOperation, ResourceTransaction};
use wonderland_legacy_formats::iff::{ChunkKey, IffChunk};

#[component]
fn Field(
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(default = 256)] max: usize,
) -> impl IntoView {
    view! {<label class="field"><span>{label}</span><input type=kind maxlength=max.to_string() prop:value=move||value.get() on:input=move|ev|value.set(event_target_value(&ev))/></label>}
}
#[component]
fn TextArea(
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = 65536)] max: usize,
) -> impl IntoView {
    view! {<label class="field wide"><span>{label}</span><textarea rows="5" maxlength=max.to_string() spellcheck="false" prop:value=move||value.get() on:input=move|ev|value.set(event_target_value(&ev))></textarea></label>}
}
fn number<T: std::str::FromStr>(value: &str, label: &str) -> Result<T, String> {
    value
        .parse::<T>()
        .map_err(|_| format!("{label} must be a valid number in its supported range."))
}
fn raw_bytes(value: &str) -> Result<Vec<u8>, String> {
    if value.len() > 128 * 1024 {
        return Err("Raw edit exceeds 64 KiB.".into());
    }
    // Reject non-hex text without accepting Unicode look-alike digits.
    let actual: String = value.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if !actual.len().is_multiple_of(2) || !actual.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(
            "Enter pairs of hexadecimal digits, optionally separated by whitespace.".into(),
        );
    }
    actual
        .as_bytes()
        .chunks_exact(2)
        .map(|p| {
            u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16)
                .map_err(|_| "Invalid hexadecimal byte.".into())
        })
        .collect()
}

#[component]
pub fn EditPanel(inspection: Inspection) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let key = inspection.row.key;
    let guard = inspection.guard.clone();
    let data = StoredValue::new(inspection.details.clone());
    let index = RwSignal::new("0".to_owned());
    let set = RwSignal::new("0".to_owned());
    let value = RwSignal::new(String::new());
    let second = RwSignal::new(String::new());
    let third = RwSignal::new(String::new());
    let operand = RwSignal::new(String::new());
    let mode = RwSignal::new("branch".to_owned());
    Effect::new(move |_| {
        let Ok(i) = index.get().parse::<usize>() else {
            return;
        };
        let language = set.get().parse::<usize>().unwrap_or(0);
        data.with_value(|d| match &key.kind {
            b"BHAV" => {
                if let Some(row) = d["instructions"].get(i) {
                    value.set(text(row, "true"));
                    second.set(text(row, "false"));
                    operand.set(text(row, "operand_hex"));
                }
            }
            b"BCON" => {
                if let Some(v) = d["constants"].get(i) {
                    value.set(v.to_string());
                }
            }
            b"STR#" | b"CTSS" | b"TTAs" => {
                if let Some(v) = d["sets"].get(language).and_then(|s| s.get(i)) {
                    value.set(text(v, "value"));
                }
            }
            b"PALT" => {
                if let Some(v) = d["colors"].get(i) {
                    value.set(v[0].to_string());
                    second.set(v[1].to_string());
                    third.set(v[2].to_string());
                }
            }
            b"SLOT" => {
                if let Some(v) = d["slots"].get(i) {
                    let values = v["offset"].as_array();
                    if let Some(values) = values {
                        let floats: Vec<_> = values
                            .iter()
                            .map(|v| f32::from_bits(v.as_u64().unwrap_or(0) as u32).to_string())
                            .collect();
                        if floats.len() == 3 {
                            value.set(floats[0].clone());
                            second.set(floats[1].clone());
                            third.set(floats[2].clone());
                        }
                    }
                }
            }
            _ => value.set(if d["preview_truncated"].as_bool().unwrap_or(false) {
                String::new()
            } else {
                d["preview_hex"].as_str().unwrap_or("").to_owned()
            }),
        });
    });
    let supported = matches!(
        &key.kind,
        b"BHAV" | b"STR#" | b"CTSS" | b"TTAs" | b"BCON" | b"SLOT" | b"PALT"
    );
    let body = if inspection.error.is_some() {
        view!{<p>"This resource has a decoding or validation error. Inspect its bytes and resolve the reported issue before using typed editing."</p>}.into_any()
    } else if key.kind == *b"SPR2" {
        let json = RwSignal::new(String::new());
        view!{<h2>"Import sprite edit"</h2><p>"Paste a source-bound sprite package exported from this document. Its source and resource hashes must still match."</p><form on:submit=move|ev|{ev.prevent_default();let payload=json.get();let mut result=Err("document unavailable".into());ui.session.update_value(|s|result=s.apply_sprite_json(payload.as_bytes()));ui.report(result,"Sprite edit applied.");}><TextArea label="Sprite package JSON" value=json max=1024*1024/><button class="button primary" type="submit">"Apply sprite edit"</button></form>}.into_any()
    } else {
        let inputs=match &key.kind{
            b"BHAV"=>view!{
                <label class="field"><span>"Edit operation"</span><select prop:value=move||mode.get() on:change=move|ev|mode.set(event_target_value(&ev))><option value="branch">"Branch destinations"</option><option value="operand">"Operand bytes"</option></select></label>
                <Field label="Instruction index" value=index kind="number"/>
                {move||if mode.get()=="branch"{view!{<div class="field-row"><Field label="True destination" value=value kind="number"/><Field label="False destination" value=second kind="number"/></div><p class="quiet">"253 = alternate/error; 254 = return true; 255 = return false. Other values name instruction indices."</p>}.into_any()}else{view!{<Field label="Operand (16 hex digits)" value=operand max=16/>}.into_any()}}
            }.into_any(),
            b"STR#"|b"CTSS"|b"TTAs"=>view!{<div class="field-row"><Field label="String set index" value=set kind="number"/><Field label="String index" value=index kind="number"/></div><TextArea label="String value" value=value/>}.into_any(),
            b"BCON"=>view!{<div class="field-row"><Field label="Constant index" value=index kind="number"/><Field label="Value (0–65535)" value=value kind="number"/></div>}.into_any(),
            b"SLOT"=>view!{<Field label="Slot index" value=index kind="number"/><div class="field-row"><Field label="X offset" value=value/><Field label="Y offset" value=second/><Field label="Z offset" value=third/></div><p class="quiet">"Offsets use the original resource units. Finite values only."</p>}.into_any(),
            b"PALT"=>view!{<Field label="Palette index" value=index kind="number"/><div class="field-row"><Field label="Red (0–255)" value=value kind="number"/><Field label="Green (0–255)" value=second kind="number"/><Field label="Blue (0–255)" value=third kind="number"/></div>}.into_any(),
            _=>view!{<TextArea label="Resource bytes (hex)" value=value max=128*1024/><p class="quiet">"Raw replacement is available for unrecognized resource types. Maximum typed replacement: 64 KiB. A truncated preview is not a complete replacement."</p>}.into_any(),
        };
        let truncated = inspection.details["preview_truncated"]
            .as_bool()
            .unwrap_or(false);
        if truncated && !supported {
            value.set(String::new());
        }
        view!{
            <h2>"Edit resource"</h2><p class="quiet">"Changes are checked against this document and resource revision before publication."</p>
            <form on:submit=move|ev|{
                ev.prevent_default();
                let result=(||{
                    let i=number::<usize>(&index.get(),"Index")?;
                    Ok::<_,String>(match &key.kind{
                        b"BHAV"=>if mode.get()=="branch"{Edit::BhavBranch{instruction:i,true_pointer:number(&value.get(),"True destination")?,false_pointer:number(&second.get(),"False destination")?}}else{Edit::BhavOperand{instruction:i,operand:decode_hex(&operand.get(),"operand")?}},
                        b"STR#"|b"CTSS"|b"TTAs"=>Edit::StringValue{set:number(&set.get(),"String set")?,index:i,value:value.get()},
                        b"BCON"=>Edit::TuningConstant{index:i,value:number(&value.get(),"Constant")?},
                        b"SLOT"=>{let offset=[number::<f32>(&value.get(),"X offset")?,number::<f32>(&second.get(),"Y offset")?,number::<f32>(&third.get(),"Z offset")?];if !offset.iter().all(|v|v.is_finite()){return Err("Offsets must be finite.".into());}Edit::SlotOffset{index:i,offset}},
                        b"PALT"=>Edit::PaletteColor{index:i,rgb:[number(&value.get(),"Red")?,number(&second.get(),"Green")?,number(&third.get(),"Blue")?]},
                        _=>{let input=value.get();if truncated&&input.is_empty(){return Err("Supply the complete replacement bytes; this resource preview was truncated.".into());}Edit::UnknownBytes(raw_bytes(&input)?)},
                    })
                })();
                match result{Ok(edit)=>ui.apply(key,&guard,edit,"Edit resource"),Err(error)=>ui.error.set(Some(error))}
            }>{inputs}<button class="button primary" type="submit">"Apply edit"</button></form>
        }.into_any()
    };
    view! { {body}<MetadataForm inspection/><AddResource/> }
}

#[component]
fn MetadataForm(inspection: Inspection) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let key = inspection.row.key;
    let id = RwSignal::new(key.id.to_string());
    let flags = RwSignal::new(inspection.row.flags.to_string());
    let label = RwSignal::new(inspection.row.label_hex.clone());
    let guard = inspection.guard.clone();
    let remove_guard = inspection.guard;
    view! {<details class="metadata-edit"><summary>"Resource metadata"</summary><form on:submit=move|ev|{
        ev.prevent_default();
        let result=(||{Ok::<_,String>(ResourceTransaction{source_hash:guard.source_hash.clone(),operations:vec![ResourceOperation::SetMetadata{key,expected:ResourceGuard::from(&guard),new_key:ChunkKey{kind:key.kind,id:number(&id.get(),"Resource ID")?},flags:number(&flags.get(),"Flags")?,label:decode_hex(&label.get(),"resource label")?}]})})();
        match result{Ok(transaction)=>ui.transact(&transaction),Err(e)=>ui.error.set(Some(e))}
    }><div class="field-row"><Field label="Resource ID" value=id kind="number"/><Field label="Resource flags" value=flags kind="number"/></div><TextArea label="Label bytes (128 hex digits)" value=label max=128/><p class="quiet">"Label bytes preserve the source encoding. Resource IDs must remain unique within their type."</p><button class="button secondary" type="submit">"Apply metadata"</button></form>
        <button class="button danger" type="button" on:click=move|_|ui.transact(&ResourceTransaction{source_hash:remove_guard.source_hash.clone(),operations:vec![ResourceOperation::Remove{key,expected:ResourceGuard::from(&remove_guard)}]})>"Remove resource"</button><p class="quiet">"You can restore a removed resource from History."</p>
    </details>}
}

#[component]
pub(super) fn AddResource() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let kind = RwSignal::new(String::new());
    let id = RwSignal::new("0".into());
    let payload = RwSignal::new(String::new());
    let source = ui.session.with_value(|s| s.source_sha256().to_owned());
    view! {<details class="metadata-edit"><summary>"Add a resource"</summary><form on:submit=move|ev|{
        ev.prevent_default();
        let result=(||{let k=kind.get();if k.len()!=4||!k.bytes().all(|b|b.is_ascii_graphic()){return Err("Resource type must contain four printable ASCII bytes.".into());}let mut kind=[0;4];kind.copy_from_slice(k.as_bytes());Ok(ResourceTransaction{source_hash:source.clone(),operations:vec![ResourceOperation::Add{chunk:IffChunk{key:ChunkKey{kind,id:number(&id.get(),"New resource ID")?},flags:0,label:[0;64],data:raw_bytes(&payload.get())?}}]})})();
        match result{Ok(transaction)=>ui.transact(&transaction),Err(e)=>ui.error.set(Some(e))}
    }><div class="field-row"><Field label="New resource type" value=kind max=4/><Field label="New resource ID" value=id kind="number"/></div><TextArea label="New resource payload (hex)" value=payload max=128*1024/><button class="button secondary" type="submit">"Add resource"</button></form></details>}
}
