use crate::Result;
use bincode::Options;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    AcceptedCommand, GameRuntime, RuntimeConfig, RuntimeRole,
    live_session::TickFrame,
    live_wire::{WireLimits, encode_ticks},
};
use wonderland_native_host_contracts::{
    CursorV1, DecimalId, DecisionV1, HostBindingV1, ReceiptCheckpointV1, RequestV1,
    TakeoverMarkerV1,
};

const MAX_COMMAND_BYTES: usize = 256 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
const MAX_HISTORY: usize = 4096;
const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECEIPT_EXPORT: usize = 64;
const MAX_RECEIPT_EXPORT_BYTES: usize = 16 * 1024 * 1024;
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(&mut out, "{b:02x}").expect("string output");
    }
    out
}
pub(crate) fn unhex(input: &str) -> Result<Vec<u8>> {
    if input.len() > MAX_COMMAND_BYTES * 2
        || !input.len().is_multiple_of(2)
        || !input
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid bounded command encoding".into());
    }
    input
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}
pub(crate) fn cursor(runtime: &GameRuntime) -> Result<CursorV1> {
    let s = runtime.sim().state();
    Ok(CursorV1 {
        lot_id: DecimalId(s.lot_id),
        authority_epoch: DecimalId(s.authority_epoch),
        completed_tick: DecimalId(s.completed_tick),
        state_hash: hex(&runtime.sim().state_hash()?),
    })
}
pub(crate) fn checkpoint(runtime: &GameRuntime) -> Result<Vec<u8>> {
    let bytes = runtime.snapshot()?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("host checkpoint exceeds 8 MiB profile".into());
    }
    Ok(bytes)
}
struct Pending {
    id: u64,
    input_hash: Digest,
    digest: String,
    result: CursorV1,
    runtime: GameRuntime,
    proposal: Value,
    action: Option<Vec<u8>>,
}
struct Recorded {
    input_hash: Digest,
    digest: String,
    reply: Value,
}
pub struct Host {
    pub(crate) committed: GameRuntime,
    pub(crate) config: RuntimeConfig,
    pub(crate) binding: HostBindingV1,
    report: Value,
    takeover_marker: Option<TakeoverMarkerV1>,
    pub(crate) recovery_committed: bool,
    pub(crate) appearance: Option<wonderland_game_runtime::world_view::WorldDocument>,
    pending: Option<Pending>,
    history: BTreeMap<u64, Recorded>,
    history_bytes: usize,
    retired_through: u64,
    last_receipt_ack: Option<ReceiptCheckpointV1>,
}
impl Host {
    pub fn new(
        runtime: GameRuntime,
        config: RuntimeConfig,
        binding: HostBindingV1,
        report: Value,
        takeover_marker: Option<TakeoverMarkerV1>,
    ) -> Result<Self> {
        checkpoint(&runtime)?;
        Ok(Self {
            committed: runtime,
            appearance: None,
            config,
            binding,
            report,
            recovery_committed: takeover_marker.is_none(),
            takeover_marker,
            pending: None,
            history: BTreeMap::new(),
            history_bytes: 0,
            retired_through: 0,
            last_receipt_ack: None,
        })
    }
    pub fn handle(&mut self, request: RequestV1) -> Result<Value> {
        match request {
            RequestV1::Inspect => Ok(
                json!({"kind":if self.recovery_committed {"committed_state"} else {"recovery_candidate"},"recovery_pending":!self.recovery_committed,"binding":self.binding,"cursor":cursor(&self.committed)?,"runtime_content":self.committed.sim().state().content,"sealed_release":self.report,"takeover_marker":self.takeover_marker,"checkpoint_hex":hex(&checkpoint(&self.committed)?),"pending_batch":self.pending.as_ref().map(|p|DecimalId(p.id)),"retired_through_batch_id":DecimalId(self.retired_through),"retained_receipts":self.history.len(),"retained_receipt_bytes":self.history_bytes,"admission_ready":self.committed.sim().state().owned_admission.actors().is_some(),"source_lot_location":crate::player::source_location(&self.committed),"permission_binding":crate::player::permission(&self.committed)?,"actors":crate::player::actors(&self.committed),"actor_registry_sha256":crate::player::registry_hash(&self.committed)?}),
            ),
            RequestV1::PlayerBootstrap { control, binding } => {
                self.player_bootstrap(control, binding)
            }
            RequestV1::PlayerCheckpoint {
                control,
                request_hex,
            } => self.player_checkpoint(control, request_hex),
            RequestV1::PrepareOwnedAdmission {
                batch_id,
                binding,
                predecessor,
                expected_control_revision,
                principal,
                export,
            } => {
                let input = json!({"kind":"prepare_owned_admission","batch_id":batch_id,"binding":binding,"predecessor":predecessor,"expected_control_revision":expected_control_revision,"principal":principal,"export":export});
                if let Some(reply) = self.typed_prior(batch_id, &binding, &predecessor, &input)? {
                    return Ok(reply);
                }
                let command = self.admission_command(principal, &export)?;
                self.prepare_typed(
                    batch_id,
                    binding,
                    predecessor,
                    expected_control_revision,
                    input,
                    vec![command],
                    true,
                    None,
                )
            }
            RequestV1::PreparePlayerTick {
                batch_id,
                binding,
                predecessor,
                expected_control_revision,
            } => {
                let input = json!({"kind":"prepare_player_tick","batch_id":batch_id,"binding":binding,"predecessor":predecessor,"expected_control_revision":expected_control_revision});
                if let Some(reply) = self.typed_prior(batch_id, &binding, &predecessor, &input)? {
                    return Ok(reply);
                }
                self.prepare_typed(
                    batch_id,
                    binding,
                    predecessor,
                    expected_control_revision,
                    input,
                    vec![],
                    false,
                    None,
                )
            }
            RequestV1::PreparePlayerAction {
                batch_id,
                binding,
                predecessor,
                expected_control_revision,
                control,
                action_hex,
            } => {
                let input = json!({"kind":"prepare_player_action","batch_id":batch_id,"binding":binding,"predecessor":predecessor,"expected_control_revision":expected_control_revision,"control":control,"action_hex":action_hex});
                if let Some(reply) = self.typed_prior(batch_id, &binding, &predecessor, &input)? {
                    return Ok(reply);
                }
                let bytes = unhex(&action_hex)?;
                wonderland_game_runtime::live_wire::player::decode_action(&bytes)?;
                let result = self.action_command(&control, &bytes).and_then(|command| {
                    self.prepare_typed(
                        batch_id,
                        binding,
                        predecessor,
                        expected_control_revision,
                        input,
                        vec![command],
                        false,
                        Some(bytes.clone()),
                    )
                });
                match result {
                    Ok(reply) => Ok(reply),
                    Err(error) => Ok(
                        json!({"kind":"denied","reason":error.to_string(),"receipt_hex":action_receipt(bytes,None)?}),
                    ),
                }
            }
            RequestV1::ActivateRecovery { marker } => {
                if self.takeover_marker.as_ref() != Some(&marker) {
                    return Err("recovery activation marker mismatch".into());
                }
                self.recovery_committed = true;
                self.handle(RequestV1::Inspect)
            }
            RequestV1::Lookup { batch_id } => {
                if let Some(reply) = self.retired(batch_id) {
                    return Ok(reply);
                }
                if let Some(p) = &self.pending
                    && p.id == batch_id.0
                {
                    return Ok(p.proposal.clone());
                }
                Ok(self
                    .history
                    .get(&batch_id.0)
                    .map(|r| r.reply.clone())
                    .unwrap_or(json!({"kind":"not_found","batch_id":batch_id})))
            }
            RequestV1::ExportReceipts => self.export_receipts(),
            RequestV1::AcknowledgeReceipts { acknowledgement } => {
                self.acknowledge_receipts(acknowledgement)
            }
            RequestV1::Prepare {
                batch_id,
                binding,
                predecessor,
                commands_hex,
            } => {
                if !self.recovery_committed {
                    return Err("replacement awaits durable recovery activation".into());
                }
                if self
                    .committed
                    .sim()
                    .state()
                    .owned_admission
                    .actors()
                    .is_some()
                {
                    return Err("controlled lot refuses raw supervisor commands".into());
                }
                self.prepare(batch_id, binding, predecessor, commands_hex)
            }
            RequestV1::Resolve {
                batch_id,
                digest,
                decision,
            } => self.resolve(batch_id, digest, decision),
        }
    }
    fn prepare(
        &mut self,
        id: DecimalId,
        binding: HostBindingV1,
        predecessor: CursorV1,
        commands_hex: String,
    ) -> Result<Value> {
        if id.0 == 0 || binding != self.binding {
            return Err("host binding or batch identity mismatch".into());
        }
        let input_hash = Digest::of(&serde_json::to_vec(
            &json!({"binding":binding,"predecessor":predecessor,"commands_hex":commands_hex}),
        )?);
        if let Some(r) = self.history.get(&id.0) {
            if r.input_hash != input_hash {
                return Err("batch ID reused with different input".into());
            }
            return Ok(r.reply.clone());
        }
        if let Some(mut reply) = self.retired(id) {
            reply["input_sha256"] = json!(input_hash);
            return Ok(reply);
        }
        if let Some(p) = &self.pending {
            if p.id != id.0 {
                return Err("pending batch must be resolved first".into());
            }
            if p.input_hash != input_hash {
                return Err("pending batch input differs".into());
            }
            return Ok(p.proposal.clone());
        }
        if self.history.len() >= MAX_HISTORY {
            return Err("receipt history full; durable checkpoint acknowledgement required".into());
        }
        if predecessor != cursor(&self.committed)? {
            return Err("stale predecessor".into());
        }
        let bytes = unhex(&commands_hex)?;
        let commands: crate::bounded::BoundedVec<
            crate::bounded::BoundedVec<AcceptedCommand, 128>,
            16,
        > = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(MAX_COMMAND_BYTES as u64)
            .reject_trailing_bytes()
            .deserialize(&bytes)?;
        if commands.0.is_empty() {
            return Err("batch command/tick bounds".into());
        }
        let mut candidate = GameRuntime::new(
            self.committed.sim().content().clone(),
            self.committed.sim().state().world.lot.clone(),
            self.config.clone(),
            RuntimeRole::Authority,
        )?;
        candidate.restore(&checkpoint(&self.committed)?)?;
        let mut frames = Vec::new();
        let mut effects = Vec::new();
        let mut supervisor_events = Vec::new();
        for commands in commands.0 {
            let accepted = candidate.sim().next_tick(commands.0)?;
            let outcome = candidate.apply_accepted(&accepted)?;
            effects.extend(outcome.effects);
            crate::events::retain(outcome.tick, outcome.events, &mut supervisor_events);
            frames.push(TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            });
        }
        let snapshot = checkpoint(&candidate)?;
        let result = cursor(&candidate)?;
        let wire = encode_ticks(&frames, WireLimits::default())?;
        let effects_bytes = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(4 * 1024 * 1024)
            .serialize(&effects)?;
        let event_bytes = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(4 * 1024 * 1024)
            .serialize(&supervisor_events)?;
        let payload = json!({"binding":self.binding,"batch_id":id,"predecessor":predecessor,"result":result,"ticks_hex":hex(&wire),"checkpoint_sha256":Digest::of(&snapshot),"effects_hex":hex(&effects_bytes),"events_hex":hex(&event_bytes)});
        let encoded = serde_json::to_vec(&payload)?;
        // Profile applies to the actual serialized supervisor message, not estimates.
        if encoded.len() > 16 * 1024 * 1024
            || self.history_bytes + encoded.len() + 1024 > MAX_HISTORY_BYTES
        {
            return Err("proposal exceeds host profile".into());
        }
        let digest = Digest::of(&encoded).as_str().to_owned();
        let proposal =
            json!({"kind":"proposal","status":"pending","digest":digest,"payload":payload});
        self.pending = Some(Pending {
            id: id.0,
            input_hash,
            digest,
            result,
            runtime: candidate,
            proposal: proposal.clone(),
            action: None,
        });
        Ok(proposal)
    }
    fn typed_prior(
        &self,
        id: DecimalId,
        binding: &HostBindingV1,
        predecessor: &CursorV1,
        input: &Value,
    ) -> Result<Option<Value>> {
        if !self.recovery_committed || binding != &self.binding || id.0 == 0 {
            return Err("typed prepare identity/recovery mismatch".into());
        }
        let digest = Digest::of(&serde_json::to_vec(input)?);
        if let Some(record) = self.history.get(&id.0) {
            if record.input_hash != digest {
                return Err("batch ID reused with different typed input".into());
            }
            return Ok(Some(record.reply.clone()));
        }
        if let Some(mut reply) = self.retired(id) {
            reply["input_sha256"] = json!(digest);
            return Ok(Some(reply));
        }
        if let Some(pending) = &self.pending {
            if pending.id != id.0 || pending.input_hash != digest {
                return Err("pending typed batch differs".into());
            }
            return Ok(Some(pending.proposal.clone()));
        }
        if predecessor != &cursor(&self.committed)? {
            return Err("stale typed predecessor".into());
        }
        if self
            .committed
            .sim()
            .state()
            .owned_admission
            .actors()
            .is_none()
        {
            return Err("source admission uninitialized".into());
        }
        Ok(None)
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_typed(
        &mut self,
        id: DecimalId,
        binding: HostBindingV1,
        predecessor: CursorV1,
        revision: DecimalId,
        input: Value,
        commands: Vec<AcceptedCommand>,
        admission: bool,
        action: Option<Vec<u8>>,
    ) -> Result<Value> {
        let after = if admission {
            revision
                .0
                .checked_add(1)
                .ok_or("control revision exhausted")?
        } else {
            revision.0
        };
        let command_bytes = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(MAX_COMMAND_BYTES as u64)
            .serialize(&vec![commands])?;
        self.prepare(id, binding, predecessor, hex(&command_bytes))?;
        let mut p = self.pending.take().ok_or("prepared candidate missing")?;
        p.proposal["payload"]["control_v2"] = json!({"schema_version":2,"revision_before":revision,"revision_after":DecimalId(after),"actor_registry_before_sha256":crate::player::registry_hash(&self.committed)?,"actor_registry_after_sha256":crate::player::registry_hash(&p.runtime)?,"actors":crate::player::actors(&p.runtime),"permission_before":crate::player::permission(&self.committed)?,"permission_after":crate::player::permission(&p.runtime)?});
        let input_hash = Digest::of(&serde_json::to_vec(&input)?);
        if let Some(bytes) = &action {
            // Both receipt variants are fixed before commit. The coordinator
            // stores the selected one atomically with its durable decision,
            // even if this host disappears before resolve/export can run.
            let request: wonderland_native_host_contracts::OwnedControlV2 =
                serde_json::from_value(input["control"].clone())?;
            let owned = self.owned_control(&request)?;
            let decoded = wonderland_game_runtime::live_wire::player::decode_action(bytes)?;
            p.proposal["payload"]["action_v1"] = json!({
                "schema_version":1,
                "control":{
                    "principal":DecimalId(owned.principal),
                    "account_id":DecimalId(owned.account_id),
                    "avatar_id":DecimalId(u64::from(owned.avatar_id.0)),
                    "control_generation":DecimalId(owned.control_generation),
                    "session_ref":hex(&owned.session_ref)
                },
                "input_sha256":input_hash,
                "request_hex":hex(bytes),
                "request_sha256":Digest::of(bytes),
                "command_sequence":DecimalId(decoded.sequence()),
                "accepted_receipt_hex":action_receipt(bytes.clone(),Some(p.result.completed_tick.0))?,
                "rejected_receipt_hex":action_receipt(bytes.clone(),None)?
            });
        }
        let encoded = serde_json::to_vec(&p.proposal["payload"])?;
        let receipt_credit = action.as_ref().map_or(0, |bytes| 2 * (bytes.len() + 64));
        if encoded.len() > 16 * 1024 * 1024
            || self.history_bytes + encoded.len() + receipt_credit + 1024 > MAX_HISTORY_BYTES
        {
            return Err("controlled proposal exceeds host profile".into());
        }
        p.digest = Digest::of(&encoded).as_str().to_owned();
        p.proposal["digest"] = json!(p.digest);
        p.input_hash = input_hash;
        p.action = action;
        let result = p.proposal.clone();
        self.pending = Some(p);
        Ok(result)
    }
    fn resolve(&mut self, id: DecimalId, digest: String, decision: DecisionV1) -> Result<Value> {
        if let Some(r) = self.history.get(&id.0) {
            if r.digest != digest {
                return Err("recorded digest mismatch".into());
            }
            return Ok(r.reply.clone());
        }
        if let Some(mut reply) = self.retired(id) {
            reply["digest"] = json!(digest);
            return Ok(reply);
        }
        let p = self.pending.as_ref().ok_or("no pending batch")?;
        if p.id != id.0 || p.digest != digest {
            return Err("resolution identity/digest mismatch".into());
        }
        match &decision {
            DecisionV1::Unknown => return Ok(p.proposal.clone()),
            DecisionV1::Committed { cursor } if *cursor != p.result => {
                return Err("committed cursor differs".into());
            }
            _ => {}
        }
        let p = self.pending.take().expect("pending checked");
        let reply = match decision {
            DecisionV1::Committed { .. } => {
                self.committed = p.runtime;
                json!({"kind":"committed","batch_id":id,"digest":digest,"cursor":p.result,"ticks_hex":p.proposal["payload"]["ticks_hex"],"effects_hex":p.proposal["payload"]["effects_hex"],"events_hex":p.proposal["payload"]["events_hex"],"control_v2":p.proposal["payload"]["control_v2"],"receipt_hex":p.action.map(|bytes|action_receipt(bytes,Some(p.result.completed_tick.0))).transpose()?})
            }
            DecisionV1::Rejected => {
                json!({"kind":"rejected","batch_id":id,"digest":digest,"cursor":cursor(&self.committed)?,"receipt_hex":p.action.map(|bytes|action_receipt(bytes,None)).transpose()?})
            }
            DecisionV1::Unknown => unreachable!(),
        };
        self.history_bytes += serde_json::to_vec(&reply)?.len();
        self.history.insert(
            id.0,
            Recorded {
                input_hash: p.input_hash,
                digest,
                reply: reply.clone(),
            },
        );
        Ok(reply)
    }

    fn retired(&self, id: DecimalId) -> Option<Value> {
        (id.0 != 0 && id.0 <= self.retired_through).then(|| {
            json!({"kind":"retired","binding":self.binding,"batch_id":id,"retired_through_batch_id":DecimalId(self.retired_through)})
        })
    }

    fn export_receipts(&self) -> Result<Value> {
        if !self.recovery_committed || self.pending.is_some() {
            return Err("receipt export requires committed state without a pending batch".into());
        }
        let mut payload = json!({"schema_version":1,"binding":self.binding,"cursor":cursor(&self.committed)?,"checkpoint_sha256":Digest::of(&checkpoint(&self.committed)?),"through_batch_id":DecimalId(self.retired_through),"records":[]});
        let mut records = Vec::new();
        // Include every retained ID in the prefix, including rejected decisions.
        // Pagination bounds a durable write without silently discarding correlations.
        let mut bytes = serde_json::to_vec(&payload)?.len();
        for (id, record) in self.history.iter().take(MAX_RECEIPT_EXPORT) {
            let value = json!({"batch_id":DecimalId(*id),"input_sha256":record.input_hash,"digest":record.digest,"reply":record.reply});
            let next_bytes = serde_json::to_vec(&value)?.len() + 32;
            if bytes + next_bytes > MAX_RECEIPT_EXPORT_BYTES {
                if records.is_empty() {
                    return Err(
                        "single receipt exceeds durable export profile; retained locally".into(),
                    );
                }
                break;
            }
            bytes += next_bytes;
            records.push(value);
            payload["through_batch_id"] = json!(DecimalId(*id));
        }
        payload["records"] = json!(records);
        let digest = Digest::of(&serde_json::to_vec(&payload)?);
        Ok(json!({"kind":"receipt_export","payload":payload,"digest":digest}))
    }

    fn acknowledge_receipts(&mut self, ack: ReceiptCheckpointV1) -> Result<Value> {
        let reply = || json!({"kind":"receipts_retired","acknowledgement":ack});
        // Exact replay is harmless even after the runtime has advanced.
        if self.last_receipt_ack.as_ref() == Some(&ack) {
            return Ok(reply());
        }
        if ack.schema_version != 1
            || ack.binding != self.binding
            || ack.through_batch_id.0 <= self.retired_through
        {
            return Err("receipt acknowledgement identity or watermark mismatch".into());
        }
        let exported = self.export_receipts()?;
        let p = &exported["payload"];
        if serde_json::to_value(&ack.cursor)? != p["cursor"]
            || ack.checkpoint_sha256 != p["checkpoint_sha256"]
            || serde_json::to_value(ack.through_batch_id)? != p["through_batch_id"]
            || ack.receipt_set_sha256 != exported["digest"]
        {
            return Err("receipt acknowledgement differs from exact checkpoint/export".into());
        }
        self.history.retain(|id, _| *id > ack.through_batch_id.0);
        self.history_bytes = self.history.values().try_fold(0, |sum, record| {
            Ok::<usize, serde_json::Error>(sum + serde_json::to_vec(&record.reply)?.len())
        })?;
        self.retired_through = ack.through_batch_id.0;
        let result = reply();
        self.last_receipt_ack = Some(ack);
        Ok(result)
    }
}

fn action_receipt(bytes: Vec<u8>, tick: Option<u64>) -> Result<String> {
    Ok(hex(
        &wonderland_game_runtime::live_wire::player::encode_receipt(
            &wonderland_game_runtime::live_wire::player::ActionReceipt {
                request: bytes,
                accepted_tick: tick,
            },
        )?,
    ))
}
