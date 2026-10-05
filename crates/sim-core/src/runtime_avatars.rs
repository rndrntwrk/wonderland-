//! Source animation/outfit resource adapters for the headless runtime.
//!
//! `AnimationKey` identifies an ingested metadata record. Executable animation
//! lookup follows VMMemory.GetAnimation's STR table, owner and child rules, then
//! resolves the provider resource identity. No renderer/resource callback advances it.
//!
//! Source: VMAnimateSim.cs23-160, VMMemory.cs674-711, VMAvatar.cs364-390 and
//! 929-938, VMSuitProvider.cs77-197. Missing normalized whole walk/swim tables
//! leave the serialized arrays unfilled; the partial-content runtime does not
//! fabricate stock walk names or substitute an adult table for a child table.
//! FAR3Provider.Get and TS1BCFProvider.Get fold resource names to lower case.
//! ContentSet implements ASCII-insensitive lookup and rejects conflicting
//! aliases; B must normalize non-ASCII names to the pinned provider's rules.

use crate::avatars::{
    outfits::{LegacySuitInputs, OutfitError, OutfitReference},
    timeline::{AnimationCommand, AnimationMetadata, AnimationMode, AnimationResult},
    AvatarPlatform, AvatarState,
};
use crate::ids::EntityRef;
use crate::runtime::{avatar_mut, live_entity};
use crate::state::{ContentSet, SimState};
use crate::vm::{AnimationRequest, HostResponse, PrimitiveExit, ResolvedSuit, VmFault};

const DEFAULT_CARRY_RESOURCE: &str = "a2o-rarm-carry-loop.anim";

fn invalid(value: impl std::fmt::Debug) -> VmFault {
    VmFault::HostUnsupported(format!("avatar resource adapter: {value:?}"))
}

fn is_child(avatar: &AvatarState) -> Result<bool, VmFault> {
    Ok(
        avatar.platform == AvatarPlatform::Ts1
            && avatar.read_person_data(58).map_err(invalid)? < 18,
    )
}

/// Call after installing initial person data. Empty special-walk strings keep
/// the corresponding global value; a later nonempty object value replaces it.
/// All fallible validation precedes committing either serialized string array.
pub fn initialize_walk_animations(
    avatar: &mut AvatarState,
    object_guid: u32,
    content: &ContentSet,
) -> Result<(), VmFault> {
    avatar.validate().map_err(invalid)?;
    let child = is_child(avatar)?;
    let mut next = avatar.clone();
    for (owner, table, special, swim) in [
        (0, if child { 151 } else { 150 }, false, false),
        (0, if child { 160 } else { 158 }, false, true),
        (object_guid, 150, true, false),
    ] {
        if content.string(owner, table, 50).is_some() {
            return Err(invalid("walk/swim table exceeds source array length 50"));
        }
        for index in 0..50 {
            let Some(value) = content.string(owner, table, index) else {
                break;
            };
            if special && value.is_empty() {
                continue;
            }
            let destination = if swim {
                &mut next.swim_animations
            } else {
                &mut next.walk_animations
            };
            destination[index as usize] = value.to_owned();
        }
    }
    next.validate().map_err(invalid)?;
    *avatar = next;
    Ok(())
}

fn indexed_animation<'a>(
    state: &SimState,
    content: &'a ContentSet,
    request: &AnimationRequest,
    child: bool,
) -> Result<Option<&'a AnimationMetadata>, VmFault> {
    let (owner, table) = match request.scope {
        0 | 65_536 => {
            let owner = if request.scope == 0 {
                request.context.code_owner
            } else {
                let target = request
                    .context
                    .stack_object_ref
                    .ok_or(VmFault::MissingEntity(request.context.stack_object))?;
                if target.object_id != request.context.stack_object {
                    return Err(VmFault::InvalidContinuation(
                        "animation stack-object identity mismatch".into(),
                    ));
                }
                live_entity(state, target)?.info.guid
            };
            let object = content
                .object(owner)
                .ok_or_else(|| invalid("animation resource owner definition missing"))?;
            let configured = object.animation_table_id.wrapping_add(u16::from(child));
            // A present table with an absent ID does not fall back.
            let table = if content.has_string_table(owner, configured) {
                configured
            } else if child {
                130
            } else {
                129
            };
            (owner, table)
        }
        1 => (0, 128),
        2 => (0, 130),
        3 => (0, if child { 157 } else { 156 }),
        // C#'s switch leaves animTable null for unknown scopes.
        _ => return Ok(None),
    };
    let Some(base) = content.string(owner, table, i32::from(request.animation_id)) else {
        return Ok(None);
    };
    // VMMemory appends the extension without trimming. Resource-provider case
    // normalization belongs to ContentSet's normalized name lookup.
    Ok(content.animation_by_resource(&format!("{base}.anim")))
}

/// The primitive adapter, including source reset ordering and named carry lookup.
/// Missing ordinary resources yield the source GotoTrueNextTick result; reset
/// with an absent posture resource still clears animations and changes HeadSeek.
pub fn animate(
    state: &mut SimState,
    content: &ContentSet,
    request: &AnimationRequest,
) -> Result<HostResponse, VmFault> {
    let item = live_entity(state, request.context.caller)?;
    let avatar = item
        .avatar
        .as_ref()
        .ok_or_else(|| invalid("animation caller is not an avatar"))?;
    avatar.validate().map_err(invalid)?;
    let mode = match request.mode {
        0 => AnimationMode::PlayAndWait,
        1 => AnimationMode::Loop,
        2 => AnimationMode::Carry,
        3 => AnimationMode::ClearCarryAndWait,
        _ => {
            return Err(VmFault::InvalidOperand {
                opcode: 44,
                detail: "animation mode outside 0..3".into(),
            })
        }
    };
    let reset = request.animation_id == 0;
    let metadata = if reset {
        if mode == AnimationMode::ClearCarryAndWait {
            None
        } else {
            let posture = avatar.read_person_data(0).map_err(invalid)?;
            let posture = if posture == 1 || posture == 2 {
                posture as usize
            } else {
                3
            };
            content.animation_by_resource(&format!("{}.anim", avatar.walk_animations[posture]))
        }
    } else {
        indexed_animation(state, content, request, is_child(avatar)?)?
    };
    let command = AnimationCommand {
        mode,
        resource: metadata.map_or_else(String::new, |meta| meta.resource.clone()),
        reset,
        backwards: request.backwards,
        hurryable: request.hurryable,
        walk_style: *item
            .object_data
            .get(17)
            .ok_or_else(|| invalid("WalkStyle object variable missing"))?,
        expected_events: request.expected_events,
        carrying: item.slots.first().is_some_and(Option::is_some),
        default_carry: content
            .animation_by_resource(DEFAULT_CARRY_RESOURCE)
            .cloned(),
    };
    let result = avatar_mut(state, request.context.caller)?
        .apply_animation(&command, metadata)
        .map_err(invalid)?;
    Ok(match result {
        AnimationResult::Wait => HostResponse::NextTick,
        AnimationResult::Event(code) => HostResponse::AnimationEvent(code),
        AnimationResult::Complete => HostResponse::Complete(PrimitiveExit::GotoTrue),
        AnimationResult::CompleteNextTick => {
            HostResponse::Complete(PrimitiveExit::GotoTrueNextTick)
        }
    })
}

/// Shared resolution for primitive six and CurrentOutfit memory writes. Job
/// uniforms are looked up using the live TS1 job type/level and current gender.
/// Content decoders supply literal hand groups; this adapter does not parse meshes.
pub fn resolve_person_outfit(
    state: &SimState,
    content: &ContentSet,
    entity: EntityRef,
    suit: u16,
) -> Result<Option<OutfitReference>, VmFault> {
    let item = live_entity(state, entity)?;
    let avatar = item
        .avatar
        .as_ref()
        .ok_or_else(|| invalid("outfit target is not an avatar"))?;
    avatar.validate().map_err(invalid)?;
    let gender = avatar.read_person_data(65).map_err(invalid)?;
    if avatar.platform == AvatarPlatform::Tso {
        return match avatar.outfits.resolve_tso(
            suit,
            gender,
            avatar.read_person_data(91).map_err(invalid)?,
            avatar.read_person_data(92).map_err(invalid)?,
        ) {
            Ok(value) => Ok(Some(value)),
            Err(OutfitError::UnsupportedSuit(_)) => Ok(None),
            Err(error) => Err(invalid(error)),
        };
    }
    let object = content
        .object(item.info.guid)
        .ok_or_else(|| invalid("avatar definition missing"))?;
    if !content.has_string_table(item.info.guid, object.body_string_id) {
        return Err(invalid("TS1 body string table missing"));
    }
    let body_strings = (0..35)
        .map_while(|index| content.string(item.info.guid, object.body_string_id, index))
        .map(str::to_owned)
        .collect();
    let body_hand_group = match content.suit(item.info.guid, 1, 0) {
        Some(ResolvedSuit::Reference(OutfitReference::Legacy { hand_group, .. })) => {
            hand_group.clone()
        }
        _ => None,
    };
    let (job_uniform, job_uses_original_body) = if suit == 3 {
        let job_type = avatar.read_person_data(56).map_err(invalid)?;
        let level = avatar.read_person_data(57).map_err(invalid)?;
        let uniform = content
            .legacy_job_uniform(job_type, level)
            .ok_or_else(|| invalid("TS1 job-level uniform metadata missing"))?;
        // The source checks the male mesh for empty before the female override.
        let use_original_body = uniform.male_mesh.is_empty();
        let mesh = if use_original_body {
            String::new()
        } else if gender != 0 {
            uniform
                .female_mesh
                .as_ref()
                .unwrap_or(&uniform.male_mesh)
                .clone()
        } else {
            uniform.male_mesh.clone()
        };
        (Some((mesh, uniform.texture.clone())), use_original_body)
    } else {
        (None, false)
    };
    let returns_original_body = suit == 0 || job_uses_original_body;
    let hand_group = if returns_original_body {
        body_hand_group
    } else {
        match &avatar.outfits.defaults.daywear {
            OutfitReference::Legacy {
                hand_group: Some(group),
                ..
            } => Some(group.clone()),
            _ => body_hand_group,
        }
    };
    let input = LegacySuitInputs {
        body_strings,
        gender,
        age: avatar.read_person_data(58).map_err(invalid)?,
        hand_group,
        job_uniform,
    };
    let resolved = if suit == 3 && !job_uses_original_body {
        input.resolve_selected_job_uniform()
    } else {
        input.resolve(suit)
    };
    match resolved {
        Ok(value) => {
            value.validate().map_err(invalid)?;
            Ok(Some(value))
        }
        Err(OutfitError::UnsupportedSuit(_)) => Ok(None),
        Err(error) => Err(invalid(error)),
    }
}

/// Apply an emitted CurrentOutfit signal after the raw person-data write. The
/// selector itself is already stored. TS1 assigns null for unsupported suits;
/// TSO leaves the current body unchanged when its provider returns null.
pub fn apply_person_outfit(
    state: &mut SimState,
    content: &ContentSet,
    entity: EntityRef,
    suit: u16,
) -> Result<bool, VmFault> {
    let resolved = resolve_person_outfit(state, content, entity, suit)?;
    let avatar = avatar_mut(state, entity)?;
    if avatar.platform == AvatarPlatform::Ts1 || resolved.is_some() {
        let changed = avatar.outfits.body != resolved;
        avatar.outfits.body = resolved;
        Ok(changed)
    } else {
        Ok(false)
    }
}
