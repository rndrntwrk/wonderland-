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
    CursorV1, DecimalId, DecisionV1, HostBindingV1, RequestV1, TakeoverMarkerV1,
};

const MAX_COMMAND_BYTES: usize = 256 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
const MAX_HISTORY: usize = 4096;
const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(&mut out, "{b:02x}").expect("string output");
    }
    out
}
fn unhex(input: &str) -> Result<Vec<u8>> {
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
fn cursor(runtime: &GameRuntime) -> Result<CursorV1> {
    let s = runtime.sim().state();
    Ok(CursorV1 {
        lot_id: DecimalId(s.lot_id),
        authority_epoch: DecimalId(s.authority_epoch),
        completed_tick: DecimalId(s.completed_tick),
        state_hash: hex(&runtime.sim().state_hash()?),
    })
}
fn checkpoint(runtime: &GameRuntime) -> Result<Vec<u8>> {
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
}
struct Recorded {
    input_hash: Digest,
    digest: String,
    reply: Value,
}
pub struct Host {
    committed: GameRuntime,
    config: RuntimeConfig,
    binding: HostBindingV1,
    report: Value,
    takeover_marker: Option<TakeoverMarkerV1>,
    recovery_committed: bool,
    pending: Option<Pending>,
    history: BTreeMap<u64, Recorded>,
    history_bytes: usize,
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
            config,
            binding,
            report,
            recovery_committed: takeover_marker.is_none(),
            takeover_marker,
            pending: None,
            history: BTreeMap::new(),
            history_bytes: 0,
        })
    }
    pub fn handle(&mut self, request: RequestV1) -> Result<Value> {
        match request {
            RequestV1::Inspect => Ok(
                json!({"kind":if self.recovery_committed {"committed_state"} else {"recovery_candidate"},"recovery_pending":!self.recovery_committed,"binding":self.binding,"cursor":cursor(&self.committed)?,"runtime_content":self.committed.sim().state().content,"sealed_release":self.report,"takeover_marker":self.takeover_marker,"checkpoint_hex":hex(&checkpoint(&self.committed)?),"pending_batch":self.pending.as_ref().map(|p|DecimalId(p.id))}),
            ),
            RequestV1::ActivateRecovery { marker } => {
                if self.takeover_marker.as_ref() != Some(&marker) {
                    return Err("recovery activation marker mismatch".into());
                }
                self.recovery_committed = true;
                self.handle(RequestV1::Inspect)
            }
            RequestV1::Lookup { batch_id } => {
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
            RequestV1::Prepare {
                batch_id,
                binding,
                predecessor,
                commands_hex,
            } => {
                if !self.recovery_committed {
                    return Err("replacement awaits durable recovery activation".into());
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
            return Err("receipt history full; durable successor required".into());
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
        });
        Ok(proposal)
    }
    fn resolve(&mut self, id: DecimalId, digest: String, decision: DecisionV1) -> Result<Value> {
        if let Some(r) = self.history.get(&id.0) {
            if r.digest != digest {
                return Err("recorded digest mismatch".into());
            }
            return Ok(r.reply.clone());
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
                json!({"kind":"committed","batch_id":id,"digest":digest,"cursor":p.result,"ticks_hex":p.proposal["payload"]["ticks_hex"],"effects_hex":p.proposal["payload"]["effects_hex"],"events_hex":p.proposal["payload"]["events_hex"]})
            }
            DecisionV1::Rejected => {
                json!({"kind":"rejected","batch_id":id,"digest":digest,"cursor":cursor(&self.committed)?})
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
}
