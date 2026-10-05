// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! Detached offer queries and authoritative intent validation are separate APIs.

use super::adapters::{AuthorityOperation, CheckTreeProvider, WorldProvider};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewStamp {
    pub world_revision: u64,
    pub actor: EntityVersion,
    pub target: EntityVersion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvertisementChange {
    pub motive: i32,
    pub value: i16,
}

/// Owned, detached values only. Additional VM state is represented as bounded
/// bytes so this module does not invent an object graph or share mutable handles.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CheckState {
    pub rng_seed: u64,
    pub temp_registers: [i16; 20],
    pub temp_xl: [i32; 2],
    pub hidden: bool,
    pub hide_interaction: bool,
    pub out_of_world: bool,
    pub target_occupied: bool,
    provider_state: Vec<u8>,
    advertisements: Vec<AdvertisementChange>,
}

impl CheckState {
    pub fn provider_state(&self) -> &[u8] {
        &self.provider_state
    }

    pub fn provider_state_mut(&mut self) -> &mut [u8] {
        &mut self.provider_state
    }

    pub fn set_provider_state(&mut self, bytes: &[u8], limits: &InteractionLimits) -> Result<()> {
        if bytes.len() > limits.max_provider_state_bytes {
            return Err(Error::LimitExceeded("check provider state bytes"));
        }
        self.provider_state = bytes.to_vec();
        Ok(())
    }

    pub fn advertisements(&self) -> &[AdvertisementChange] {
        &self.advertisements
    }

    pub fn set_advertisement(
        &mut self,
        motive: i32,
        value: i16,
        limits: &InteractionLimits,
    ) -> Result<()> {
        match self
            .advertisements
            .binary_search_by_key(&motive, |entry| entry.motive)
        {
            Ok(index) => self.advertisements[index].value = value,
            Err(index) => {
                if self.advertisements.len() >= limits.max_advertisements {
                    return Err(Error::LimitExceeded("check advertisements"));
                }
                self.advertisements
                    .insert(index, AdvertisementChange { motive, value });
            }
        }
        Ok(())
    }

    /// VMContext.NextRandom, including max == 0 not consuming RNG state.
    pub fn random_below(&mut self, max: u64) -> u64 {
        if max == 0 {
            return 0;
        }
        self.rng_seed ^= self.rng_seed >> 12;
        self.rng_seed ^= self.rng_seed << 25;
        self.rng_seed ^= self.rng_seed >> 27;
        self.rng_seed.wrapping_mul(2_685_821_657_736_338_717) % max
    }

    fn validate(&self, limits: &InteractionLimits) -> Result<()> {
        if self.provider_state.len() > limits.max_provider_state_bytes {
            return Err(Error::LimitExceeded("check provider state bytes"));
        }
        if self.advertisements.len() > limits.max_advertisements {
            return Err(Error::LimitExceeded("check advertisements"));
        }
        Ok(())
    }
}

/// Populate in TTAB source order: locals first, then globals. Missing action or
/// nonzero missing check BHAVs must be omitted by the content adapter (GetAction).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionSnapshot {
    mode: LegacyMode,
    stamp: ViewStamp,
    actor: ActorFacts,
    target: TargetFacts,
    state: CheckState,
    local_table_present: bool,
    definitions: Vec<InteractionDefinition>,
}

impl InteractionSnapshot {
    pub fn new(
        mode: LegacyMode,
        world_revision: u64,
        actor: ActorFacts,
        target: TargetFacts,
        state: CheckState,
        limits: &InteractionLimits,
    ) -> Result<Self> {
        state.validate(limits)?;
        Ok(Self {
            mode,
            stamp: ViewStamp {
                world_revision,
                actor: actor.version,
                target: target.version,
            },
            actor,
            target,
            state,
            local_table_present: true,
            definitions: Vec::new(),
        })
    }

    pub fn add_definition(
        &mut self,
        definition: InteractionDefinition,
        limits: &InteractionLimits,
    ) -> Result<()> {
        validate_definition(&definition, limits)?;
        if definition.key.scope == InteractionScope::Local && !self.local_table_present {
            return Err(Error::InvalidSnapshot(
                "local interaction without a local TTAB",
            ));
        }
        if self.definitions.len() >= limits.max_definitions {
            return Err(Error::LimitExceeded("interaction definitions"));
        }
        if self
            .definitions
            .iter()
            .any(|entry| entry.key == definition.key)
        {
            return Err(Error::InvalidSnapshot("duplicate TTAB scope/TTAIndex"));
        }
        if definition.key.scope == InteractionScope::Local
            && self
                .definitions
                .last()
                .is_some_and(|entry| entry.key.scope == InteractionScope::Global)
        {
            return Err(Error::InvalidSnapshot(
                "local interactions must precede globals",
            ));
        }
        self.definitions.push(definition);
        Ok(())
    }

    pub fn stamp(&self) -> ViewStamp {
        self.stamp
    }
    pub fn mode(&self) -> LegacyMode {
        self.mode
    }
    pub fn state(&self) -> &CheckState {
        &self.state
    }
    /// Only an authoritative in-tick adapter should persist these mutations.
    pub fn state_mut(&mut self) -> &mut CheckState {
        &mut self.state
    }
    pub fn definitions(&self) -> &[InteractionDefinition] {
        &self.definitions
    }

    /// GetPieMenu returns immediately when the local TTAB is absent, even if
    /// globals exist. GetPieMenuForInteraction may still validate a global key.
    pub fn set_local_table_present(&mut self, present: bool) -> Result<()> {
        if !present
            && self
                .definitions
                .iter()
                .any(|definition| definition.key.scope == InteractionScope::Local)
        {
            return Err(Error::InvalidSnapshot(
                "cannot remove local TTAB with local definitions",
            ));
        }
        self.local_table_present = present;
        Ok(())
    }

    fn validate(&self, limits: &InteractionLimits) -> Result<()> {
        self.state.validate(limits)?;
        if self.definitions.len() > limits.max_definitions {
            return Err(Error::LimitExceeded("interaction definitions"));
        }
        for definition in &self.definitions {
            validate_definition(definition, limits)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_definition(
    definition: &InteractionDefinition,
    limits: &InteractionLimits,
) -> Result<()> {
    if definition
        .label
        .as_ref()
        .is_some_and(|label| label.len() > limits.max_label_bytes)
    {
        return Err(Error::LimitExceeded("interaction label bytes"));
    }
    if definition
        .check
        .is_some_and(|binding| binding.routine_id == 0)
    {
        return Err(Error::InvalidSnapshot("zero check ID must be None"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckOrigin {
    Ui,
    IntentValidation,
    InTick,
}

#[derive(Clone, Copy, Debug)]
pub struct CheckRequest<'a> {
    pub mode: LegacyMode,
    pub actor: EntityKey,
    pub target: EntityKey,
    pub definition: &'a InteractionDefinition,
    /// CheckAction supplies four zero args; only autonomous checks set arg 0 = 1.
    /// This is deliberately not the eventual action's variant/Param0.
    pub args: [i16; 4],
    pub origin: CheckOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckExit {
    ReturnTrue,
    ReturnFalse,
    Yielded,
    Aborted,
}

#[derive(Clone, Debug)]
pub struct CheckBudget {
    remaining: u64,
    exhausted: bool,
}

impl CheckBudget {
    pub fn new(steps: u64) -> Self {
        Self {
            remaining: steps,
            exhausted: false,
        }
    }
    pub fn spend(&mut self, steps: u64) -> Result<()> {
        match self.remaining.checked_sub(steps) {
            Some(left) if !self.exhausted => {
                self.remaining = left;
                Ok(())
            }
            _ => {
                self.exhausted = true;
                Err(Error::LimitExceeded("check instruction budget"))
            }
        }
    }
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckVariant {
    label: Option<String>,
    param0: i16,
}

/// VMChangeActionString output. Checked before allocating/copying each label.
#[derive(Clone, Debug)]
pub struct CheckOutput {
    variants: Vec<CheckVariant>,
    limits: InteractionLimits,
    failure: Option<Error>,
}

impl CheckOutput {
    fn new(limits: &InteractionLimits) -> Self {
        Self {
            variants: Vec::new(),
            limits: *limits,
            failure: None,
        }
    }

    pub fn push_variant(&mut self, label: Option<&str>, param0: i16) -> Result<()> {
        let error = if self.variants.len() >= self.limits.max_offers {
            Some(Error::LimitExceeded("check variants"))
        } else if label.is_some_and(|label| label.len() > self.limits.max_label_bytes) {
            Some(Error::LimitExceeded("check variant label bytes"))
        } else {
            None
        };
        if let Some(error) = error {
            self.failure = Some(error.clone());
            return Err(error);
        }
        self.variants.push(CheckVariant {
            label: label.map(str::to_owned),
            param0,
        });
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionOffer {
    pub interaction: InteractionKey,
    pub param0: i16,
    pub label: String,
    pub advertisements: Vec<AdvertisementChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferBatch {
    pub seen: ViewStamp,
    pub offers: Vec<InteractionOffer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryOptions {
    pub include_hidden: bool,
    pub include_global: bool,
    pub autonomous: bool,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            include_global: true,
            autonomous: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfferQuery {
    pub principal: PrincipalKey,
    pub seen: ViewStamp,
    pub options: QueryOptions,
}

/// This function cannot hand live mutable VM state to a check provider. On every
/// exit path (including interpreter errors), the detached snapshot is discarded.
pub fn query_offers<W: WorldProvider, C: CheckTreeProvider>(
    world: &W,
    checks: &C,
    query: OfferQuery,
    limits: &InteractionLimits,
) -> Result<OfferBatch> {
    check_world_stamp(world, query.seen)?;
    require_authority(
        world,
        query.principal,
        query.seen.actor.key,
        AuthorityOperation::Query {
            include_hidden: query.options.include_hidden,
        },
    )?;
    let mut snapshot = checked_snapshot(world, query.seen, limits)?;
    let result = collect_offers(
        &mut snapshot,
        checks,
        query.options,
        None,
        CheckOrigin::Ui,
        limits,
    )?;
    check_world_stamp(world, query.seen)?;
    Ok(result)
}

/// Explicitly mutates the supplied tick-owned projection. The adapter must apply
/// changes to the authoritative VM within the same tick; the UI never calls this
/// on live state. Check-only advertisements are reset for each evaluated routine.
pub fn query_in_tick<C: CheckTreeProvider>(
    snapshot: &mut InteractionSnapshot,
    checks: &C,
    options: QueryOptions,
    limits: &InteractionLimits,
) -> Result<OfferBatch> {
    collect_offers(snapshot, checks, options, None, CheckOrigin::InTick, limits)
}

fn collect_offers<C: CheckTreeProvider>(
    snapshot: &mut InteractionSnapshot,
    checks: &C,
    options: QueryOptions,
    only: Option<InteractionKey>,
    origin: CheckOrigin,
    limits: &InteractionLimits,
) -> Result<OfferBatch> {
    snapshot.validate(limits)?;
    let mut offers = Vec::new();
    let mut budget = CheckBudget::new(limits.max_check_steps);
    let baseline_temps = snapshot.state.temp_registers;
    let baseline_temp_xl = snapshot.state.temp_xl;
    if snapshot.target.disabled || (only.is_none() && !snapshot.local_table_present) {
        return Ok(OfferBatch {
            seen: snapshot.stamp,
            offers,
        });
    }
    for definition in &snapshot.definitions {
        if (!options.include_global && definition.key.scope == InteractionScope::Global)
            || only.is_some_and(|key| key != definition.key)
        {
            continue;
        }
        snapshot.state.hide_interaction = false;
        // EvaluateCheck creates a fresh check-thread advertisement dictionary.
        snapshot.state.advertisements.clear();
        let policy = permission_decision(
            snapshot.mode,
            &snapshot.actor,
            &snapshot.target,
            definition,
            options.autonomous,
        );
        if policy == PermissionDecision::Denied {
            continue;
        }
        let mut output = CheckOutput::new(limits);
        let ran_check = policy == PermissionDecision::RunCheck;
        if ran_check {
            budget.spend(1)?; // a dispatch also consumes budget, even for an empty fixture
            if origin != CheckOrigin::InTick {
                // VMThread.EvaluateCheck clones the caller's original arrays
                // separately for every out-of-tick check, even after rejection.
                // RNG and other detached state have distinct source semantics.
                snapshot.state.temp_registers = baseline_temps;
                snapshot.state.temp_xl = baseline_temp_xl;
            }
            let request = CheckRequest {
                mode: snapshot.mode,
                actor: snapshot.stamp.actor.key,
                target: snapshot.stamp.target.key,
                definition,
                args: [i16::from(options.autonomous), 0, 0, 0],
                origin,
            };
            let exit = checks.evaluate(request, &mut snapshot.state, &mut output, &mut budget)?;
            if budget.exhausted {
                return Err(Error::LimitExceeded("check instruction budget"));
            }
            if let Some(error) = output.failure {
                return Err(error);
            }
            snapshot.state.validate(limits)?;
            match exit {
                CheckExit::ReturnTrue => {}
                CheckExit::ReturnFalse | CheckExit::Yielded => continue,
                CheckExit::Aborted => return Err(Error::RuntimeFailure("check tree aborted")),
            }
        }
        if !options.include_hidden
            && (snapshot.state.hidden
                || snapshot.state.hide_interaction
                || snapshot.state.out_of_world)
        {
            continue;
        }
        if output.variants.is_empty() {
            // EvaluateCheck synthesizes an advertisement-only entry when it ran.
            // Without a check, GetPieMenu adds a base entry only when TTAs exists.
            if !ran_check && definition.label.is_none() {
                continue;
            }
            push_offer(
                &mut offers,
                InteractionOffer {
                    interaction: definition.key,
                    param0: 0,
                    label: definition
                        .label
                        .as_deref()
                        .unwrap_or("***MISSING***")
                        .to_owned(),
                    advertisements: if ran_check {
                        snapshot.state.advertisements.clone()
                    } else {
                        Vec::new()
                    },
                },
                limits,
            )?;
        } else {
            for variant in output.variants {
                // VMChangeActionString variants do not copy the default motive ads.
                let label = variant
                    .label
                    .or_else(|| definition.label.clone())
                    .unwrap_or_else(|| "***MISSING***".to_owned());
                push_offer(
                    &mut offers,
                    InteractionOffer {
                        interaction: definition.key,
                        param0: variant.param0,
                        label,
                        advertisements: Vec::new(),
                    },
                    limits,
                )?;
            }
        }
    }
    Ok(OfferBatch {
        seen: snapshot.stamp,
        offers,
    })
}

fn push_offer(
    offers: &mut Vec<InteractionOffer>,
    offer: InteractionOffer,
    limits: &InteractionLimits,
) -> Result<()> {
    if offers.len() >= limits.max_offers {
        return Err(Error::LimitExceeded("interaction offers"));
    }
    if offer.label.len() > limits.max_label_bytes {
        return Err(Error::LimitExceeded("offer label bytes"));
    }
    offers.push(offer);
    Ok(())
}

pub(crate) fn require_authority<W: WorldProvider>(
    world: &W,
    principal: PrincipalKey,
    actor: EntityKey,
    operation: AuthorityOperation,
) -> Result<()> {
    if world.authorize(principal, actor, operation) {
        Ok(())
    } else {
        Err(Error::Unauthorized)
    }
}

pub(crate) fn check_world_stamp<W: WorldProvider>(world: &W, seen: ViewStamp) -> Result<()> {
    check_actor_stamp(world, seen.world_revision, seen.actor)?;
    if world.entity_version(seen.target.key) != Some(seen.target) {
        return Err(Error::StaleEntity(seen.target.key));
    }
    Ok(())
}

pub(crate) fn check_actor_stamp<W: WorldProvider>(
    world: &W,
    revision: u64,
    actor: EntityVersion,
) -> Result<()> {
    let actual = world.revision();
    if actual != revision {
        return Err(Error::StaleWorldRevision {
            expected: revision,
            actual,
        });
    }
    if world.entity_version(actor.key) != Some(actor) {
        return Err(Error::StaleEntity(actor.key));
    }
    Ok(())
}

fn checked_snapshot<W: WorldProvider>(
    world: &W,
    seen: ViewStamp,
    limits: &InteractionLimits,
) -> Result<InteractionSnapshot> {
    let snapshot = world.snapshot(seen.actor.key, seen.target.key, limits)?;
    if snapshot.stamp != seen {
        return Err(Error::InvalidSnapshot(
            "snapshot version differs from requested live versions",
        ));
    }
    snapshot.validate(limits)?;
    Ok(snapshot)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionIntent {
    pub principal: PrincipalKey,
    pub seen: ViewStamp,
    pub queue_revision: u64,
    /// Monotonic sequence in this actor-generation's authoritative command stream.
    /// The network adapter must bind this to the authenticated command envelope.
    pub command_sequence: u64,
    pub interaction: InteractionKey,
    pub param0: i16,
}

/// A single-use, non-Clone permit. Fields and construction stay private: an
/// InteractionOffer cannot be converted directly into an executable queue entry.
#[derive(Debug)]
pub struct ValidatedIntent {
    pub(crate) intent: InteractionIntent,
    pub(crate) mode: LegacyMode,
    pub(crate) action: ActionInvocation,
}

pub fn validate_intent<W: WorldProvider, C: CheckTreeProvider>(
    world: &W,
    checks: &C,
    queue: &ActionQueue,
    intent: InteractionIntent,
    limits: &InteractionLimits,
) -> Result<ValidatedIntent> {
    check_world_stamp(world, intent.seen)?;
    queue.validate_command(
        intent.seen.actor.key,
        intent.queue_revision,
        intent.command_sequence,
    )?;
    require_authority(
        world,
        intent.principal,
        intent.seen.actor.key,
        AuthorityOperation::Invoke {
            target: intent.seen.target.key,
            interaction: intent.interaction,
        },
    )?;
    let mut snapshot = checked_snapshot(world, intent.seen, limits)?;
    if snapshot.mode != queue.mode() {
        return Err(Error::InvalidSnapshot(
            "queue/snapshot legacy dialect mismatch",
        ));
    }
    // VMNetInteractionCmd.Verify ignores occupied only while verifying an offer.
    // The queue runtime rechecks real occupancy before an action starts.
    snapshot.state.target_occupied = false;
    let offers = collect_offers(
        &mut snapshot,
        checks,
        QueryOptions::default(),
        Some(intent.interaction),
        CheckOrigin::IntentValidation,
        limits,
    )?;
    if !offers
        .offers
        .iter()
        .any(|offer| offer.interaction == intent.interaction && offer.param0 == intent.param0)
    {
        return Err(Error::VariantUnavailable);
    }
    let definition = snapshot
        .definitions
        .into_iter()
        .find(|entry| entry.key == intent.interaction)
        .ok_or(Error::Unavailable)?;
    check_world_stamp(world, intent.seen)?;
    let action = ActionInvocation::user(
        intent.seen.actor.key,
        intent.seen.target.key,
        definition,
        intent.param0,
    );
    Ok(ValidatedIntent {
        intent,
        mode: snapshot.mode,
        action,
    })
}
