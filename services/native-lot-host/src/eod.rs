//! Native-only Timer transaction adapter. The complete private checkpoint and
//! pending delivery ledger are paired with the accepted simulation cursor.
use crate::{
    Result,
    runtime::{hex, unhex},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wonderland_content_ir::manifest::Digest;
use wonderland_eod_runtime as eod;
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, GameRuntime, RuntimeEvent,
    sim_core::{
        eod::{EodEventDelivery, EodSessionIdentity},
        owned_admission::OwnedControl,
        state::SimState,
    },
};
use wonderland_native_host_contracts::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Ticket {
    scope: u64,
    epoch: u64,
    instance: u64,
    generation: u64,
}
impl From<eod::SessionTicket> for Ticket {
    fn from(t: eod::SessionTicket) -> Self {
        Self {
            scope: t.host_scope.0,
            epoch: t.host_epoch,
            instance: t.instance.0,
            generation: t.generation,
        }
    }
}
impl From<Ticket> for eod::SessionTicket {
    fn from(t: Ticket) -> Self {
        Self {
            host_scope: eod::HostScopeId(t.scope),
            host_epoch: t.epoch,
            instance: eod::InstanceId(t.instance),
            generation: t.generation,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Association {
    source: EodSessionIdentity,
    owner: OwnedControl,
    /// Every historical native ticket is retained to route delayed outputs to
    /// the source association which produced them, including after rebind.
    tickets: Vec<Ticket>,
    connection: u64,
    next_native_sequence: u64,
    closed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct RequestReceipt {
    id: u64,
    sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct PublicMapping {
    output_id: u64,
    delivery: EodEventDelivery,
}
#[derive(Clone, Serialize, Deserialize)]
struct UiReceipt {
    session: u64,
    sequence: u64,
    sha256: String,
}
fn ui_cursor(receipts: &[UiReceipt], session: u64) -> (u64, Option<&UiReceipt>) {
    let mut next = 1;
    let mut last = None;
    for receipt in receipts.iter().filter(|r| r.session == session) {
        next += 1;
        last = Some(receipt);
    }
    (next, last)
}
#[derive(Clone, Serialize, Deserialize)]
struct Stored {
    schema_version: u32,
    lot_id: u64,
    revision: u64,
    handler_scope: u64,
    handler_epoch: u64,
    handler_revision: u64,
    handler_hex: String,
    next_connection: u64,
    associations: Vec<Association>,
    requests: Vec<RequestReceipt>,
    public: Vec<PublicMapping>,
    ui_receipts: Vec<UiReceipt>,
}
struct Auth(BTreeMap<eod::ConnectionId, eod::ActorId>);
impl eod::ConnectionAuthority for Auth {
    fn authenticated_actor(&self, c: eod::ConnectionId) -> Option<eod::ActorId> {
        self.0.get(&c).copied()
    }
}
struct Registers(BTreeMap<eod::InvokerId, eod::TimerRegisters>);
impl eod::RegisterSource for Registers {
    fn timer_registers(&self, i: eod::InvokerId) -> Option<eod::TimerRegisters> {
        self.0.get(&i).copied()
    }
}
struct PrivateStage {
    bytes: Vec<u8>,
}
impl eod::PrivateTimerTransactionSink for PrivateStage {
    fn stage_private_transaction(
        &mut self,
        _: eod::CheckpointStamp,
        bytes: &[u8],
        _: &[eod::PrivateTimerDelivery],
    ) -> std::result::Result<(), eod::StoreError> {
        if bytes.len() > EOD_PRIVATE_CHECKPOINT_LIMIT {
            return Err(eod::StoreError::TooLarge);
        }
        self.bytes = bytes.to_vec();
        Ok(())
    }
}
struct PrivateLoad(Vec<u8>);
impl eod::PrivateTimerTransactionStore for PrivateLoad {
    fn read_private_transaction(
        &mut self,
        _: eod::HostScopeId,
        to: &mut [u8],
    ) -> std::result::Result<eod::PrivateRead, eod::StoreError> {
        if self.0.len() > to.len() {
            return Ok(eod::PrivateRead {
                bytes_written: 0,
                complete: false,
            });
        }
        to[..self.0.len()].copy_from_slice(&self.0);
        Ok(eod::PrivateRead {
            bytes_written: self.0.len(),
            complete: true,
        })
    }
}
fn limits() -> eod::HostLimits {
    eod::HostLimits {
        max_instances: 64,
        max_participants: 64,
        max_timers: 64,
        max_checkpoint_bytes: EOD_PRIVATE_CHECKPOINT_LIMIT,
        max_public_events: 512,
        max_private_messages: 512,
        max_private_output_bytes: 96 * 1024,
        idle_timeout_ticks: 30 * 86400,
        ..Default::default()
    }
}
fn owner(state: &SimState, source: EodSessionIdentity) -> Result<OwnedControl> {
    state
        .owned_admission
        .actors()
        .and_then(|a| {
            a.values().find(|c| {
                c.actor == source.actor && c.control_generation == source.control_generation
            })
        })
        .cloned()
        .ok_or_else(|| "EOD source owner is not installed".into())
}
fn matches_owner(state: &SimState, control: &OwnedControl) -> bool {
    state
        .owned_admission
        .actors()
        .and_then(|a| a.get(&control.principal))
        == Some(control)
}
fn is_active(state: &SimState, control: &OwnedControl) -> bool {
    matches_owner(state, control) && state.interaction_access.contains_key(&control.actor)
}
fn registers(state: &SimState, invoker: EntityRef) -> Result<eod::TimerRegisters> {
    if !state.ids.is_live(invoker) {
        return Err("EOD invoker generation is stale".into());
    }
    let t = state
        .threads
        .get(&invoker.object_id)
        .ok_or("EOD invoker thread missing")?;
    Ok(eod::TimerRegisters([
        t.temps[0], t.temps[1], t.temps[2], t.temps[3],
    ]))
}
fn auth(stored: &Stored, state: &SimState) -> Auth {
    Auth(
        stored
            .associations
            .iter()
            .filter(|a| is_active(state, &a.owner))
            .map(|a| {
                (
                    eod::ConnectionId(a.connection),
                    eod::ActorId(u64::from(a.owner.avatar_id.0)),
                )
            })
            .collect(),
    )
}
fn key(t: Ticket) -> (u64, u64, u64, u64) {
    (t.scope, t.epoch, t.instance, t.generation)
}
fn private_digest(session: u64, id: u64, event: &str, binary: bool, payload_hex: &str) -> String {
    Digest::of(&serde_json::to_vec(&json!({"source_session_id":DecimalId(session),"output_id":DecimalId(id),"event":event,"binary":binary,"payload_hex":payload_hex})).expect("serializable")).as_str().to_owned()
}

// Receipt keys identify the immutable output, not a tick or epoch shared by
// several effects. The complete proof remains in the private receipt bytes.
fn suppress_output(
    transaction: &mut eod::PrivateTimerTransaction,
    id: eod::PrivateTimerOutputId,
    recipient: eod::PrivateTimerRecipient,
    proof: &[u8],
) -> Result<()> {
    transaction.suppress_private(id, recipient, id.0, proof)?;
    Ok(())
}
fn settle_public_output(
    transaction: &mut eod::PrivateTimerTransaction,
    id: eod::PrivateTimerOutputId,
    origin: eod::SessionTicket,
    superseded: bool,
    proof: &[u8],
) -> Result<()> {
    if superseded {
        transaction.supersede_public(id, origin, id.0, proof)?;
    } else {
        transaction.record_public_delivery(id, id.0, proof)?;
    }
    Ok(())
}

pub(crate) enum Input {
    Ui(OwnedControlV2, NativeEodUiV1),
    Ack(OwnedControlV2, u64, u64, String),
}

pub(crate) struct TimerAdapter {
    transaction: eod::PrivateTimerTransaction,
    stored: Stored,
    committed_sha256: Option<String>,
}
impl TimerAdapter {
    pub(crate) fn input(&mut self, runtime: &GameRuntime, input: Input) -> Result<()> {
        match input {
            Input::Ui(control, message) => self.ui(runtime, &control, &message),
            Input::Ack(control, session, id, digest) => {
                self.acknowledge(runtime, &control, session, id, &digest)
            }
        }
    }
    /// Dedicated custody may only release after the exact public and private
    /// effects have a retained terminal disposition in this paired candidate.
    pub(crate) fn validate_departures(&self, runtime: &GameRuntime) -> Result<()> {
        for record in runtime
            .sim()
            .state()
            .owned_departures
            .records
            .values()
            .filter(|r| r.detached_tick.is_some())
        {
            for association in self
                .stored
                .associations
                .iter()
                .filter(|a| a.owner == record.request.control)
            {
                if !association.closed
                    || self.transaction.private_deliveries().iter().any(|d| {
                        association
                            .tickets
                            .contains(&Ticket::from(d.origin_ticket()))
                            && d.disposition().is_none()
                    })
                {
                    return Err("owned release awaits exact private EOD disposition".into());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn fresh(runtime: &GameRuntime) -> Result<Self> {
        let s = runtime.sim().state();
        if !s.eod.sessions.is_empty() {
            return Err("active EOD requires its paired private checkpoint".into());
        }
        let transaction = eod::NativeHost::new(
            eod::HostIdentity {
                scope: eod::HostScopeId(s.lot_id),
                epoch: s.authority_epoch,
            },
            limits(),
        )?
        .fork_private_timer_transaction()?;
        Ok(Self {
            transaction,
            stored: Stored {
                schema_version: 1,
                lot_id: s.lot_id,
                revision: 0,
                handler_scope: s.lot_id,
                handler_epoch: s.authority_epoch,
                handler_revision: 0,
                handler_hex: String::new(),
                next_connection: 1,
                associations: vec![],
                requests: vec![],
                public: vec![],
                ui_receipts: vec![],
            },
            committed_sha256: None,
        })
    }
    pub(crate) fn fork(&self) -> Self {
        Self {
            transaction: self.transaction.fork(),
            stored: self.stored.clone(),
            committed_sha256: self.committed_sha256.clone(),
        }
    }
    pub(crate) fn restore(
        runtime: &GameRuntime,
        checkpoint: &NativeEodCheckpointV1,
        marker: &TakeoverMarkerV1,
    ) -> Result<Self> {
        checkpoint.validate()?;
        let state = runtime.sim().state();
        if checkpoint.binding != marker.grant.old_binding
            || checkpoint.cursor != marker.grant.old_cursor
            || crate::runtime::cursor(runtime)? != marker.new_cursor
        {
            return Err("EOD recovery pairing mismatch".into());
        }
        let bytes = unhex(&checkpoint.checkpoint_hex)?;
        if Digest::of(&bytes).as_str() != checkpoint.checkpoint_sha256 {
            return Err("EOD private checkpoint digest mismatch".into());
        }
        let stored: Stored = serde_json::from_slice(&bytes)?;
        if stored.schema_version != 1
            || stored.revision != checkpoint.revision.0
            || stored.lot_id != state.lot_id
            || stored.handler_scope != state.lot_id
            || stored.handler_epoch != marker.grant.old_cursor.authority_epoch.0
            || stored.associations.len() > 64
            || stored.requests.len() > 4096
            || stored.public.len() > 512
            || stored.ui_receipts.len() > 4096
            || stored.next_connection == 0
        {
            return Err("EOD private checkpoint scope/bounds mismatch".into());
        }
        let mut load = PrivateLoad(unhex(&stored.handler_hex)?);
        let transaction = eod::PrivateTimerTransaction::restore_private_timer_transaction(
            &mut load,
            eod::HostIdentity {
                scope: eod::HostScopeId(state.lot_id),
                epoch: state.authority_epoch,
            },
            eod::CheckpointStamp {
                scope: eod::HostScopeId(stored.handler_scope),
                epoch: stored.handler_epoch,
                revision: stored.handler_revision,
            },
            limits(),
        )?;
        let mut result = Self {
            transaction,
            stored,
            committed_sha256: Some(checkpoint.checkpoint_sha256.clone()),
        };
        result.validate_source(state)?;
        // Every old websocket is closed by the controlled recovery workflow.
        // Old private outputs cannot authenticate in the successor epoch. Keep
        // an explicit suppression receipt instead of calling them delivered.
        let recovery_receipt = serde_json::to_vec(marker)?;
        let pending: Vec<_> = result
            .transaction
            .private_deliveries()
            .iter()
            .filter(|d| d.disposition().is_none())
            .filter_map(|d| d.private_recipient().map(|r| (d.id(), r)))
            .collect();
        for (id, recipient) in pending {
            suppress_output(&mut result.transaction, id, recipient, &recovery_receipt)?;
        }
        for index in 0..result.stored.associations.len() {
            let a = result.stored.associations[index].clone();
            if a.closed {
                continue;
            }
            let invoker = eod::InvokerId(u32::try_from(a.source.invoker.object_id.0)?);
            let Some(ticket) = result.transaction.current_ticket(invoker) else {
                return Err("restored active EOD native association missing".into());
            };
            if !result.stored.associations[index]
                .tickets
                .contains(&Ticket::from(ticket))
            {
                result.stored.associations[index]
                    .tickets
                    .push(ticket.into());
            }
            // A retained owned departure authorizes exact server teardown even
            // though its former player transport can no longer authenticate.
            if !is_active(state, &a.owner) {
                if !state
                    .owned_departures
                    .records
                    .get(&a.source.actor)
                    .is_some_and(|r| r.request.control == a.owner)
                {
                    return Err("EOD recovery lacks owner or durable fence".into());
                }
                result.transaction.disconnect_invoker(invoker, ticket)?;
                result.stored.associations[index].closed = true;
            } else {
                let connection = result.allocate_connection()?;
                result.stored.associations[index].connection = connection;
                let authority = auth(&result.stored, state);
                let rebound = result.transaction.rebind(
                    &authority,
                    eod::ConnectionId(connection),
                    ticket.instance_address(),
                    registers(state, a.source.invoker)?,
                )?;
                if !result.stored.associations[index]
                    .tickets
                    .contains(&Ticket::from(rebound))
                {
                    result.stored.associations[index]
                        .tickets
                        .push(rebound.into());
                }
                result.stored.associations[index].next_native_sequence = 1;
            }
        }
        result.assign_public()?;
        Ok(result)
    }
    fn allocate_connection(&mut self) -> Result<u64> {
        let id = self.stored.next_connection;
        self.stored.next_connection = id.checked_add(1).ok_or("EOD connection exhaustion")?;
        Ok(id)
    }
    fn validate_source(&self, state: &SimState) -> Result<()> {
        let mut tickets = std::collections::BTreeSet::new();
        let mut sessions = std::collections::BTreeSet::new();
        for a in &self.stored.associations {
            a.source.validate()?;
            a.owner.validate()?;
            if !sessions.insert(a.source.session_id)
                || a.owner.actor != a.source.actor
                || a.owner.control_generation != a.source.control_generation
                || a.connection == 0
                || a.connection >= self.stored.next_connection
                || a.tickets.is_empty()
                || a.tickets.len() > 128
                || state
                    .eod
                    .sessions
                    .get(&a.source.session_id)
                    .map(|s| s.identity)
                    != Some(a.source)
            {
                return Err("EOD association checkpoint mismatch".into());
            }
            for t in &a.tickets {
                if t.scope != state.lot_id
                    || t.epoch == 0
                    || t.instance == 0
                    || t.generation == 0
                    || !tickets.insert(key(*t))
                {
                    return Err("EOD ticket mapping mismatch".into());
                }
            }
        }
        for s in state.eod.sessions.values() {
            if !sessions.contains(&s.identity.session_id) {
                return Err("EOD public association lacks private checkpoint".into());
            }
        }
        Ok(())
    }
    fn association_for_ticket(&self, t: eod::SessionTicket) -> Result<&Association> {
        self.stored
            .associations
            .iter()
            .find(|a| a.tickets.contains(&Ticket::from(t)))
            .ok_or_else(|| "EOD output lacks immutable source association".into())
    }
    fn assign_public(&mut self) -> Result<()> {
        for d in self.transaction.private_deliveries() {
            if self.stored.public.iter().any(|m| m.output_id == d.id().0) {
                continue;
            }
            let Some(event) = d.public_event() else {
                continue;
            };
            let source = self.association_for_ticket(d.origin_ticket())?.source;
            let (invoker, code, data) = event.source_event();
            if invoker.0 != u32::try_from(source.invoker.object_id.0)? {
                return Err("EOD public invoker mismatches source association".into());
            }
            let event_id = self
                .stored
                .public
                .iter()
                .filter(|m| m.delivery.session == source)
                .count() as u64
                + 1;
            self.stored.public.push(PublicMapping {
                output_id: d.id().0,
                delivery: EodEventDelivery {
                    session: source,
                    event_id,
                    code,
                    data,
                },
            });
        }
        Ok(())
    }
    pub(crate) fn committed_digest(&self) -> Result<&str> {
        self.committed_sha256
            .as_deref()
            .ok_or_else(|| "paired EOD checkpoint has not committed".into())
    }
    pub(crate) fn owned_disconnect_disposition(
        &self,
        state: &SimState,
        owner: &OwnedControl,
    ) -> Result<Value> {
        self.validate_source(state)?;
        if !state.owned_departures.eod_known_clear
            || !state
                .owned_departures
                .eod_disconnect_requests
                .contains(&(owner.actor, owner.control_generation))
        {
            return Err("historical EOD coverage or exact disconnect request missing".into());
        }
        if state.eod.actor_teardown_pending(owner.actor) {
            return Err("source EOD session or public FIFO pending".into());
        }
        let mut ids = vec![];
        for a in self
            .stored
            .associations
            .iter()
            .filter(|a| a.source.actor == owner.actor)
        {
            if &a.owner != owner || !a.closed {
                return Err("native EOD exact owner/session not terminal".into());
            }
            if self.transaction.private_deliveries().iter().any(|d| {
                a.tickets.contains(&Ticket::from(d.origin_ticket())) && d.disposition().is_none()
            }) {
                return Err("native EOD public/private delivery pending".into());
            }
            ids.push(DecimalId(a.source.session_id));
        }
        Ok(
            json!({"disposition":if ids.is_empty(){"complete_known_clear"}else{"exact_sessions_terminal"},"session_ids":ids}),
        )
    }
    pub(crate) fn pending_commands(&self) -> Vec<AcceptedCommand> {
        self.stored
            .public
            .iter()
            .filter(|m| {
                self.transaction
                    .private_deliveries()
                    .iter()
                    .any(|d| d.id().0 == m.output_id && d.disposition().is_none())
            })
            .map(|m| AcceptedCommand::EodEventDelivered(m.delivery.clone()))
            .collect()
    }
    pub(crate) fn before_tick(&mut self, runtime: &GameRuntime) -> Result<()> {
        let state = runtime.sim().state();
        let source = Registers(
            self.stored
                .associations
                .iter()
                .filter(|a| !a.closed)
                .map(|a| {
                    Ok((
                        eod::InvokerId(u32::try_from(a.source.invoker.object_id.0)?),
                        registers(state, a.source.invoker)?,
                    ))
                })
                .collect::<Result<_>>()?,
        );
        let authority = auth(&self.stored, state);
        self.transaction.tick(&authority, &source)?;
        for a in &mut self.stored.associations {
            if !a.closed
                && self
                    .transaction
                    .current_ticket(eod::InvokerId(a.source.invoker.object_id.0 as u32))
                    .is_none()
            {
                a.closed = true
            }
        }
        self.assign_public()?;
        Ok(())
    }
    pub(crate) fn after_tick(
        &mut self,
        runtime: &GameRuntime,
        events: &[RuntimeEvent],
    ) -> Result<()> {
        let state = runtime.sim().state();
        // Public input was applied to the candidate. These receipts become
        // authoritative only together with its eventual coordinator commit.
        for mapping in &self.stored.public {
            let Some(output) = self
                .transaction
                .private_deliveries()
                .iter()
                .find(|d| d.id().0 == mapping.output_id && d.disposition().is_none())
            else {
                continue;
            };
            let origin = output.origin_ticket();
            if let Some((attached, tick)) = events.iter().find_map(|event| match event {
                RuntimeEvent::EodEventApplied {
                    delivery,
                    attached,
                    tick,
                } if delivery == &mapping.delivery => Some((*attached, *tick)),
                _ => None,
            }) {
                if state.eod.delivered.get(&(
                    mapping.delivery.session.session_id,
                    mapping.delivery.event_id,
                )) != Some(&mapping.delivery)
                {
                    return Err("EOD applied outcome lacks exact source receipt".into());
                }
                let proof = serde_json::to_vec(
                    &json!({"accepted_tick":tick,"attached_at_application":attached,"delivery":mapping.delivery}),
                )?;
                settle_public_output(
                    &mut self.transaction,
                    eod::PrivateTimerOutputId(mapping.output_id),
                    origin,
                    !attached,
                    &proof,
                )?;
            }
        }
        for event in events {
            let (session, id, tick, body) = match event {
                RuntimeEvent::EodConnectRequested {
                    session,
                    request_id,
                    tick,
                    temps,
                } => (
                    *session,
                    *request_id,
                    *tick,
                    json!({"kind":"connect","session":session,"request_id":request_id,"tick":tick,"temps":temps}),
                ),
                RuntimeEvent::EodDisconnectRequested {
                    session,
                    request_id,
                    tick,
                } => (
                    *session,
                    *request_id,
                    *tick,
                    json!({"kind":"disconnect","session":session,"request_id":request_id,"tick":tick}),
                ),
                RuntimeEvent::EodSimanticsRequested {
                    session,
                    request_id,
                    tick,
                    code,
                    temps,
                } => (
                    *session,
                    *request_id,
                    *tick,
                    json!({"kind":"simantics","session":session,"request_id":request_id,"tick":tick,"code":code,"temps":temps}),
                ),
                _ => continue,
            };
            if tick != state.completed_tick || id == 0 {
                return Err("EOD request tick/identity mismatch".into());
            }
            let digest = Digest::of(&serde_json::to_vec(&body)?).as_str().to_owned();
            if let Some(old) = self.stored.requests.iter().find(|r| r.id == id) {
                if old.sha256 != digest {
                    return Err("EOD source request identity reused".into());
                }
                continue;
            }
            if self.stored.requests.len() >= 4096 {
                return Err("EOD source receipt capacity".into());
            }
            match event {
                RuntimeEvent::EodConnectRequested { temps, .. } => {
                    if self.stored.associations.len() >= 64 {
                        return Err("EOD association retention capacity".into());
                    }
                    let control = owner(state, session)?;
                    let connection = self.allocate_connection()?;
                    let authority = Auth(
                        [(
                            eod::ConnectionId(connection),
                            eod::ActorId(u64::from(control.avatar_id.0)),
                        )]
                        .into(),
                    );
                    let ticket = self.transaction.connect(
                        &authority,
                        eod::ConnectRequest {
                            connection: eod::ConnectionId(connection),
                            plugin: eod::PluginId(session.plugin_id),
                            object: u32::try_from(session.object.object_id.0)?,
                            invoker: eod::InvokerId(u32::try_from(session.invoker.object_id.0)?),
                            registers: eod::TimerRegisters([
                                temps[0], temps[1], temps[2], temps[3],
                            ]),
                        },
                    )?;
                    self.stored.associations.push(Association {
                        source: session,
                        owner: control,
                        tickets: vec![ticket.into()],
                        connection,
                        next_native_sequence: 1,
                        closed: false,
                    });
                }
                RuntimeEvent::EodDisconnectRequested { .. } => {
                    let a = self
                        .stored
                        .associations
                        .iter_mut()
                        .find(|a| a.source == session)
                        .ok_or("EOD close association missing")?;
                    if !a.closed {
                        let ticket = *a.tickets.last().ok_or("EOD close ticket missing")?;
                        self.transaction.disconnect_invoker(
                            eod::InvokerId(u32::try_from(session.invoker.object_id.0)?),
                            ticket.into(),
                        )?;
                        a.closed = true;
                    } else if !self.transaction.private_deliveries().iter().any(|d| {
                        a.tickets.contains(&Ticket::from(d.origin_ticket()))
                            && matches!(
                                d.public_event(),
                                Some(eod::PublicVmEvent::Disconnected { .. })
                            )
                    }) {
                        return Err("EOD already-closed lacks retained source effect".into());
                    }
                }
                RuntimeEvent::EodSimanticsRequested { .. } => {
                    // Original Timer registers no SimanticsHandlers. The
                    // authoritative received event therefore has no effect.
                    if !self.stored.associations.iter().any(|a| a.source == session) {
                        return Err("EOD Simantics association missing".into());
                    }
                }
                _ => unreachable!(),
            }
            self.stored
                .requests
                .push(RequestReceipt { id, sha256: digest });
            self.assign_public()?;
        }
        // Revoked owned sessions cannot receive UI. A retained source departure
        // proves the exact fence; absence or an unrelated replacement does not.
        let revoke: Vec<_> = self
            .transaction
            .private_deliveries()
            .iter()
            .filter(|d| d.disposition().is_none())
            .filter_map(|d| {
                d.private_recipient()
                    .map(|r| (d.id(), r, d.origin_ticket()))
            })
            .collect();
        for (id, recipient, ticket) in revoke {
            let a = self.association_for_ticket(ticket)?;
            if state
                .owned_departures
                .records
                .get(&a.source.actor)
                .is_some_and(|r| r.request.control == a.owner)
            {
                let proof = serde_json::to_vec(
                    &json!({"reason":"owned_departure_fenced","session":a.source,"tick":state.completed_tick}),
                )?;
                suppress_output(&mut self.transaction, id, recipient, &proof)?;
            }
        }
        self.validate_source(state)?;
        Ok(())
    }
    pub(crate) fn prepare_checkpoint(&mut self) -> Result<NativeEodCandidateV1> {
        let mut sink = PrivateStage { bytes: vec![] };
        let stamp = self.transaction.stage_private_transaction(&mut sink)?;
        let before = self.stored.revision;
        self.stored.revision = before.checked_add(1).ok_or("EOD revision exhausted")?;
        self.stored.handler_scope = stamp.scope.0;
        self.stored.handler_epoch = stamp.epoch;
        self.stored.handler_revision = stamp.revision;
        self.stored.handler_hex = hex(&sink.bytes);
        let bytes = serde_json::to_vec(&self.stored)?;
        if bytes.len() > EOD_PRIVATE_CHECKPOINT_LIMIT {
            return Err("EOD private envelope capacity".into());
        }
        let digest = Digest::of(&bytes).as_str().to_owned();
        let candidate = NativeEodCandidateV1 {
            schema_version: 1,
            revision_before: DecimalId(before),
            revision_after: DecimalId(self.stored.revision),
            predecessor_sha256: self.committed_sha256.clone(),
            checkpoint_sha256: digest.clone(),
            checkpoint_hex: hex(&bytes),
        };
        candidate.validate()?;
        self.committed_sha256 = Some(digest);
        Ok(candidate)
    }
    pub(crate) fn ui(
        &mut self,
        runtime: &GameRuntime,
        control: &OwnedControlV2,
        message: &NativeEodUiV1,
    ) -> Result<()> {
        message.validate()?;
        let state = runtime.sim().state();
        let index = self
            .stored
            .associations
            .iter()
            .position(|a| a.source.session_id == message.source_session_id.0)
            .ok_or("EOD UI source session missing")?;
        let a = self.stored.associations[index].clone();
        self.check_control(state, &a, control)?;
        let sha = Digest::of(&serde_json::to_vec(message)?)
            .as_str()
            .to_owned();
        if let Some(old) = self
            .stored
            .ui_receipts
            .iter()
            .find(|r| r.session == a.source.session_id && r.sequence == message.sequence.0)
        {
            return if old.sha256 == sha {
                Ok(())
            } else {
                Err("EOD UI sequence identity reused".into())
            };
        }
        if a.closed || self.stored.ui_receipts.len() >= 4096 {
            return Err("EOD UI session closed or receipt capacity".into());
        }
        let expected = self
            .stored
            .ui_receipts
            .iter()
            .filter(|r| r.session == a.source.session_id)
            .count() as u64
            + 1;
        if message.sequence.0 != expected {
            return Err("EOD UI source sequence mismatch".into());
        }
        let bytes = unhex(&message.payload_hex)?;
        let authority = auth(&self.stored, state);
        let payload = if message.binary {
            eod::WirePayload::Binary(&bytes)
        } else {
            eod::WirePayload::Text(std::str::from_utf8(&bytes)?)
        };
        let outcome = self.transaction.receive(
            &authority,
            eod::ConnectionId(a.connection),
            eod::ClientMessage {
                version: 1,
                ticket: (*a.tickets.last().ok_or("EOD ticket missing")?).into(),
                plugin: eod::PluginId(a.source.plugin_id),
                sequence: a.next_native_sequence,
                event: &message.event,
                payload,
            },
        )?;
        self.stored.associations[index].next_native_sequence = a
            .next_native_sequence
            .checked_add(1)
            .ok_or("EOD sequence exhausted")?;
        if outcome == eod::DispatchOutcome::Closed {
            self.stored.associations[index].closed = true
        }
        self.stored.ui_receipts.push(UiReceipt {
            session: a.source.session_id,
            sequence: message.sequence.0,
            sha256: sha,
        });
        self.assign_public()?;
        Ok(())
    }
    fn check_control(&self, state: &SimState, a: &Association, c: &OwnedControlV2) -> Result<()> {
        if !is_active(state, &a.owner)
            || c.principal.0 != a.owner.principal
            || c.avatar_id.0 != u64::from(a.owner.avatar_id.0)
            || c.control_generation.0 != a.owner.control_generation
            || c.session_ref != hex(&a.owner.session_ref)
        {
            return Err("EOD authenticated owner mismatch".into());
        }
        Ok(())
    }
    pub(crate) fn private_outputs(
        &self,
        runtime: &GameRuntime,
        control: &OwnedControlV2,
    ) -> Result<Value> {
        let state = runtime.sim().state();
        let mut messages = vec![];
        for d in self.transaction.private_deliveries() {
            if d.disposition().is_some() {
                continue;
            }
            let Some(message) = d.private_message() else {
                continue;
            };
            let a = self.association_for_ticket(d.origin_ticket())?;
            if self.check_control(state, a, control).is_err() {
                continue;
            }
            let (binary, bytes) = match message.body() {
                eod::UiBody::Binary(v) => (true, v),
                eod::UiBody::Text(v) => (false, v.as_bytes()),
            };
            let payload = hex(bytes);
            messages.push(json!({"source_session_id":DecimalId(a.source.session_id),"output_id":DecimalId(d.id().0),"plugin_id":a.source.plugin_id,"event":message.event(),"binary":binary,"payload_hex":payload,"sha256":private_digest(a.source.session_id,d.id().0,message.event(),binary,&payload)}));
        }
        let mut source_sessions = vec![];
        for a in &self.stored.associations {
            if self.check_control(state, a, control).is_err()
                || (a.closed
                    && !messages
                        .iter()
                        .any(|m| m["source_session_id"] == json!(DecimalId(a.source.session_id))))
            {
                continue;
            }
            let (next, last) = ui_cursor(&self.stored.ui_receipts, a.source.session_id);
            source_sessions.push(json!({
                "source_session_id":DecimalId(a.source.session_id),"plugin_id":a.source.plugin_id,"closed":a.closed,
                "next_ui_sequence":DecimalId(next),
                "last_ui_receipt":last.map(|r|json!({"sequence":DecimalId(r.sequence),"sha256":r.sha256}))
            }));
        }
        Ok(
            json!({"kind":"private_eod_outputs","messages":messages,"source_sessions":source_sessions}),
        )
    }
    pub(crate) fn acknowledge(
        &mut self,
        runtime: &GameRuntime,
        control: &OwnedControlV2,
        session: u64,
        id: u64,
        digest: &str,
    ) -> Result<()> {
        let d = self
            .transaction
            .private_deliveries()
            .iter()
            .find(|d| d.id().0 == id)
            .ok_or("EOD output missing")?;
        let message = d.private_message().ok_or("EOD output is not private")?;
        let a = self.association_for_ticket(d.origin_ticket())?;
        self.check_control(runtime.sim().state(), a, control)?;
        let (binary, bytes) = match message.body() {
            eod::UiBody::Binary(v) => (true, v),
            eod::UiBody::Text(v) => (false, v.as_bytes()),
        };
        if session != a.source.session_id
            || private_digest(session, id, message.event(), binary, &hex(bytes)) != digest
        {
            return Err("EOD output acknowledgement mismatch".into());
        }
        let recipient = d
            .private_recipient()
            .ok_or("EOD output recipient missing")?;
        let authority = auth(&self.stored, runtime.sim().state());
        let receipt = serde_json::to_vec(
            &json!({"source_session_id":DecimalId(session),"output_id":DecimalId(id),"sha256":digest}),
        )?;
        self.transaction.acknowledge_private(
            &authority,
            recipient.connection,
            recipient.ticket,
            eod::PrivateTimerOutputId(id),
            id,
            &receipt,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_ui_cursor_retains_only_the_requested_source_session_sequence() {
        let receipts = vec![
            UiReceipt {
                session: 8,
                sequence: 1,
                sha256: "a".repeat(64),
            },
            UiReceipt {
                session: 9,
                sequence: 1,
                sha256: "b".repeat(64),
            },
            UiReceipt {
                session: 8,
                sequence: 2,
                sha256: "c".repeat(64),
            },
        ];
        let (next, last) = ui_cursor(&receipts, 8);
        assert_eq!(next, 3);
        assert_eq!(last.unwrap().sha256, "c".repeat(64));
        let (next, last) = ui_cursor(&receipts, 9);
        assert_eq!(next, 2);
        assert_eq!(last.unwrap().sha256, "b".repeat(64));
        let (next, last) = ui_cursor(&receipts, 10);
        assert_eq!(next, 1);
        assert!(last.is_none());
    }
    fn connected() -> (eod::PrivateTimerTransaction, Auth, eod::SessionTicket) {
        let authority = Auth([(eod::ConnectionId(1), eod::ActorId(9))].into());
        let mut tx = eod::NativeHost::new(
            eod::HostIdentity {
                scope: eod::HostScopeId(7),
                epoch: 1,
            },
            limits(),
        )
        .unwrap()
        .fork_private_timer_transaction()
        .unwrap();
        let ticket = tx
            .connect(
                &authority,
                eod::ConnectRequest {
                    connection: eod::ConnectionId(1),
                    plugin: eod::PluginId(0xaa65fe9e),
                    object: 2,
                    invoker: eod::InvokerId(3),
                    registers: eod::TimerRegisters([0, 1, 0, 0]),
                },
            )
            .unwrap();
        (tx, authority, ticket)
    }
    #[test]
    fn generic_disconnect_requires_known_coverage_and_every_exact_output_terminal() {
        use wonderland_game_runtime::sim_core::{
            eod::EodSession,
            state::{ContentSet, TuningSet},
            vm::RoutineStore,
        };
        use wonderland_game_runtime::{
            LotModel, ObjectId, PersistentId, RuntimeConfig, RuntimeRole, VmMode,
        };
        let runtime = GameRuntime::new(
            ContentSet::new(
                RoutineStore::default(),
                vec![],
                vec![],
                TuningSet::default(),
            )
            .unwrap(),
            LotModel::new(8, 8, 1).unwrap(),
            RuntimeConfig::new(VmMode::Tso, 7, 1, 1),
            RuntimeRole::Authority,
        )
        .unwrap();
        let mut state = runtime.sim().state().clone();
        let entity = |id| EntityRef {
            object_id: ObjectId(id),
            generation: 1,
        };
        let owner = OwnedControl {
            principal: 1,
            account_id: 1,
            avatar_id: PersistentId(9),
            actor: entity(9),
            control_generation: 1,
            session_ref: [1; 32],
            admission_operation: "test_admission_0001".into(),
            reservation_digest: [2; 32],
            export_digest: [3; 32],
        };
        state
            .owned_departures
            .eod_disconnect_requests
            .insert((owner.actor, 1));
        let mut adapter = TimerAdapter::fresh(&runtime).unwrap();
        assert_eq!(
            adapter
                .owned_disconnect_disposition(&state, &owner)
                .unwrap()["disposition"],
            "complete_known_clear"
        );
        state.owned_departures.eod_known_clear = false;
        assert!(
            adapter
                .owned_disconnect_disposition(&state, &owner)
                .is_err()
        );
        state.owned_departures.eod_known_clear = true;
        let source = EodSessionIdentity {
            session_id: 1,
            thread: entity(3),
            invoker: entity(3),
            actor: owner.actor,
            object: entity(2),
            control_generation: 1,
            plugin_id: 0xaa65fe9e,
        };
        state.eod.sessions.insert(
            1,
            EodSession {
                identity: source,
                created_tick: 1,
                connect_request_id: 1,
                connect_temps: [0; 20],
                disconnect_request: None,
                source_detached_tick: None,
                next_event_id: 1,
                ended: false,
                events: Default::default(),
            },
        );
        assert!(
            adapter
                .owned_disconnect_disposition(&state, &owner)
                .is_err()
        );
        let (tx, _, ticket) = connected();
        adapter.transaction = tx;
        adapter.stored.next_connection = 2;
        adapter.stored.associations.push(Association {
            source,
            owner: owner.clone(),
            tickets: vec![ticket.into()],
            connection: 1,
            next_native_sequence: 1,
            closed: false,
        });
        assert!(
            adapter
                .owned_disconnect_disposition(&state, &owner)
                .is_err()
        );
        adapter
            .transaction
            .disconnect_invoker(eod::InvokerId(3), ticket)
            .unwrap();
        adapter.stored.associations[0].closed = true;
        state.eod.sessions.get_mut(&1).unwrap().ended = true;
        assert!(
            adapter
                .owned_disconnect_disposition(&state, &owner)
                .is_err()
        );
        let outputs: Vec<_> = adapter
            .transaction
            .private_deliveries()
            .iter()
            .map(|d| (d.id(), d.origin_ticket(), d.private_recipient()))
            .collect();
        for (id, origin, recipient) in outputs {
            if let Some(recipient) = recipient {
                suppress_output(
                    &mut adapter.transaction,
                    id,
                    recipient,
                    b"exact test owner revocation",
                )
                .unwrap();
            } else {
                settle_public_output(
                    &mut adapter.transaction,
                    id,
                    origin,
                    false,
                    b"exact test accepted public event",
                )
                .unwrap();
            }
        }
        assert_eq!(
            adapter
                .owned_disconnect_disposition(&state, &owner)
                .unwrap()["disposition"],
            "exact_sessions_terminal"
        );
        let mut stale = owner;
        stale.session_ref = [9; 32];
        assert!(
            adapter
                .owned_disconnect_disposition(&state, &stale)
                .is_err()
        );
    }
    #[test]
    fn recovery_suppresses_multiple_outputs_in_one_epoch_with_unique_output_keys() {
        let (mut tx, _, _) = connected();
        let mut sink = PrivateStage { bytes: vec![] };
        let stamp = tx.stage_private_transaction(&mut sink).unwrap();
        let mut restored = eod::PrivateTimerTransaction::restore_private_timer_transaction(
            &mut PrivateLoad(sink.bytes),
            eod::HostIdentity {
                scope: eod::HostScopeId(7),
                epoch: 2,
            },
            stamp,
            limits(),
        )
        .unwrap();
        let private: Vec<_> = restored
            .private_deliveries()
            .iter()
            .filter_map(|d| d.private_recipient().map(|r| (d.id(), r)))
            .collect();
        assert_eq!(private.len(), 2);
        let same_epoch_proof = br#"{"test_takeover_epoch":2}"#;
        for (id, recipient) in private {
            suppress_output(&mut restored, id, recipient, same_epoch_proof).unwrap();
        }
        let settled: Vec<_> = restored
            .private_deliveries()
            .iter()
            .filter(|d| d.private_message().is_some())
            .collect();
        assert!(settled.iter().all(|d| d.disposition().unwrap().kind()
            == eod::PrivateTimerDispositionKind::Suppressed
            && d.disposition().unwrap().receipt_key() == d.id().0));
    }
    #[test]
    fn two_public_effects_in_same_tick_have_distinct_receipt_keys() {
        let (mut tx, authority, ticket) = connected();
        tx.receive(
            &authority,
            eod::ConnectionId(1),
            eod::ClientMessage {
                version: 1,
                ticket,
                plugin: eod::PluginId(0xaa65fe9e),
                sequence: 1,
                event: "Timer_Set",
                payload: eod::WirePayload::Binary(&[1, 7]),
            },
        )
        .unwrap();
        let public: Vec<_> = tx
            .private_deliveries()
            .iter()
            .filter(|d| d.public_event().is_some())
            .map(|d| (d.id(), d.origin_ticket()))
            .collect();
        assert_eq!(public.len(), 2);
        let same_tick_proof = br#"{"test_accepted_tick":8}"#;
        for (id, origin) in public {
            settle_public_output(&mut tx, id, origin, false, same_tick_proof).unwrap();
        }
        assert!(
            tx.private_deliveries()
                .iter()
                .filter(|d| d.public_event().is_some())
                .all(|d| d.disposition().unwrap().receipt_key() == d.id().0)
        );
    }
}
