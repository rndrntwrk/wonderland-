//! Account-owned creator reuses the original appearance stage and thumbnails.
use crate::{
    avatar_content::{AvatarStage, ContentUi, OriginalThumbnail},
    native_creation::CreationCatalog,
};
use leptos::prelude::*;
use wonderland_contracts::authoring::{AppearanceContent, AppearanceSelection};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Head,
    Body,
    Skin,
    Gender,
}
impl Part {
    fn label(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Body => "Body",
            Self::Skin => "Skin tone",
            Self::Gender => "Gender",
        }
    }
}
#[component]
fn PartChoices(
    part: Part,
    choices: Memo<Option<AppearanceContent>>,
    appearance: RwSignal<AppearanceSelection>,
    blocked: Signal<bool>,
) -> impl IntoView {
    let page = RwSignal::new(0usize);
    let rows = Memo::new(move |_| {
        choices.with(|v| {
            v.as_ref()
                .map(|v| match part {
                    Part::Head => v.heads.clone(),
                    Part::Body => v.bodies.clone(),
                    Part::Skin => v.skin_tones.clone(),
                    Part::Gender => v.genders.clone(),
                })
                .unwrap_or_default()
        })
    });
    Effect::new(move |_| page.update(|p| *p = (*p).min(rows.get().len().saturating_sub(1) / 18)));
    view! {<fieldset class="appearance-options" class:part-grid=matches!(part,Part::Head|Part::Body)><legend>{part.label()}</legend><div class="appearance-grid">
    <For each=move ||{rows.get().into_iter().skip(page.get()*18).take(18).collect::<Vec<_>>()} key=|v|v.key.clone() children=move |v|{
    let key=v.key.clone();let selected=key.clone();let image=key.clone();let option=v.clone();
    view!{<button type="button" class="chrome look-card" aria-pressed=move ||appearance.with(|a|match part{Part::Head=>a.head.as_ref(),Part::Body=>a.body.as_ref(),Part::Skin=>a.skin_tone.as_ref(),Part::Gender=>a.gender.as_ref()}==Some(&selected)).to_string()
    disabled=move ||blocked.get()||!option.availability.is_available()||(matches!(part,Part::Head|Part::Body)&&!option.compatible(&appearance.get()))
    on:click=move |_|appearance.update(|a|{
    match part{Part::Head=>a.head=Some(key.clone()),Part::Body=>a.body=Some(key.clone()),Part::Skin=>a.skin_tone=Some(key.clone()),Part::Gender=>a.gender=Some(key.clone())}
    if let Some(c)=choices.get_untracked(){if matches!(part,Part::Skin|Part::Gender){
     if !c.heads.iter().any(|v|Some(&v.key)==a.head.as_ref()&&v.compatible(a)){a.head=c.heads.iter().find(|v|v.availability.is_available()&&v.compatible(a)).map(|v|v.key.clone());}
     if !c.bodies.iter().any(|v|Some(&v.key)==a.body.as_ref()&&v.compatible(a)){a.body=c.bodies.iter().find(|v|v.availability.is_available()&&v.compatible(a)).map(|v|v.key.clone());}
    }}
    })><Show when=move ||matches!(part,Part::Head|Part::Body)><OriginalThumbnail content_key=image.clone() skin=Signal::derive(move ||appearance.with(|a|a.skin_tone.clone()))/></Show><span>{v.label}</span></button>}
    }/></div>
    <Show when=move ||{rows.get().len()>18}><nav class="choice-pages" aria-label=format!("{} pages",part.label())><button type="button" class="chrome" disabled=move ||page.get()==0 on:click=move |_|page.update(|p|*p=p.saturating_sub(1))>"Previous"</button><span>{move ||format!("{} / {}",page.get()+1,rows.get().len().div_ceil(18))}</span><button type="button" class="chrome" disabled=move ||{(page.get()+1)*18>=rows.get().len()} on:click=move |_|page.update(|p|*p+=1)>"Next"</button></nav></Show>
    </fieldset>}
}
#[component]
pub fn NativeCreator(
    catalog: Signal<Option<CreationCatalog>>,
    busy: Signal<bool>,
    pending: Signal<bool>,
    unknown: Signal<bool>,
    submit: Callback<String>,
    close: Callback<()>,
    retry: Callback<()>,
    reload: Callback<()>,
) -> impl IntoView {
    let content = expect_context::<ContentUi>();
    let appearance = RwSignal::new(AppearanceSelection::default());
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let part = RwSignal::new(Part::Head);
    let choices = Memo::new(move |_| {
        content
            .choices
            .get()
            .zip(catalog.get())
            .map(|(source, c)| c.choices(&source))
    });
    Effect::new(move |_| {
        if let Some(c) = choices.get() {
            appearance.set(c.default_selection())
        }
    });
    let blocked = Signal::derive(move || busy.get() || pending.get());
    let validation = move || -> Result<String, String> {
        if content.loading.get() {
            return Err("Loading original character content…".into());
        }
        let c = catalog.get().ok_or("Character catalog is unavailable.")?;
        let source = content
            .choices
            .get()
            .ok_or("Character content is not ready. Retry loading it.")?;
        let spec = c.specification(&source, &appearance.get(), &name.get(), &description.get())?;
        serde_json::to_string(&spec)
            .map_err(|_| "Character specification could not be prepared.".into())
    };
    view! {<section class="avatars-screen creator-screen connected-creator" aria-labelledby="native-create-title">
    <h1 id="native-create-title" class="choose-title">"Create a Sim"</h1>
    <div class="creator-stage"><img class="stage-diamond" src="/assets/art/selection-diamond.png" alt=""/><AvatarStage appearance=Signal::derive(move ||appearance.get())/></div>
    <div class="creator-choices">
    <p role="status">{move ||content.notice.get()}</p><button type="button" class="chrome" disabled=move ||blocked.get()||content.loading.get() on:click=move |_|reload.run(())>"Retry content loading"</button>
    <nav class="creator-part-tabs" aria-label="Character parts"><button type="button" class="chrome" aria-pressed=move ||(part.get()==Part::Head).to_string() on:click=move |_|part.set(Part::Head)>"Head"</button><button type="button" class="chrome" aria-pressed=move ||(part.get()==Part::Body).to_string() on:click=move |_|part.set(Part::Body)>"Body"</button></nav>
    <Show when=move ||part.get()==Part::Head fallback=move ||view!{<PartChoices part=Part::Body choices appearance blocked/>}><PartChoices part=Part::Head choices appearance blocked/></Show>
    <div class="creator-traits"><PartChoices part=Part::Skin choices appearance blocked/><PartChoices part=Part::Gender choices appearance blocked/></div>
    <label class="chrome nameplate"><span>"Name"</span><input id="native-create-name" maxlength="24" autocomplete="off" prop:value=move ||name.get() disabled=move ||blocked.get() on:input=move|e|name.set(event_target_value(&e))/></label>
    <details class="creator-description"><summary>"Description"</summary><label class="chrome nameplate"><span>"About your Sim"</span><textarea id="native-create-description" maxlength="499" prop:value=move ||description.get() disabled=move ||blocked.get() on:input=move|e|description.set(event_target_value(&e))/></label></details>
    <p class="creator-validity" role="status">{move ||validation().err().unwrap_or_default()}</p>
    <Show when=move ||unknown.get()><p role="status">"Creation is unconfirmed. Retry the original request before editing or creating another Sim."</p><button type="button" class="chrome primary" disabled=move ||busy.get() on:click=move |_|retry.run(())>"Retry creating this Sim"</button></Show>
    <div class="creator-actions"><button type="button" class="chrome" disabled=move ||blocked.get() on:click=move |_|close.run(())>"Back to Sims"</button><button type="button" class="chrome primary" disabled=move ||blocked.get()||validation().is_err() on:click=move |_|if let Ok(spec)=validation(){submit.run(spec)}>"Create Sim"</button></div>
    </div></section>}
}
