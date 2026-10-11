// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! Integration seams for Swarms A/F. Implementations must be deterministic and
//! obey the snapshot, budgeting, and atomic-start contracts below.

use super::{
    ActionInvocation, CheckBudget, CheckExit, CheckOutput, CheckRequest, CheckState, EntityKey,
    EntityVersion, InteractionKey, InteractionLimits, InteractionSnapshot, PrincipalKey, Result,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityOperation {
    Query {
        include_hidden: bool,
    },
    Invoke {
        target: EntityKey,
        interaction: InteractionKey,
    },
    Cancel {
        action_id: u64,
    },
}

/// Authoritative generation lookup, command authorization, and detached state.
///
/// `snapshot` must copy **all** check-tree-visible state (including RNG, every
/// writable entity, temps, and advertisements) into owned values/bytes. It must
/// resolve TTAB/BHAV/TTAs using effective content identity and reject missing
/// nonzero test routines. No pointer, Arc, handle back to live mutable VM state,
/// durable provider, UI, wall clock, or ambient random source belongs in a check.
/// `revision` must change when any query-visible data or permissions change.
/// Mark missing local TTAB with `set_local_table_present(false)`; an empty but
/// existing TTAB is distinct. Check-frame CodeOwner is the action binding's owner
/// (VMThread.CheckAction), even when the resolved test binding has another owner.
pub trait WorldProvider {
    fn revision(&self) -> u64;
    /// None means missing/dead or generation mismatch, not a replacement entity.
    fn entity_version(&self, key: EntityKey) -> Option<EntityVersion>;
    fn authorize(
        &self,
        principal: PrincipalKey,
        actor: EntityKey,
        operation: AuthorityOperation,
    ) -> bool;
    fn snapshot(
        &self,
        actor: EntityKey,
        target: EntityKey,
        limits: &InteractionLimits,
    ) -> Result<InteractionSnapshot>;
}

/// The same interpreter is used for detached UI checks and explicit in-tick
/// checks. Its only mutable capability here is `state` and bounded `output`.
///
/// Spend budget for every primitive/step, terminate on failure, and use only the
/// supplied RNG/state. Rust cannot prevent a malicious implementation from
/// reaching global state; integration must audit this provider contract.
pub trait CheckTreeProvider {
    fn evaluate(
        &self,
        request: CheckRequest<'_>,
        state: &mut CheckState,
        output: &mut CheckOutput,
        budget: &mut CheckBudget,
    ) -> Result<CheckExit>;
}

/// Called only by the authoritative 30 Hz scheduler, or its deterministic mirror.
/// `check_action` applies VMThread.CheckAction with real occupancy and may persist
/// legitimate in-tick register/RNG changes. `start_action` pushes the resolved
/// action frame; false/error must leave the VM stack unchanged. Route/slot/lock
/// primitives run through the real action interpreter, never this queue module.
/// A route failure after start is reported with `finish_active(Failed)`.
pub trait QueueRuntime {
    fn target_is_alive(&self, target: EntityKey) -> bool;
    fn check_action(&mut self, action: &ActionInvocation) -> Result<bool>;
    fn start_action(&mut self, action: &ActionInvocation) -> Result<bool>;
}
