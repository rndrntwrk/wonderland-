//! Disposable local-preview session; only accepted receipts mirror into the original shell.
use crate::{
    authoring_adapter::{Delivery, deliver, preview_reply},
    bridge::{Ui, focus_later},
    persistence,
};
use leptos::{
    leptos_dom::helpers::{TimeoutHandle, set_timeout_with_handle},
    prelude::*,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use wonderland_client_app::authoring::*;
use wonderland_contracts::authoring::*;
use wonderland_contracts::{Availability, CharacterId, UiEvent, UiIntent};
#[derive(Clone, Copy)]
pub struct AuthorUi {
    pub state: RwSignal<AuthoringState>,
    provider: StoredValue<PreviewAuthoringProvider>,
    timer: RwSignal<Option<TimeoutHandle>>,
    alive: StoredValue<Arc<AtomicBool>>,
    reject_once: RwSignal<bool>,
    reject_fixture: bool,
    pub writable: RwSignal<bool>,
    pub storage_notice: RwSignal<String>,
    pub scenario: bool,
}
impl AuthorUi {
    pub fn new(ui: Ui, fixture: Option<&str>) -> Self {
        let scenario = fixture.is_some();
        let mut projection = preview_authoring_projection();
        let mut writable = !scenario;
        let mut storage_notice = String::new();
        if !scenario {
            match persistence::load() {
                Ok(Some(saved)) => projection = saved,
                Ok(None) => {}
                Err(error) => {
                    writable = false;
                    storage_notice =
                        format!("{error} Temporary preview; existing data is preserved.");
                }
            }
        }
        match fixture {
            Some("read-only-home") => {
                for profile in &mut projection.profiles {
                    let unavailable = Availability::Unavailable {
                        reason: "This Home is read only in this preview.".into(),
                    };
                    profile.home.permissions.purchase = unavailable.clone();
                    profile.home.permissions.arrange = unavailable;
                }
            }
            Some("poor-home") => {
                for p in &mut projection.profiles {
                    p.character.money = 20;
                }
            }
            Some("empty-catalog") => projection.catalog.clear(),
            _ => {}
        }
        let preserve = matches!(fixture, Some("empty-characters" | "unavailable-characters"));
        if !preserve {
            ui.state.update(|shell| {
                if let Ok(projection) =
                    preview_project_authoring_to_ui(&shell.projection, &projection)
                {
                    let _ = shell.receive(UiEvent::ProjectionUpdated { projection });
                }
            });
            if let Some(id) = projection.profiles.first().map(|p| p.character.id.clone()) {
                ui.send(UiIntent::SelectCharacter(id));
            }
        }
        let author = Self {
            state: RwSignal::new(AuthoringState::new(projection.clone())),
            provider: StoredValue::new(
                PreviewAuthoringProvider::new(projection).expect("validated preview"),
            ),
            timer: RwSignal::new(None),
            alive: StoredValue::new(Arc::new(AtomicBool::new(true))),
            reject_once: RwSignal::new(false),
            reject_fixture: fixture == Some("reject-authoring"),
            writable: RwSignal::new(writable),
            storage_notice: RwSignal::new(storage_notice),
            scenario,
        };
        let alive = author.alive.get_value();
        on_cleanup(move || {
            alive.store(false, Ordering::Release);
            if let Some(Some(timer)) = author.timer.try_get_untracked() {
                timer.clear();
            }
        });
        author
    }
    pub fn busy(self) -> bool {
        self.state.with(|s| s.pending().is_some())
    }
    pub fn select(self, id: CharacterId) {
        self.send(AuthoringIntent::SelectProfile(id));
    }
    pub fn send(self, intent: AuthoringIntent) {
        let ui = expect_context::<Ui>();
        let mut requests = Ok(Vec::new());
        self.state.update(|s| requests = s.dispatch(intent));
        match requests {
            Err(error) => ui.explain(error.to_string()),
            Ok(requests) => {
                for request in requests {
                    ui.notice.set(None);
                    ui.announcement.set("Saving preview change…".into());
                    let reject = self.reject_fixture && !self.reject_once.get_untracked();
                    if reject {
                        self.reject_once.set(true);
                    }
                    let alive = self.alive.get_value();
                    let expected = request.clone();
                    let timer = set_timeout_with_handle(
                        move || {
                            if !alive.load(Ordering::Acquire) {
                                return;
                            }
                            self.timer.set(None);
                            if !self
                                .state
                                .with_untracked(|s| s.pending() == Some(&expected))
                            {
                                return;
                            }
                            let mut event = None;
                            self.provider.update_value(|provider| {
                                event = Some(preview_reply(provider, &expected, reject))
                            });
                            let event = event.expect("preview reply");
                            let mut result = Ok(Delivery::Ignored);
                            self.state.update(|s| result = deliver(s, &expected, event));
                            match result {
                                Ok(Delivery::Committed(commit)) => {
                                    let snapshot =
                                        self.state.with_untracked(|s| s.projection().clone());
                                    let projection = ui.state.with_untracked(|s| {
                                        preview_project_authoring_to_ui(&s.projection, &snapshot)
                                    });
                                    match projection {
                                        Ok(projection) => {
                                            ui.state.update(|s| {
                                                let _ = s.receive(UiEvent::ProjectionUpdated {
                                                    projection,
                                                });
                                            });
                                            let selected = self
                                                .state
                                                .with_untracked(|s| s.selected_profile().cloned());
                                            if let Some(id)=selected && ui.state.with_untracked(|s|s.screen==wonderland_contracts::Screen::CharacterSelection) {ui.send(UiIntent::SelectCharacter(id));}
                                            if self.writable.get_untracked()
                                                && let Err(error) = persistence::save(&snapshot)
                                            {
                                                self.storage_notice.set(error);
                                            }
                                            ui.announcement.set("Preview change saved.".into());
                                            match commit.outcome {
                                                AuthoringOutcome::ProfileCreated {
                                                    character_id,
                                                }
                                                | AuthoringOutcome::OutfitSaved { character_id } => {
                                                    focus_later(format!("avatar-{character_id}"))
                                                }
                                                AuthoringOutcome::Purchased {
                                                    instance_id, ..
                                                }
                                                | AuthoringOutcome::Moved { instance_id, .. }
                                                | AuthoringOutcome::Placed {
                                                    instance_id, ..
                                                } => focus_later(format!(
                                                    "home-object-{instance_id}"
                                                )),
                                                _ => focus_later("home-inventory".into()),
                                            }
                                        }
                                        Err(error) => ui.explain(error.to_string()),
                                    }
                                }
                                Ok(Delivery::Rejected(error)) | Err(error) => {
                                    ui.explain(error.to_string())
                                }
                                Ok(Delivery::Ignored) => {}
                            }
                        },
                        Duration::from_millis(850),
                    );
                    match timer {
                        Ok(timer) => self.timer.set(Some(timer)),
                        Err(_) => {
                            let event = AuthoringEvent::Rejected {
                                operation_id: request.operation_id,
                                base_revision: request.base_revision,
                                error: AuthoringError::Rejected(
                                    "Preview timer unavailable. Please retry.".into(),
                                ),
                            };
                            self.state.update(|s| {
                                let _ = s.receive(event);
                            });
                            ui.explain("Preview timer unavailable. Please retry.");
                        }
                    }
                }
            }
        }
    }
    pub fn path(self, id: &CharacterId) -> String {
        self.state.with(|s| {
            s.projection()
                .profile(id)
                .map(|p| look_path(p.identity, &p.look_id))
                .unwrap_or_else(|| crate::components::portrait_path(id.as_ref()))
        })
    }
}
pub fn look_path(identity: VisualIdentity, look: &LookId) -> String {
    match look.style_for(identity).unwrap_or(LookStyle::Everyday) {
        LookStyle::Everyday => format!("/assets/art/{}.png", identity.as_str()),
        style => format!(
            "/assets/authoring/{}-{}.png",
            identity.as_str(),
            style.as_str()
        ),
    }
}
