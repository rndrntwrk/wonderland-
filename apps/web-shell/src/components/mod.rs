pub mod actions;
pub mod chrome;
pub mod hud;
pub mod queue;
pub mod scene;

use leptos::prelude::*;

#[component]
pub fn Icon(
    #[prop(into)] name: Signal<String>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    view! { <span class=format!("icon {class}") aria-hidden="true" style=move || format!("--icon: url('/assets/icons/{}.svg')", name.get())></span> }
}

pub fn portrait_path(id: &str) -> String {
    match id {
        "maya" | "jules" | "nico" | "amara" | "leo" => format!("/assets/art/{id}.png"),
        _ => "/assets/art/maya.png".into(),
    }
}

pub fn availability_reason(availability: &wonderland_contracts::Availability) -> Option<String> {
    match availability {
        wonderland_contracts::Availability::Unavailable { reason } => Some(reason.clone()),
        wonderland_contracts::Availability::Available => None,
    }
}
