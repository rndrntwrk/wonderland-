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
        Arc, Mutex,
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
    persistence: StoredValue<Arc<Mutex<persistence::SaveSession>>>,
    persisting: RwSignal<bool>,
    pub storage_notice: RwSignal<String>,
    pub scenario: bool,
}
impl AuthorUi {
    pub fn new(ui: Ui, fixture: Option<&str>) -> Self {
        let scenario = fixture.is_some();
        let mut projection = preview_authoring_projection();
        let mut save_session = persistence::SaveSession::temporary("");
        if !scenario {
            match persistence::load() {
                Ok((saved, session)) => {
                    if let Some(saved) = saved {
                        projection = resume_preview_projection(saved);
                    }
                    save_session = session;
                }
                Err(error) => {
                    save_session = persistence::SaveSession::temporary(format!(
                        "{error} Temporary preview; existing data is preserved."
                    ));
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
            writable: RwSignal::new(save_session.writable()),
            storage_notice: RwSignal::new(save_session.notice().into()),
            persistence: StoredValue::new(Arc::new(Mutex::new(save_session))),
            persisting: RwSignal::new(false),
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
        self.persisting.get() || self.state.with(|s| s.pending().is_some())
    }
    pub fn select(self, id: CharacterId) {
        self.send(AuthoringIntent::SelectProfile(id));
    }
    pub fn send(self, intent: AuthoringIntent) {
        let ui = expect_context::<Ui>();
        if self.persisting.get_untracked() {
            ui.explain("Finishing the local save. Please try again in a moment.");
            return;
        }
        let mut requests = Ok(Vec::new());
        self.state.update(|s| requests = s.dispatch(intent));
        match requests {
            Err(error) => ui.explain(error.to_string()),
            Ok(requests) => {
                for request in requests {
                    ui.notice.set(None);
                    ui.announce("Saving preview change…");
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
                                            self.persisting.set(true);
                                            let session = self.persistence.get_value();
                                            let active = self.alive.get_value();
                                            // Capture the current feedback; navigation or later errors
                                            // must not be replaced by an older persistence completion.
                                            let feedback_generation = ui.feedback_generation();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let outcome = persistence::save(
                                                    session.clone(),
                                                    snapshot,
                                                    active.clone(),
                                                )
                                                .await;
                                                if !active.load(Ordering::Acquire) {
                                                    return;
                                                }
                                                self.persisting.set(false);
                                                let session = session.lock().expect("save session");
                                                self.writable.set(session.writable());
                                                self.storage_notice.set(session.notice().into());
                                                if outcome != persistence::SaveOutcome::Cancelled
                                                    && ui.feedback_generation()
                                                        == feedback_generation
                                                {
                                                    ui.announce(outcome.announcement());
                                                }
                                            });
                                            match commit.outcome {
                                                AuthoringOutcome::ProfileCreated {
                                                    character_id,
                                                }
                                                | AuthoringOutcome::OutfitSaved {
                                                    character_id,
                                                    ..
                                                } => focus_later(format!("avatar-{character_id}")),
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
    /// Install a new source bank only at a quiescent authoring boundary. Decoded
    /// resources stay in ContentUi; only content identities enter the projection.
    pub fn install_content(self, content: AppearanceContent) -> Result<(), AuthoringError> {
        if self.busy() || !self.alive.get_value().load(Ordering::Acquire) {
            return Err(AuthoringError::Busy);
        }
        let draft = self.state.with_untracked(|s| s.draft().cloned());
        let mut projection = self.state.with_untracked(|s| s.projection().clone());
        projection.revision = projection
            .revision
            .checked_add(1)
            .ok_or(AuthoringError::StaleRevision)?;
        projection.appearance_content = content;
        projection.validate()?;
        let mut result = Ok(());
        self.provider
            .update_value(|p| result = p.replace_projection(projection.clone()));
        result?;
        let mut result = Ok(());
        self.state
            .update(|s| result = s.receive(AuthoringEvent::ProjectionReplaced { projection }));
        result?;
        if let Some(AuthoringDraft::Creation(d)) = draft {
            self.send(AuthoringIntent::OpenCreate);
            self.send(AuthoringIntent::UpdateName(d.name));
            self.send(AuthoringIntent::UpdateDescription(d.description));
            let content = self
                .state
                .with_untracked(|s| s.projection().appearance_content.clone());
            let retained = |old: Option<ContentKey>, options: &[AppearanceOption]| {
                old.filter(|key| options.iter().any(|o| &o.key == key))
            };
            let mut selection = content.default_selection();
            selection.gender = retained(d.appearance.gender, &content.genders).or(selection.gender);
            selection.skin_tone =
                retained(d.appearance.skin_tone, &content.skin_tones).or(selection.skin_tone);
            selection.head = retained(d.appearance.head, &content.heads).or_else(|| {
                content
                    .heads
                    .iter()
                    .find(|o| o.availability.is_available() && o.compatible(&selection))
                    .map(|o| o.key.clone())
            });
            selection.body = retained(d.appearance.body, &content.bodies).or_else(|| {
                content
                    .bodies
                    .iter()
                    .find(|o| o.availability.is_available() && o.compatible(&selection))
                    .map(|o| o.key.clone())
            });
            if let Some(key) = selection.gender {
                self.send(AuthoringIntent::SelectGender(key));
            }
            if let Some(key) = selection.skin_tone {
                self.send(AuthoringIntent::SelectSkinTone(key));
            }
            if let Some(key) = selection.head {
                self.send(AuthoringIntent::SelectHead(key));
            }
            if let Some(key) = selection.body {
                self.send(AuthoringIntent::SelectBody(key));
            }
            if let Some(id) = d.shard_id {
                self.send(AuthoringIntent::SelectShard(id));
            }
        } else if let Some(AuthoringDraft::Outfit(d)) = draft {
            self.send(AuthoringIntent::OpenOutfit);
            if let Some(id) = d.owned_outfit_id {
                self.send(AuthoringIntent::SelectOwnedOutfit(id));
            }
            self.send(AuthoringIntent::SelectWardrobeAction(d.action));
        }
        Ok(())
    }
    pub fn path(self, id: &CharacterId) -> String {
        self.state.with(|s| {
            s.projection()
                .profile(id)
                .and_then(|profile| {
                    profile
                        .portrait
                        .as_ref()
                        .map(|portrait| portrait.asset_path.clone())
                })
                .unwrap_or_else(|| crate::components::portrait_path(id.as_ref()))
        })
    }
}
