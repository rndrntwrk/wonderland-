use super::*;
use crate::lot_permissions::{
    self, LotPermissionOperation, LotPermissionProjection, PermissionEntityKind, PermissionObject,
    PersistedPermissionGroup,
};
use crate::owned_admission::{
    MAX_OWNED_ACTORS, OwnedAdmissionState, OwnedAvatarAdmission, SourceJoinMetadata,
};

pub(super) fn initialize(
    state: &mut SimState,
    source: SourceJoinMetadata,
    content: &ContentSet,
) -> Result<(), RuntimeError> {
    if !matches!(state.owned_admission, OwnedAdmissionState::Uninitialized) {
        return Err(invalid("source admission already initialized"));
    }
    state.owned_admission = OwnedAdmissionState::Initialized {
        source: Box::new(source),
        revision: 1,
        permission_revision: 1,
        last_principal: 0,
        actors: BTreeMap::new(),
    };
    state.owned_admission.validate(state).map_err(invalid)?;
    projection(state, content)?;
    Ok(())
}
// Source import binds the native portal projection to the ContentDescriptor.
fn projection(
    state: &SimState,
    content: &ContentSet,
) -> Result<LotPermissionProjection, RuntimeError> {
    let OwnedAdmissionState::Initialized { source, .. } = &state.owned_admission else {
        return Err(invalid("source admission uninitialized"));
    };
    let ReplicatedLotPermissions::Initialized(lot) = &state.lot_permissions else {
        return Err(invalid("lot permissions uninitialized"));
    };
    let mut result = LotPermissionProjection {
        lot: lot.clone(),
        ..Default::default()
    };
    for item in state.entities.values() {
        let object = state
            .world
            .object(item.info.reference)
            .ok_or_else(|| invalid("source world object missing"))?;
        let game = !item.info.is_avatar;
        result.objects.insert(
            item.info.reference.object_id,
            PermissionObject {
                kind: if game {
                    PermissionEntityKind::GameObject
                } else {
                    PermissionEntityKind::Other
                },
                persistent_id: PersistentId(item.info.persistent_id),
                owner_id: if game {
                    object
                        .owner
                        .ok_or_else(|| invalid("source object ownership unavailable"))?
                } else {
                    PersistentId(0)
                },
                object_flags: if game {
                    *source
                        .object_flags
                        .get(&item.info.reference)
                        .ok_or_else(|| invalid("source donation metadata unavailable"))?
                } else {
                    0
                },
                disabled_flags: item.disabled_flags,
                occupied: item.object_data[8] as u16 & (1 << 5) != 0,
                portal: content
                    .object(item.info.guid)
                    .and_then(|definition| definition.entry_points.get(&15))
                    .is_some_and(|routine| routine.id != 0),
            },
        );
    }
    for (pid, refs) in &source.registered_groups {
        let first = live_entity(
            state,
            *refs
                .first()
                .ok_or_else(|| invalid("empty source registration"))?,
        )?;
        let native: BTreeSet<_> = first.info.group.iter().copied().collect();
        let supplied: BTreeSet<_> = refs.iter().map(|r| r.object_id).collect();
        if native != supplied {
            return Err(invalid("source registration differs from native group"));
        }
        for r in refs {
            let part = live_entity(state, *r)?;
            if part.info.persistent_id != pid.0
                || part.info.base_object != first.info.base_object
                || part.info.group != first.info.group
            {
                return Err(invalid("inconsistent source registered group"));
            }
        }
        let in_avatar_stack = state
            .entities
            .values()
            .filter(|e| e.info.is_avatar && !e.info.dead)
            .any(|avatar| {
                state
                    .threads
                    .get(&avatar.info.reference.object_id)
                    .is_some_and(|thread| {
                        thread.frames.iter().any(|frame| {
                            state
                                .entities
                                .get(&frame.context.callee.object_id)
                                .is_some_and(|callee| {
                                    callee.info.reference == frame.context.callee
                                        && !callee.info.is_avatar
                                        && supplied.contains(&callee.info.reference.object_id)
                                })
                        })
                    })
            });
        result.persisted_groups.insert(
            *pid,
            PersistedPermissionGroup {
                base_object: first.info.base_object,
                objects: refs.iter().map(|r| r.object_id).collect(),
                in_avatar_stack,
            },
        );
    }
    result
        .validate()
        .map_err(|e| invalid(format!("source permission projection: {e:?}")))?;
    Ok(result)
}

pub(super) fn admit(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    mut admission: OwnedAvatarAdmission,
) -> Result<(), RuntimeError> {
    admission.control.validate().map_err(invalid)?;
    let OwnedAdmissionState::Initialized {
        source,
        last_principal,
        actors,
        permission_revision,
        ..
    } = &state.owned_admission
    else {
        return Err(invalid("source admission uninitialized"));
    };
    if admission.permission_revision != *permission_revision
        || admission.permission_sha256
            != crate::owned_admission::permission_digest(state).map_err(invalid)?
    {
        return Err(invalid("stale source permission binding"));
    }
    if admission.control.principal <= *last_principal
        || actors.len() >= MAX_OWNED_ACTORS
        || actors
            .values()
            .any(|a| a.avatar_id == admission.control.avatar_id)
        || state
            .entities
            .values()
            .any(|entity| entity.info.persistent_id == admission.control.avatar_id.0)
        || (admission.control.actor.object_id != ObjectId(0)
            || admission.control.actor.generation != 0)
        || admission.portable.persistent_id != admission.control.avatar_id
    {
        return Err(invalid("owned admission identity already used or invalid"));
    }
    let guid = if admission.portable.custom_guid == 0 {
        crate::avatars::state::TEMPLATE_PERSON
    } else {
        admission.portable.custom_guid
    };
    if !source.avatar_guids.contains(&guid) {
        return Err(invalid("source avatar archetype unavailable"));
    }
    let april_fools = source.april_fools_2019;
    projection(state, content)?; // Validate source readiness, never retain this pre-creation query.
    // Bound all portable fields before object creation; apply itself is atomic.
    let mut validation = AvatarState::new(
        EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        },
        admission.portable.persistent_id,
        AvatarPlatform::Tso,
    );
    validation
        .apply_persistent_state(&admission.portable)
        .map_err(|e| invalid(format!("portable avatar: {e:?}")))?;
    let old_strict = work.strict_source;
    work.strict_source = true;
    let actor = spawn_entity(
        state,
        content,
        work,
        SpawnSpec {
            guid,
            position: LotPosition::OUT_OF_WORLD,
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: true,
        },
        ObjectId::NULL,
        ObjectId::NULL,
        0,
    )?;
    let mailbox = state
        .entities
        .values()
        .find(|e| matches!(e.info.guid, 0xef121974 | 0x1d95c9b0))
        .map(|e| e.info.reference);
    let mut budget = ScriptBudget {
        remaining: state
            .limits
            .max_tick_instructions
            .saturating_sub(work.instructions),
        depth: 0,
        used: 0,
    };
    if let Some(mailbox) = mailbox {
        crate::runtime_routes::find_location_for(
            state,
            content,
            actor,
            mailbox,
            world::PlaceRequestFlags::default(),
            false,
            &mut budget,
        )?;
    } else {
        crate::runtime_routes::move_checked(
            state,
            content,
            actor,
            world::TilePos::new(3, 3, 1).center(),
            Facing::NORTH,
            &mut budget,
        )?;
    }
    work.instructions = work
        .instructions
        .checked_add(budget.used)
        .ok_or_else(|| invalid("join instruction overflow"))?;
    live_entity_mut(state, actor)?.info.persistent_id = admission.control.avatar_id.0;
    if april_fools {
        let sum: u32 = admission.portable.name.encode_utf16().map(u32::from).sum();
        let avatar = live_entity_mut(state, actor)?
            .avatar
            .as_mut()
            .expect("created avatar");
        if sum % 4 == 0 {
            avatar
                .write_person_data(63, 50)
                .map_err(|e| invalid(format!("join performance: {e:?}")))?;
        }
        if sum % 128 == 127 {
            avatar
                .write_person_data(63, 2)
                .map_err(|e| invalid(format!("join performance: {e:?}")))?;
        }
    }
    // VMNetSimJoinCmd promotes community donors before portable Apply.
    if let ReplicatedLotPermissions::Initialized(lot) = &state.lot_permissions {
        if lot.is_community()
            && admission.portable.permissions < crate::avatars::state::AvatarPermissions::Owner
            && lot.roommates.contains(&admission.portable.persistent_id)
        {
            admission.portable.permissions = if lot
                .build_roommates
                .contains(&admission.portable.persistent_id)
            {
                crate::avatars::state::AvatarPermissions::BuildBuyRoommate
            } else {
                crate::avatars::state::AvatarPermissions::Roommate
            };
        }
    }
    live_entity_mut(state, actor)?
        .avatar
        .as_mut()
        .expect("created avatar")
        .apply_persistent_state(&admission.portable)
        .map_err(|e| invalid(format!("portable avatar: {e:?}")))?;
    for (pid, values) in &admission.portable.relationships {
        state.relationships.write(
            RelationshipKey {
                owner: RelationshipOwner::Entity(actor),
                target: RelationshipTarget::Persistent(pid.0),
            },
            values.clone(),
        )?;
    }
    // Creation, placement and portable application may have changed native
    // occupied flags, groups and script stacks. Query only at the source point.
    let before = projection(state, content)?;
    let transition = lot_permissions::reconcile_join(
        &before,
        admission.portable.persistent_id,
        admission.portable.permissions,
    )
    .map_err(|e| invalid(format!("join permissions: {e:?}")))?;
    let changed = before.lot != transition.transition.next.lot;
    state.lot_permissions = ReplicatedLotPermissions::Initialized(transition.transition.next.lot);
    for operation in transition.transition.operations {
        match operation {
            LotPermissionOperation::SetDisabledFlags { object_id, flags } => {
                state
                    .entities
                    .get_mut(&object_id)
                    .ok_or_else(|| invalid("permission object disappeared"))?
                    .disabled_flags = flags;
            }
            LotPermissionOperation::RequiredSchedulerInterrupt(object_id) => {
                let target = state
                    .entities
                    .get(&object_id)
                    .ok_or_else(|| invalid("permission object disappeared"))?
                    .info
                    .reference;
                state.scheduler.interrupt(target).map_err(invalid)?;
            }
            LotPermissionOperation::RequiredLightRefresh(object_id) => {
                let target = state
                    .entities
                    .get(&object_id)
                    .ok_or_else(|| invalid("permission object disappeared"))?
                    .info
                    .reference;
                work.events
                    .push(RuntimeEvent::Display(PresentationRequest::Refresh {
                        target,
                        kind: RefreshKind::Light,
                    }));
            }
            _ => {}
        }
    }
    admission.control.actor = actor;
    state.interaction_access.insert(
        actor,
        interaction_adapter::InteractionAccess {
            principal: crate::interactions::PrincipalKey(admission.control.principal),
            allow_hidden: false,
        },
    );
    if let OwnedAdmissionState::Initialized {
        revision,
        permission_revision,
        last_principal,
        actors,
        ..
    } = &mut state.owned_admission
    {
        *revision = revision
            .checked_add(1)
            .ok_or_else(|| invalid("owned registry revision exhausted"))?;
        if changed {
            *permission_revision = permission_revision
                .checked_add(1)
                .ok_or_else(|| invalid("permission revision exhausted"))?;
        }
        *last_principal = admission.control.principal;
        actors.insert(admission.control.principal, admission.control);
    }
    work.events.push(RuntimeEvent::OwnedAvatarJoined {
        actor,
        name: admission.portable.name,
    });
    let controllers: Vec<_> = state
        .entities
        .values()
        .filter(|e| {
            state
                .world
                .object(e.info.reference)
                .is_some_and(|o| o.position.is_out_of_world())
        })
        .map(|e| e.info.reference)
        .collect();
    for controller in controllers {
        let (accepted, suppressed_fault) = execute_named_entrypoint(
            state,
            content,
            work,
            controller,
            "CT - FSO Player Joined",
            vec![actor.object_id.0, 0, 0, 0],
            true,
        )?;
        work.events.push(RuntimeEvent::SourceNamedEntryPoint {
            entity: controller,
            name: "CT - FSO Player Joined".into(),
            accepted,
            suppressed_fault,
        });
    }
    work.strict_source = old_strict;
    crate::runtime_memory::sync_projection(state, actor)?;
    state.owned_admission.validate(state).map_err(invalid)?;
    Ok(())
}
