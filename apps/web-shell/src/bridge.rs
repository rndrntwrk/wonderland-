use std::{collections::BTreeMap, time::Duration};

use leptos::{
    leptos_dom::helpers::{TimeoutHandle, request_animation_frame, set_timeout_with_handle},
    prelude::*,
};
use wasm_bindgen::JsCast;
use wonderland_client_app::ShellState;
use wonderland_contracts::*;

use crate::{
    feedback::{ReplyFeedback, receive_reply},
    fixture::{self, Scenario},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    None,
    Settings,
    Needs,
}

#[derive(Clone, Copy)]
pub struct Ui {
    pub state: RwSignal<ShellState>,
    pub announcement: RwSignal<String>,
    pub notice: RwSignal<Option<String>>,
    pub overlay: RwSignal<Overlay>,
    pub reduced_motion: RwSignal<bool>,
    timers: RwSignal<BTreeMap<OperationId, TimeoutHandle>>,
    scenario: Scenario,
    rejected_once: RwSignal<bool>,
}

impl Ui {
    pub fn new(scenario: Scenario) -> Self {
        let mut state = ShellState::new(fixture::projection(scenario));
        if let Some(first) = state.projection.characters.first() {
            let id = first.id.clone();
            let _ = state.dispatch(UiIntent::SelectCharacter(id));
        }
        let reduced = web_sys::window()
            .and_then(|w| {
                w.match_media("(prefers-reduced-motion: reduce)")
                    .ok()
                    .flatten()
            })
            .is_some_and(|q| q.matches());
        let ui = Self {
            state: RwSignal::new(state),
            announcement: RwSignal::new(String::new()),
            notice: RwSignal::new(None),
            overlay: RwSignal::new(Overlay::None),
            reduced_motion: RwSignal::new(reduced),
            timers: RwSignal::new(BTreeMap::new()),
            scenario,
            rejected_once: RwSignal::new(false),
        };
        on_cleanup(move || {
            if let Some(timers) = ui.timers.try_get_untracked() {
                for timer in timers.values() {
                    timer.clear();
                }
            }
        });
        ui
    }

    pub fn send(self, intent: UiIntent) {
        self.notice.set(None);
        let before = self.state.with_untracked(|state| state.screen.clone());
        let mut requests = Ok(Vec::new());
        self.state.update(|state| requests = state.dispatch(intent));
        self.prune_timers();
        match requests {
            Ok(requests) => {
                for request in requests {
                    self.schedule(request);
                }
            }
            Err(error) => self.explain(error.to_string()),
        }
        let after = self.state.with_untracked(|state| state.screen.clone());
        if before != after {
            self.focus_screen();
        }
    }

    fn schedule(self, request: UiRequest) {
        let reject = !self.rejected_once.get_untracked()
            && fixture::should_reject(self.scenario, &request.kind);
        if reject {
            self.rejected_once.set(true);
        }
        let pending_message = match &request.kind {
            RequestKind::Travel { .. } => self.state.with_untracked(|state| {
                crate::authoring_adapter::travel_message(state, &request, true)
            }),
            RequestKind::Interaction { .. } => "Action requested. Waiting for a reply…".into(),
            RequestKind::Cancellation { .. } => "Cancelling action…".into(),
        };
        self.announcement.set(pending_message);
        let id = request.operation_id.clone();
        let callback_id = id.clone();
        let timer = set_timeout_with_handle(
            move || {
                let active = self
                    .state
                    .try_with_untracked(|state| {
                        state
                            .pending_requests
                            .get(&callback_id)
                            .is_some_and(|pending| pending.request == request)
                    })
                    .unwrap_or(false);
                let _ = self.timers.try_update(|timers| {
                    timers.remove(&callback_id);
                });
                if !active {
                    return;
                }
                let before = self.state.with_untracked(|state| state.screen.clone());
                let mut feedback = ReplyFeedback::None;
                self.state.update(|state| {
                    feedback = receive_reply(state, &request, fixture::reply(&request, reject));
                });
                match feedback {
                    ReplyFeedback::Error(error) => self.explain(error.to_string()),
                    ReplyFeedback::Announcement(message) => self.announcement.set(message.into()),
                    ReplyFeedback::TravelAnnouncement(message) => self.announcement.set(message),
                    ReplyFeedback::None => {}
                }
                if before != self.state.with_untracked(|state| state.screen.clone()) {
                    self.focus_screen();
                }
            },
            Duration::from_millis(850),
        );
        match timer {
            Ok(handle) => self.timers.update(|timers| {
                timers.insert(id, handle);
            }),
            Err(_) => {
                self.state.update(|state| {
                    let _ = state.receive(UiEvent::Rejected {
                        operation_id: id,
                        projection_revision: state.projection.revision,
                        reason: "The preview could not start this request. Please try again."
                            .into(),
                    });
                });
                self.explain("The preview could not start this request. Please try again.");
            }
        }
    }

    fn prune_timers(self) {
        let pending = self
            .state
            .with_untracked(|state| state.pending_requests.clone());
        self.timers.update(|timers| {
            timers.retain(|id, timer| {
                if pending.contains_key(id) {
                    true
                } else {
                    timer.clear();
                    false
                }
            })
        });
    }

    pub fn explain(self, reason: impl Into<String>) {
        let reason = reason.into();
        self.announcement.set(reason.clone());
        self.notice.set(Some(reason));
    }

    pub fn retry_characters(self) {
        let mut projection = fixture::projection(Scenario::Accept);
        projection.revision = self
            .state
            .with_untracked(|state| state.projection.revision + 1);
        self.state.update(|state| {
            let _ = state.receive(UiEvent::ProjectionUpdated { projection });
        });
        self.prune_timers();
        if let Some(id) = self
            .state
            .with_untracked(|state| state.projection.characters.first().map(|c| c.id.clone()))
        {
            self.send(UiIntent::SelectCharacter(id));
        }
        self.focus_screen();
    }

    pub fn dismiss_object(self) {
        let id = self.state.with_untracked(|state| {
            state
                .selected_object
                .as_ref()
                .map(|target| format!("object-{}", target.id))
        });
        self.send(UiIntent::DismissObject);
        if let Some(id) = id {
            focus_later(id);
        }
    }

    pub fn close_overlay(self) {
        let target = match self.overlay.get_untracked() {
            Overlay::Needs => "all-needs",
            _ => "settings",
        };
        self.overlay.set(Overlay::None);
        focus_later(target.into());
    }

    fn focus_screen(self) {
        let target = self
            .state
            .with_untracked(|state| match &state.screen {
                Screen::CharacterSelection => state
                    .selected_character
                    .as_ref()
                    .map(|id| format!("avatar-{id}")),
                Screen::City => state
                    .selected_place
                    .as_ref()
                    .map(|id| format!("place-{id}")),
                Screen::Lot { .. } => None,
            })
            .unwrap_or_else(|| "screen-title".into());
        focus_later(target);
    }
}

pub fn focus_later(id: String) {
    request_animation_frame(move || {
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    });
}

pub fn selected_character(ui: Ui) -> Option<Character> {
    ui.state.with(|state| {
        state
            .projection
            .characters
            .iter()
            .find(|character| Some(&character.id) == state.selected_character.as_ref())
            .cloned()
    })
}
