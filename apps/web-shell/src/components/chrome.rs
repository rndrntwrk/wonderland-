use crate::{bridge::Ui, components::Icon};
use leptos::prelude::*;
use wonderland_contracts::UiIntent;

#[component]
pub fn SceneHeader(
    #[prop(into)] title: Signal<String>,
    #[prop(into)] icon: String,
    #[prop(into)] back_label: String,
) -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {
        <header class="scene-header chrome">
            <button class="header-back" aria-label=back_label on:click=move |_| ui.send(UiIntent::Back)><Icon name="chevron-left"/></button>
            <Icon name=icon/>
            <h1 id="screen-title" tabindex="-1">{move || title.get()}</h1>
        </header>
    }
}
