use crate::{bridge::Ui, components::Icon};
use leptos::prelude::*;
use wonderland_contracts::*;

#[component]
pub fn ActionQueue() -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {
        <aside class="action-queue" aria-label="Action queue">
            <For each=move || ui.state.with(|state| state.pending_requests.values().filter(|pending| matches!(pending.request.kind, RequestKind::Interaction { .. })).map(|pending| pending.request.clone()).collect::<Vec<_>>()) key=|request| request.operation_id.clone() children=move |request| {
                let (label, icon) = ui.state.with_untracked(|state| match &request.kind {
                    RequestKind::Interaction { target, action_id, .. } => (state.projection.objects.iter().find(|object| &object.target == target).and_then(|object| object.offers.iter().find(|offer| &offer.id == action_id)).map(|offer| offer.label.clone()).unwrap_or_else(|| "Action".into()), if action_id.as_ref() == "inspect" { "search" } else { "coffee" }),
                    _ => ("Action".into(), "coffee"),
                });
                view! { <div class="queue-item queue-pending"><div class="queue-orb chrome"><Icon name=icon/><span class="queue-progress"><Icon name="loader-2"/></span></div><strong>{label}</strong><span class="queue-status">"Requested…"</span></div> }
            }/>
            <For each=move || ui.state.with(|state| state.queue.clone()) key=|queued| queued.operation_id.clone() children=move |queued| {
                let id = queued.operation_id.clone(); let status_id = id.clone();
                let cancelling = Memo::new(move |_| ui.state.with(|state| state.queue.iter().find(|queued| queued.operation_id == status_id).is_some_and(|queued| queued.status == QueueStatus::CancellationPending)));
                let label = queued.label.clone();
                let icon = if queued.action_id.as_ref() == "inspect" { "search" } else { "coffee" };
                view! {
                    <div class="queue-item" class:queue-cancelling=move || cancelling.get()>
                        <div class="queue-orb chrome"><Icon name=icon/><button class="queue-cancel chrome" aria-label=format!("Cancel {label}") disabled=move || cancelling.get() on:click=move |_| ui.send(UiIntent::Cancel { operation_id: id.clone() })><Icon name="x"/></button></div>
                        <strong>{queued.label}</strong><span class="queue-status">{move || if cancelling.get() { "Cancelling…" } else { "Accepted" }}</span>
                    </div>
                }
            }/>
        </aside>
    }
}
