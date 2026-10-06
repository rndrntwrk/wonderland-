//! Memory-only browser transport for the configured gateway.
use crate::connected_adapter::{state::*, *};
pub use crate::vm_inbox::VmDelivery;
use crate::vm_inbox::{InboxError, VmInbox, VmStream};
use leptos::prelude::*;
use serde::de::DeserializeOwned;
use std::{cell::Cell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use wonderland_game_services::*;

struct Socket {
    ws: web_sys::WebSocket,
    _open: Closure<dyn FnMut(web_sys::Event)>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _close: Closure<dyn FnMut(web_sys::CloseEvent)>,
    _error: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        self.ws.set_onerror(None);
        let _ = self.ws.close();
    }
}

struct Resources {
    base: String,
    token: Option<String>,
    socket: Option<Socket>,
    generation: u64,
    requests: BTreeMap<u64, web_sys::AbortController>,
    next_request: u64,
}

impl Resources {
    fn disconnect(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.socket.take();
    }
    fn clear(&mut self) {
        self.disconnect();
        for (_, controller) in std::mem::take(&mut self.requests) {
            controller.abort();
        }
        self.token.take();
    }
}

#[derive(Clone, Copy)]
pub struct ConnectedUi {
    pub state: RwSignal<ConnectedState>,
    pub reduced_motion: RwSignal<bool>,
    /// Wake-up only. Payloads live in an ordered inbox, never a latest-value signal.
    pub vm_pending: RwSignal<()>,
    vm_inbox: StoredValue<VmInbox>,
    resources: StoredValue<Resources, LocalStorage>,
}

impl ConnectedUi {
    pub fn new(gateway_url: String) -> Self {
        let base = canonical_base(&gateway_url);
        let state = RwSignal::new(ConnectedState::default());
        let this = Self {
            state,
            reduced_motion: RwSignal::new(false),
            vm_pending: RwSignal::new(()),
            vm_inbox: StoredValue::new(VmInbox::default()),
            resources: StoredValue::new_local(Resources {
                base: base.clone().unwrap_or_default(),
                token: None,
                socket: None,
                generation: 0,
                requests: BTreeMap::new(),
                next_request: 0,
            }),
        };
        on_cleanup(move || {
            this.resources.try_update_value(Resources::clear);
            this.vm_inbox.try_update_value(VmInbox::reset);
            this.state.try_update(|s| s.ledger.logout());
        });
        match base {
            Ok(_) => this.check_health(),
            Err(error) => state.update(|s| s.notice = error.message),
        }
        this
    }

    fn reset_vm(self) {
        self.vm_inbox.try_update_value(VmInbox::reset);
        self.vm_pending.try_update(|_| {});
    }

    fn vm_stream(self) -> Option<VmStream> {
        self.state
            .try_with_untracked(|state| {
                let session = state.session.as_ref()?;
                if !state.ledger.authenticated
                    || !state.ledger.transport_ready
                    || session.state != SessionState::LotReady
                {
                    return None;
                }
                Some(VmStream {
                    browser_epoch: state.ledger.epoch,
                    source_epoch: session.epoch,
                    lot_incarnation: session.lot_incarnation?,
                })
            })
            .flatten()
    }

    /// Called by the single active world adapter after tracking `vm_pending`.
    /// Ordinary UI rerenders see an empty queue, not the last source frame again.
    pub fn drain_vm(self, stream: VmStream) -> Result<Vec<VmDelivery>, InboxError> {
        if self.vm_stream() != Some(stream) {
            return Err(InboxError::WrongStream);
        }
        self.vm_inbox
            .try_update_value(|inbox| inbox.drain(stream))
            .unwrap_or(Err(InboxError::WrongStream))
    }

    /// Malformed/overflowed streams cannot leave the world falsely marked live.
    /// Reconnection must request authoritative state; writes are never retried here.
    pub fn require_vm_recovery(self, message: &str) {
        let epoch = self.state.try_with_untracked(|state| state.ledger.epoch);
        let generation = self
            .resources
            .try_with_value(|resources| resources.generation);
        if let (Some(epoch), Some(generation)) = (epoch, generation) {
            self.disconnected(epoch, generation, message);
        }
    }

    pub fn check_health(self) {
        spawn_local(async move {
            let epoch = self.state.with_untracked(|s| s.ledger.epoch);
            let result = self
                .http::<GatewayHealth>("GET", "/health", None, false)
                .await;
            self.state.try_update(|s| {
                if s.ledger.epoch != epoch { return; }
                match result {
                    Ok(health) if health.protocol_version == GATEWAY_PROTOCOL_VERSION => {
                        if !health.configured { s.notice="This world is not ready to sign in yet.".into(); }
                        else { s.notice.clear(); }
                        s.health=Some(health);
                    },
                    Ok(_) => s.notice="This world needs a compatible game client. Ask the world operator for an updated link.".into(),
                    Err(error) => s.notice=error.message,
                }
            });
        });
    }

    pub fn login(self, username: String, password: String) {
        if username.trim().is_empty() || password.is_empty() {
            self.notice("Enter your account name and password.");
            return;
        }
        self.resources.update_value(Resources::clear);
        self.reset_vm();
        let epoch = self.state.try_update(|s| {
            let health = s.health.clone();
            let epoch = s.ledger.begin_login();
            let ledger = std::mem::take(&mut s.ledger);
            *s = ConnectedState {
                ledger,
                health,
                login_busy: true,
                ..ConnectedState::default()
            };
            epoch
        });
        let Some(epoch) = epoch else {
            return;
        };
        spawn_local(async move {
            let body = serde_json::to_string(&LoginRequest { username, password }).map_err(|_| {
                safe_error(
                    ErrorCode::InvalidRequest,
                    "Could not prepare the sign in request.",
                )
            });
            let result = match body {
                Ok(body) => {
                    self.http::<SessionCreated>("POST", "/v1/sessions", Some(body), false)
                        .await
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(created) => {
                    let current = self
                        .state
                        .try_with_untracked(|s| s.ledger.epoch == epoch)
                        .unwrap_or(false);
                    if !current {
                        let base = self.resources.try_with_value(|r| r.base.clone());
                        if let Some(base) = base {
                            revoke(base, created.session_token);
                        }
                        return;
                    }
                    let SessionCreated {
                        session_token,
                        session,
                        roster,
                        shards,
                    } = created;
                    let accepted = self
                        .state
                        .try_update(|s| s.accept_account(epoch, session, roster, shards))
                        .unwrap_or(false);
                    if accepted {
                        self.resources
                            .update_value(|r| r.token = Some(session_token));
                        self.connect_socket();
                        focus("connected-play");
                    }
                }
                Err(error) => {
                    self.state.try_update(|s| {
                        if s.ledger.epoch == epoch {
                            s.login_busy = false;
                            s.notice = error.message;
                        }
                    });
                }
            }
        });
    }

    pub fn logout(self) {
        let old = self
            .resources
            .with_value(|r| (r.base.clone(), r.token.clone()));
        self.resources.update_value(Resources::clear);
        self.reset_vm();
        self.state.update(|s| {
            let health = s.health.clone();
            s.ledger.logout();
            let ledger = std::mem::take(&mut s.ledger);
            *s = ConnectedState {
                ledger,
                health,
                ..ConnectedState::default()
            };
        });
        if let Some(token) = old.1 {
            revoke(old.0, token);
        }
        focus("connected-username");
    }

    fn session_expired(self, epoch: u64) {
        if !self
            .state
            .try_with_untracked(|s| s.ledger.epoch == epoch)
            .unwrap_or(false)
        {
            return;
        }
        self.logout();
        self.notice("Your session has ended. Sign in again to continue.");
    }

    pub fn reconnect(self) {
        if !self.state.with_untracked(|s| s.ledger.authenticated) {
            return;
        }
        self.resources.update_value(Resources::disconnect);
        let epoch = self.state.with_untracked(|s| s.ledger.epoch);
        self.state.update(|s|{s.socket_connecting=true;s.notice.clear();s.clear_live_hud();s.cancel_home_intent();s.ledger.close_transport(epoch,"The connection was interrupted. Check the current state before repeating an action.");});
        spawn_local(async move {
            let result = self
                .http::<SessionProjection>("GET", "/v1/session", None, true)
                .await;
            if !self
                .state
                .try_with_untracked(|s| s.ledger.epoch == epoch)
                .unwrap_or(false)
            {
                return;
            }
            match result {
                Ok(session) => {
                    self.state.update(|s| s.session = Some(session));
                    self.connect_socket();
                }
                Err(error) if error.code == ErrorCode::Unauthorized => self.session_expired(epoch),
                Err(error) => {
                    self.state.update(|s| {
                        s.socket_connecting = false;
                        s.notice = error.message;
                    });
                }
            }
        });
    }

    fn connect_socket(self) {
        self.resources.update_value(Resources::disconnect);
        self.reset_vm();
        let (base, generation) = self
            .resources
            .with_value(|r| (r.base.clone(), r.generation));
        let local_epoch = self.state.with_untracked(|s| s.ledger.epoch);
        self.state.update(|s| s.socket_connecting = true);
        let url = match socket_url(&base) {
            Ok(url) => url,
            Err(error) => {
                self.notice(&error.message);
                return;
            }
        };
        let ws = match web_sys::WebSocket::new(&url) {
            Ok(ws) => ws,
            Err(_) => {
                self.disconnected(
                    local_epoch,
                    generation,
                    "Could not connect to this world. Try reconnecting.",
                );
                return;
            }
        };
        let open_ws = ws.clone();
        let open = Closure::wrap(Box::new(move |_: web_sys::Event| {
            if !self.current(local_epoch, generation) {
                return;
            }
            let token = self.resources.with_value(|r| r.token.clone());
            if let Some(session_token) = token {
                match serde_json::to_string(&BrowserMessage::Authenticate { session_token }) {
                    Ok(text) if open_ws.send_with_str(&text).is_ok() => {}
                    _ => self.disconnected(
                        local_epoch,
                        generation,
                        "Could not authenticate this connection. Try reconnecting.",
                    ),
                }
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            if !self.current(local_epoch, generation) {
                return;
            }
            let Some(text) = event.data().as_string() else {
                self.disconnected(
                    local_epoch,
                    generation,
                    "This world sent an unreadable update. Reconnect to continue.",
                );
                return;
            };
            let envelope = match decode_gateway_envelope(text.as_bytes()) {
                Ok(envelope) => envelope,
                Err(error) => {
                    self.disconnected(local_epoch, generation, &error.message);
                    return;
                }
            };
            if matches!(&envelope.event,GatewayEvent::Error {error} if error.code==ErrorCode::Unauthorized)
            {
                self.session_expired(local_epoch);
                return;
            }
            let refresh = envelope
                .operation_id
                .as_ref()
                .and_then(|id| self.state.with_untracked(|s| s.sent.get(id).cloned()))
                .is_some_and(|op| {
                    matches!(
                        op,
                        GatewayOperation::CreateAvatar { .. } | GatewayOperation::RetireAvatar
                    )
                });
            let accepted = matches!(
                &envelope.event,
                GatewayEvent::Outcome {
                    status: OutcomeStatus::Accepted,
                    ..
                }
            );
            let vm = match &envelope.event {
                GatewayEvent::VmFrame {
                    lot_incarnation,
                    direct,
                    data,
                } => {
                    let valid = self.state.with_untracked(|s| {
                        s.session.as_ref().is_some_and(|session| {
                            session.state == SessionState::LotReady
                                && session.lot_incarnation == Some(*lot_incarnation)
                        })
                    });
                    if !valid {
                        return;
                    }
                    Some(VmDelivery {
                        browser_epoch: local_epoch,
                        source_epoch: envelope.epoch,
                        lot_incarnation: Some(*lot_incarnation),
                        direct: *direct,
                        data: data.clone(),
                    })
                }
                _ => None,
            };
            let applied = self
                .state
                .try_update(|s| s.receive(local_epoch, envelope))
                .unwrap_or(false);
            if applied {
                // Derive the inbox scope from the accepted authenticated state,
                // not from an unaccepted message or a DOM selection.
                let stream = self.vm_stream();
                self.vm_inbox.update_value(|inbox| inbox.bind(stream));
                if let Some(frame) = vm {
                    if !matches!(
                        self.vm_inbox.try_update_value(|inbox| inbox.push(frame)),
                        Some(Ok(()))
                    ) {
                        self.disconnected(
                            local_epoch,
                            generation,
                            "World updates could not be kept in order. Reconnect to receive a complete world state. Unconfirmed actions remain unknown.",
                        );
                        return;
                    }
                    self.vm_pending.set(());
                }
                if refresh && accepted {
                    self.refresh_roster();
                }
                if let Some(operation) = self
                    .state
                    .try_update(|state| state.take_home_follow_up())
                    .flatten()
                {
                    self.send(operation, "Go home", None);
                }
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        let close = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
            self.disconnected(local_epoch,generation,"Connection lost. Reconnect to continue. Unconfirmed actions have an unknown result.");
        }) as Box<dyn FnMut(web_sys::CloseEvent)>);
        let error = Closure::wrap(Box::new(move |_: web_sys::Event| {
            self.disconnected(
                local_epoch,
                generation,
                "The connection was interrupted. Reconnect to continue.",
            );
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onopen(Some(open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(close.as_ref().unchecked_ref()));
        ws.set_onerror(Some(error.as_ref().unchecked_ref()));
        self.resources.update_value(|r| {
            r.socket = Some(Socket {
                ws,
                _open: open,
                _message: message,
                _close: close,
                _error: error,
            })
        });
    }

    fn current(self, epoch: u64, generation: u64) -> bool {
        self.state
            .try_with_untracked(|s| s.ledger.epoch == epoch && s.ledger.authenticated)
            .unwrap_or(false)
            && self
                .resources
                .try_with_value(|r| r.generation == generation)
                .unwrap_or(false)
    }

    fn disconnected(self, epoch: u64, generation: u64, message: &str) {
        if !self.current(epoch, generation) {
            return;
        }
        self.state.try_update(|s| {
            s.ledger.close_transport(epoch, message);
            s.cancel_home_intent();
            s.clear_live_hud();
            s.socket_connecting = false;
            s.notice = message.into();
        });
        self.reset_vm();
        // A malformed or closed socket cannot later restore this transport with another callback.
        // Defer dropping its callback closures until the current browser callback has returned.
        self.resources
            .try_update_value(|r| r.generation = r.generation.wrapping_add(1));
        spawn_local(async move {
            let _ = JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL)).await;
            self.resources.try_update_value(|r| {
                if r.generation == generation.wrapping_add(1) {
                    r.socket.take();
                }
            });
        });
    }

    pub fn refresh_roster(self) {
        let Some(stamp) = self.state.try_update(|s| s.ledger.begin_read("roster")) else {
            return;
        };
        spawn_local(async move {
            let result = self
                .http::<Vec<RosterEntry>>("GET", "/v1/roster", None, true)
                .await;
            if matches!(&result,Err(error) if error.code==ErrorCode::Unauthorized) {
                self.session_expired(stamp.epoch);
                return;
            }
            self.state.try_update(|s| {
                if !s.ledger.finish_read("roster", &stamp) {
                    return;
                }
                match result {
                    Ok(roster) => {
                        if !roster
                            .iter()
                            .any(|entry| Some(entry.avatar_id) == s.selected_avatar)
                        {
                            s.selected_avatar = roster.first().map(|entry| entry.avatar_id);
                        }
                        s.roster = roster;
                    }
                    Err(error) => s.notice = error.message,
                }
            });
        });
    }

    pub fn query(self, slot: impl Into<String>, query: DirectoryQuery) {
        let slot = slot.into();
        let Some(stamp) = self.state.try_update(|s| {
            s.reads.insert(
                slot.clone(),
                ReadSlot {
                    status: LoadState::Loading,
                    result: Some(DirectoryResult {
                        query: query.clone(),
                        data: serde_json::Value::Null,
                    }),
                },
            );
            s.ledger.begin_read(&slot)
        }) else {
            return;
        };
        spawn_local(async move {
            let body = serde_json::to_string(&DirectoryRequest {
                query: query.clone(),
            })
            .ok();
            let result = self
                .http::<DirectoryResult>("POST", "/v1/query", body, true)
                .await;
            if matches!(&result,Err(error) if error.code==ErrorCode::Unauthorized) {
                self.session_expired(stamp.epoch);
                return;
            }
            self.state.try_update(|s| {
                if !s.ledger.finish_read(&slot, &stamp) {
                    return;
                }
                match result {
                    Ok(result) if result.query == query => {
                        if slot == "property" {
                            s.ledger.selected_lot = source_u32(&result.data, "lot_id");
                        }
                        s.reads.insert(
                            slot,
                            ReadSlot {
                                status: LoadState::Ready,
                                result: Some(result),
                            },
                        );
                    }
                    Ok(_) => {
                        s.reads.insert(
                            slot,
                            ReadSlot {
                                status: LoadState::Failed(
                                    "The world returned a different selection. Try again.".into(),
                                ),
                                result: Some(DirectoryResult {
                                    query,
                                    data: serde_json::Value::Null,
                                }),
                            },
                        );
                    }
                    Err(error) => {
                        s.reads.insert(
                            slot,
                            ReadSlot {
                                status: LoadState::Failed(error.message),
                                result: Some(DirectoryResult {
                                    query,
                                    data: serde_json::Value::Null,
                                }),
                            },
                        );
                    }
                }
            });
        });
    }

    pub fn send(self, operation: GatewayOperation, label: &str, draft_key: Option<&str>) {
        if matches!(
            operation,
            GatewayOperation::ConnectCity { .. }
                | GatewayOperation::DisconnectCity
                | GatewayOperation::JoinLot { .. }
                | GatewayOperation::LeaveLot
                | GatewayOperation::RetireAvatar
                | GatewayOperation::Roommate {
                    action: RoommateAction::Accept | RoommateAction::Kick,
                    ..
                }
        ) {
            self.state.update(|state| state.cancel_home_intent());
        }
        let _ = self.send_tracked(operation, label, draft_key);
    }

    fn send_tracked(
        self,
        operation: GatewayOperation,
        label: &str,
        draft_key: Option<&str>,
    ) -> Option<RequestStamp> {
        let capability = operation_capability(&operation);
        if let Some(reason) = self
            .state
            .with_untracked(|s| s.capability_reason(capability))
        {
            self.notice(&reason);
            return None;
        }
        if self.state.with_untracked(|s| s.busy(label)) {
            return None;
        }
        let source_epoch = self
            .state
            .with_untracked(|s| s.session.as_ref().map(|s| s.epoch))?;
        let stamp = match self
            .state
            .try_update(|s| s.ledger.begin_operation(label, draft_key))
        {
            Some(Ok(stamp)) => stamp,
            Some(Err(error)) => {
                self.notice(&error);
                return None;
            }
            None => return None,
        };
        let body = serde_json::to_string(&BrowserMessage::Request {
            epoch: source_epoch,
            operation_id: stamp.operation_id.clone(),
            operation: operation.clone(),
        });
        self.state.update(|s| {
            s.sent.insert(stamp.operation_id.clone(), operation);
            s.wire_epochs
                .insert(stamp.operation_id.clone(), source_epoch);
        });
        let body = match body {
            Ok(body) if body.len() <= MAX_GATEWAY_REQUEST_BYTES => body,
            _ => {
                self.state.update(|s| {
                    s.ledger.receive_operation(
                        &stamp,
                        OperationStatus::Rejected(
                            "This action is too large to send. Shorten it and try again.".into(),
                        ),
                    );
                });
                return None;
            }
        };
        let sent = self.resources.with_value(|r| {
            r.socket.as_ref().is_some_and(|socket| {
                socket.ws.ready_state() == web_sys::WebSocket::OPEN
                    && socket.ws.send_with_str(&body).is_ok()
            })
        });
        if !sent {
            self.state.update(|s| {s.ledger.receive_operation(&stamp,OperationStatus::Unknown("Could not confirm that the world received this action. Refresh before trying it again.".into()));s.ledger.close_transport(stamp.epoch,"Connection interrupted.");s.clear_live_hud();s.cancel_home_intent();});
            return None;
        }
        Some(stamp)
    }

    pub fn select_avatar(self, avatar_id: u32) {
        self.state.update(|s| {
            if s.selected_avatar != Some(avatar_id) {
                s.cancel_home_intent();
            }
            if let Some(entry) = s.roster.iter().find(|entry| entry.avatar_id == avatar_id) {
                s.selected_avatar = Some(avatar_id);
                s.selected_shard = Some(entry.shard_name.clone());
            }
        });
    }

    pub fn select_shard(self, shard_name: String) {
        self.state.update(|state| {
            state.cancel_home_intent();
            state.selected_avatar = state
                .roster
                .iter()
                .find(|entry| entry.shard_name == shard_name)
                .map(|entry| entry.avatar_id);
            state.selected_shard = Some(shard_name);
        });
    }

    pub fn go_home(self) {
        let operation = match self.state.with_untracked(|state| state.home_request()) {
            Ok(operation) => operation,
            Err(message) => {
                self.notice(&message);
                return;
            }
        };
        if matches!(operation, GatewayOperation::JoinLot { .. }) {
            self.send(operation, "Go home", None);
        } else if let Some(stamp) = self.send_tracked(operation, "Go home city", None) {
            self.state.update(|state| {
                state.arm_home_city(&stamp);
            });
        }
    }

    pub fn play(self) {
        let selected = self.state.with_untracked(|s| s.selected_entry().cloned());
        if let Some(entry) = selected {
            self.send(
                GatewayOperation::ConnectCity {
                    shard_name: entry.shard_name,
                    avatar_id: entry.avatar_id,
                },
                "Enter city",
                None,
            );
        }
    }

    pub fn select_person(self, avatar_id: u32) {
        self.state.update(|s| {
            s.ledger.selected_person = Some(avatar_id);
            s.panel = Some(Panel::Profile);
        });
        self.query("profile", DirectoryQuery::Avatar { avatar_id });
        focus("connected-panel-title");
    }

    pub fn select_lot(self, lot_id: u32) {
        self.state.update(|s| {
            s.ledger.selected_lot = Some(lot_id);
            s.panel = Some(Panel::Property);
        });
        self.query("property", DirectoryQuery::Lot { lot_id });
        focus("connected-panel-title");
    }

    pub fn select_neighborhood(self, neighborhood_id: u32) {
        self.state.update(|s| {
            s.selected_neighborhood = Some(neighborhood_id);
            s.panel = Some(Panel::Neighborhood);
        });
        self.query(
            "neighborhood",
            DirectoryQuery::Neighborhood { neighborhood_id },
        );
        self.query("bulletins", DirectoryQuery::Bulletins { neighborhood_id });
        self.query("elections", DirectoryQuery::Elections { neighborhood_id });
        focus("connected-panel-title");
    }

    pub fn panel(self, panel: Panel) {
        self.state.update(|s| {
            if panel == Panel::Create {
                s.cancel_home_intent();
            }
            s.panel = Some(panel);
        });
        if panel == Panel::Profile
            && let Some(id) = self.state.with_untracked(|s| {
                s.session
                    .as_ref()
                    .and_then(|s| s.avatar_id)
                    .filter(|id| *id != 0)
                    .or(s.selected_avatar)
            })
        {
            self.select_person(id);
        }
        if panel == Panel::Inbox && self.state.with_untracked(|s| s.inbox.is_empty()) {
            self.poll_mail();
        }
        if panel == Panel::People {
            self.people_page(1);
        }
        if panel == Panel::Property
            && self
                .state
                .with_untracked(|s| s.read("lots").status == LoadState::Idle)
        {
            self.lots_page(1);
        }
        if panel == Panel::Neighborhood
            && self
                .state
                .with_untracked(|s| s.read("neighborhoods").status == LoadState::Idle)
            && let Some(shard_id) = self.state.with_untracked(|s| s.shard().map(|s| s.id))
        {
            self.query("neighborhoods", DirectoryQuery::Neighborhoods { shard_id });
        }
        focus("connected-panel-title");
    }

    pub fn people_page(self, page: u32) {
        if let Some(shard) = self.state.with_untracked(|s| s.shard().cloned()) {
            self.query(
                "people",
                DirectoryQuery::AvatarPage {
                    shard_id: shard.id,
                    page,
                    per_page: 50,
                },
            );
        }
    }
    pub fn lots_page(self, page: u32) {
        if let Some(shard) = self.state.with_untracked(|s| s.shard().cloned()) {
            self.query(
                "lots",
                DirectoryQuery::LotPage {
                    shard_id: shard.id,
                    page,
                    per_page: 50,
                },
            );
        }
    }
    pub fn poll_mail(self) {
        self.send(
            GatewayOperation::MailPoll {
                since_ticks: DecimalU64(0),
            },
            "Refresh inbox",
            None,
        );
    }
    pub fn draft(self, key: &str, value: String) {
        self.state.update(|s| {
            s.ledger.drafts.insert(key.into(), value);
        });
    }
    pub fn notice(self, message: &str) {
        self.state.try_update(|s| s.notice = message.into());
    }

    async fn http<T: DeserializeOwned>(
        self,
        method: &str,
        path: &str,
        body: Option<String>,
        authenticated: bool,
    ) -> Result<T, ServiceError> {
        let controller = web_sys::AbortController::new().map_err(|_| {
            safe_error(
                ErrorCode::Transport,
                "This browser could not start a request.",
            )
        })?;
        let (base, token, id) = self
            .resources
            .try_update_value(|r| {
                r.next_request = r.next_request.wrapping_add(1);
                let id = r.next_request;
                r.requests.insert(id, controller.clone());
                (
                    r.base.clone(),
                    if authenticated { r.token.clone() } else { None },
                    id,
                )
            })
            .ok_or_else(|| safe_error(ErrorCode::Disconnected, "This session has ended."))?;
        let result = fetch_json(&base, method, path, body, token, controller).await;
        self.resources.try_update_value(|r| {
            r.requests.remove(&id);
        });
        result
    }
}

fn operation_capability(operation: &GatewayOperation) -> &'static str {
    match operation {
        GatewayOperation::RefreshRoster => "account",
        GatewayOperation::ConnectCity { .. }
        | GatewayOperation::DisconnectCity
        | GatewayOperation::FindAvatar { .. } => "city",
        GatewayOperation::JoinLot { .. } | GatewayOperation::LeaveLot => "lot",
        GatewayOperation::CreateAvatar { .. } => "create_avatar",
        GatewayOperation::RetireAvatar => "retire_avatar",
        GatewayOperation::PrivateMessage { .. } => "private_message",
        GatewayOperation::MailPoll { .. }
        | GatewayOperation::MailSend { .. }
        | GatewayOperation::MailDelete { .. } => "mail",
        GatewayOperation::PurchaseLot { .. } | GatewayOperation::Roommate { .. } => "property",
        GatewayOperation::Neighborhood { .. } => "neighborhood",
        GatewayOperation::Bulletin { .. } => "bulletin",
        GatewayOperation::LotChat { .. } => "lot_chat",
        GatewayOperation::Eod { .. } => "eod",
        GatewayOperation::LotCommand { .. } => "lot_command",
        GatewayOperation::RequestWorldSnapshot { .. } => "world_snapshot",
        GatewayOperation::CancelInteraction { .. } => "cancel_interaction",
        GatewayOperation::WalkTo { .. } => "walk",
    }
}

fn safe_error(code: ErrorCode, message: &str) -> ServiceError {
    ServiceError::new(code, message)
}

fn canonical_base(value: &str) -> Result<String, ServiceError> {
    let base = web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_default();
    let url = web_sys::Url::new_with_base(value, &base).map_err(|_| {
        safe_error(
            ErrorCode::NotConfigured,
            "This world's connection address is invalid.",
        )
    })?;
    if !matches!(url.protocol().as_str(), "https:" | "http:")
        || !url.username().is_empty()
        || !url.password().is_empty()
        || !url.search().is_empty()
        || !url.hash().is_empty()
    {
        return Err(safe_error(
            ErrorCode::NotConfigured,
            "This world's connection address is invalid.",
        ));
    }
    Ok(url.href().trim_end_matches('/').into())
}

fn socket_url(base: &str) -> Result<String, ServiceError> {
    let url = web_sys::Url::new(&format!("{base}/v1/ws")).map_err(|_| {
        safe_error(
            ErrorCode::NotConfigured,
            "This world's connection address is invalid.",
        )
    })?;
    url.set_protocol(if url.protocol() == "https:" {
        "wss:"
    } else {
        "ws:"
    });
    Ok(url.href())
}

struct RequestTimeout {
    id: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Drop for RequestTimeout {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(self.id);
        }
    }
}

async fn fetch_json<T: DeserializeOwned>(
    base: &str,
    method: &str,
    path: &str,
    body: Option<String>,
    token: Option<String>,
    controller: web_sys::AbortController,
) -> Result<T, ServiceError> {
    let window = web_sys::window()
        .ok_or_else(|| safe_error(ErrorCode::Transport, "The browser is not available."))?;
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let abort = controller.clone();
    let callback = Closure::wrap(Box::new(move || {
        timeout_flag.set(true);
        abort.abort();
    }) as Box<dyn FnMut()>);
    let id = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            30_000,
        )
        .map_err(|_| safe_error(ErrorCode::Transport, "Could not start the request."))?;
    let _timeout = RequestTimeout {
        id,
        _callback: callback,
    };
    let init = web_sys::RequestInit::new();
    init.set_method(method);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_cache(web_sys::RequestCache::NoStore);
    init.set_redirect(web_sys::RequestRedirect::Error);
    init.set_signal(Some(&controller.signal()));
    if let Some(body) = &body {
        init.set_body(&JsValue::from_str(body));
    }
    let request = web_sys::Request::new_with_str_and_init(&format!("{base}{path}"), &init)
        .map_err(|_| safe_error(ErrorCode::InvalidRequest, "Could not prepare the request."))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|_| safe_error(ErrorCode::Transport, "Could not prepare the request."))?;
    if body.is_some() {
        let _ = request.headers().set("Content-Type", "application/json");
    }
    if let Some(token) = token {
        request
            .headers()
            .set("Authorization", &format!("Bearer {token}"))
            .map_err(|_| safe_error(ErrorCode::Transport, "Could not authenticate the request."))?;
    }
    let failure = || {
        if timed_out.get() {
            safe_error(
                ErrorCode::Timeout,
                "The world took too long to reply. Try again.",
            )
        } else {
            safe_error(
                ErrorCode::Transport,
                "Could not reach this world. Check the connection and try again.",
            )
        }
    };
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| failure())?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| failure())?;
    if response
        .headers()
        .get("Content-Length")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|n| n > MAX_RESPONSE_BYTES)
    {
        controller.abort();
        return Err(safe_error(
            ErrorCode::ResponseTooLarge,
            "This response is too large. Narrow the search and try again.",
        ));
    }
    let status = response.status();
    let mut bytes = Vec::new();
    if let Some(stream) = response.body() {
        let reader = stream
            .get_reader()
            .dyn_into::<web_sys::ReadableStreamDefaultReader>()
            .map_err(|_| failure())?;
        loop {
            let chunk = JsFuture::from(reader.read()).await.map_err(|_| failure())?;
            if js_sys::Reflect::get(&chunk, &JsValue::from_str("done"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                break;
            }
            let value =
                js_sys::Reflect::get(&chunk, &JsValue::from_str("value")).map_err(|_| failure())?;
            let array = value
                .dyn_into::<js_sys::Uint8Array>()
                .map_err(|_| failure())?;
            if bytes.len().saturating_add(array.length() as usize) > MAX_RESPONSE_BYTES {
                controller.abort();
                let _ = reader.cancel();
                return Err(safe_error(
                    ErrorCode::ResponseTooLarge,
                    "This response is too large. Narrow the search and try again.",
                ));
            }
            bytes.extend(array.to_vec());
        }
    }
    if !(200..300).contains(&status) {
        return Err(decode_bounded::<ServiceError>(&bytes).unwrap_or_else(|_| {
            safe_error(
                if status == 401 {
                    ErrorCode::Unauthorized
                } else {
                    ErrorCode::Rejected
                },
                if status == 401 {
                    "Your session has ended. Sign in again."
                } else {
                    "The world could not complete this request."
                },
            )
        }));
    }
    if bytes.is_empty() {
        bytes.extend_from_slice(b"null");
    }
    decode_bounded(&bytes)
}

fn revoke(base: String, token: String) {
    spawn_local(async move {
        if let Ok(controller) = web_sys::AbortController::new() {
            let _: Result<serde_json::Value, _> = fetch_json(
                &base,
                "DELETE",
                "/v1/session",
                None,
                Some(token),
                controller,
            )
            .await;
        }
    });
}

pub fn focus(id: &str) {
    let id = id.to_owned();
    spawn_local(async move {
        let _ = JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL)).await;
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    });
}
