use crate::{
    bridge::{Overlay, Ui, selected_character},
    components::{Icon, portrait_path},
};
use leptos::prelude::*;
use wonderland_contracts::Need;

#[component]
pub fn NeedsGrid(#[prop(default = false)] all: bool) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let needs = if all {
        Need::ALL.to_vec()
    } else {
        vec![Need::Energy, Need::Hunger, Need::Fun, Need::Social]
    };
    view! { <div class="needs-grid">{needs.into_iter().map(|need| {
        let (label, icon) = match need {
            Need::Energy => ("Energy", "bolt"), Need::Hunger => ("Hunger", "tools-kitchen-2"),
            Need::Fun => ("Fun", "mood-smile"), Need::Social => ("Social", "users"),
            Need::Hygiene => ("Hygiene", "droplet"), Need::Bladder => ("Bladder", "bath"),
            Need::Comfort => ("Comfort", "armchair-2"), Need::Room => ("Room", "home"),
        };
        let value = move || selected_character(ui).and_then(|character| character.needs.get(&need).copied()).unwrap_or(0);
        view! { <div class="need"><Icon name=icon/><div class="need-content"><span class="need-label">{label}</span><div class="need-meter" role="meter" aria-label=label aria-valuemin="0" aria-valuemax="100" aria-valuenow=move || value().to_string() aria-valuetext=move || format!("{} out of 100", value())><span style:width=move || format!("{}%", value())></span></div></div></div> }
    }).collect_view()}</div> }
}

#[component]
pub fn Hud(#[prop(default = false)] lot: bool) -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {
        <aside class="character-hud" class:lot-hud=lot aria-label="Your Sim">
            <div class="hud-profile chrome">
                <div class="hud-portrait"><img src=move || selected_character(ui).map(|c| portrait_path(c.id.as_ref())).unwrap_or_default() alt=""/></div>
                <div class="hud-name-money"><strong>{move || selected_character(ui).map(|c| c.name).unwrap_or_default()}</strong><span class="money"><span class="currency-mark">"$"</span>{move || selected_character(ui).map(|c| format_money(c.money)).unwrap_or_default()}</span></div>
            </div>
            {lot.then(|| view! { <div class="needs-hud chrome"><NeedsGrid/><button id="all-needs" class="all-needs" aria-haspopup="dialog" on:click=move |_| ui.overlay.set(Overlay::Needs)>"All needs"<Icon name="chevron-up"/></button></div> })}
        </aside>
    }
}

fn format_money(value: i64) -> String {
    let mut result = String::new();
    let digits = value.unsigned_abs().to_string();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(character);
    }
    if value < 0 {
        result.insert(0, '-');
    }
    result
}
